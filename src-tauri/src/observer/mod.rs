use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

pub(crate) mod commands;
mod ipc;
mod runtime;
mod selection;
mod session;

use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use walkdir::WalkDir;

use crate::local_trust::owned_regular_metadata;

#[cfg(test)]
use self::commands::{
    dispatch_steering, dispatch_stop, is_stale_client_error, refresh_and_retry_follow_up,
};
use self::ipc::IpcCommand;
pub use self::runtime::RuntimeSnapshot;
#[cfg(test)]
use self::runtime::{
    ActionKind, LifecycleEvent, LifecycleSource, TargetRefresh, ThreadContext, ThreadLifecycle,
    ThreadRecord, apply_context_event, authorize_action, build_runtime_snapshot,
    complete_control_action, control_snapshot, is_terminal_state, reduce_lifecycle,
};
use self::runtime::{
    RuntimeEvent, RuntimeState, SessionContextEvent, ThreadControl, compact_tail_text,
    compact_text, hash_id, snapshot_can_refresh_target,
};
use self::selection::AppLogSelectionAdapter;

const ACTIVE_WINDOW: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct RuntimeHandle {
    state: Arc<Mutex<RuntimeState>>,
    ipc: Arc<Mutex<Option<mpsc::UnboundedSender<IpcCommand>>>>,
    selection: Arc<Mutex<Option<AppLogSelectionAdapter>>>,
}

impl Default for RuntimeHandle {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeState {
                selected: None,
                threads: HashMap::new(),
                snapshot: RuntimeSnapshot {
                    state: "disconnected".into(),
                    connected: false,
                    thread_id_hash: None,
                    epoch: 0,
                    current_question: None,
                    task_summary: None,
                    latest_update: None,
                },
                follow_up_inflight: HashSet::new(),
                dismissed: HashSet::new(),
                ipc_connected: false,
            })),
            ipc: Arc::new(Mutex::new(None)),
            selection: Arc::new(Mutex::new(AppLogSelectionAdapter::from_default_paths())),
        }
    }
}

pub fn start(app: AppHandle, runtime: RuntimeHandle) {
    let (sender, receiver) = mpsc::unbounded_channel();
    *runtime.ipc.lock().expect("IPC sender poisoned") = Some(sender);
    let ipc_app = app.clone();
    let ipc_runtime = runtime.clone();
    tauri::async_runtime::spawn(async move { ipc::run(ipc_app, ipc_runtime, receiver).await });

    let session_app = app.clone();
    let session_runtime = runtime.clone();
    tauri::async_runtime::spawn(async move { session::run(session_app, session_runtime).await });

    tauri::async_runtime::spawn(async move { selection::run(app, runtime).await });
}

fn apply_runtime_event(
    app: &AppHandle,
    runtime: &RuntimeHandle,
    event: RuntimeEvent,
    _source: &str,
) {
    let effects = runtime
        .state
        .lock()
        .expect("runtime state poisoned")
        .reduce(event);
    #[cfg(debug_assertions)]
    if let Some(snapshot) = effects.snapshot.as_ref() {
        eprintln!(
            "pet-state state={} source={} thread={}",
            snapshot.state,
            _source,
            snapshot.thread_id_hash.as_deref().unwrap_or("none")
        );
    }
    if let Some(snapshot) = effects.snapshot {
        let _ = app.emit("pet-state", snapshot);
    }
    if let Some(control) = effects.control {
        let _ = app.emit("control-state", control);
    }
}

fn select_thread(app: &AppHandle, runtime: &RuntimeHandle, thread: String, source: &str) {
    apply_runtime_event(app, runtime, RuntimeEvent::Select { thread }, source);
}

fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|path| path.join(".codex")))
}

fn recent_files(root: &Path, extension: &str) -> Vec<PathBuf> {
    let cutoff = SystemTime::now()
        .checked_sub(ACTIVE_WINDOW)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().extension().and_then(|value| value.to_str()) == Some(extension)
        })
        .filter_map(|entry| {
            let modified = owned_regular_metadata(entry.path()).ok()?.modified().ok()?;
            (modified >= cutoff).then(|| entry.path().to_path_buf())
        })
        .collect();
    files.sort_by_key(|path| {
        owned_regular_metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    files
}

#[cfg(test)]
mod tests;
