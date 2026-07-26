use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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

use crate::{
    cdp::{CdpEndpoint, ControlTransport},
    control::ControlTarget,
    local_trust::owned_regular_metadata,
};

#[cfg(test)]
use self::commands::{
    OWNER_UNAVAILABLE_MESSAGE, cdp_verification_within_deadline, dispatch_ready_follow_up,
    dispatch_steering, dispatch_stop, follow_up_error_class, is_stale_client_error,
    mark_target_stale, poll_cdp_verification_until_ready,
    prepare_follow_up_after_foreground_selection, prepare_follow_up_attempt,
    reauthorize_refreshed_follow_up, refresh_and_retry_follow_up, release_follow_up_inflight,
};
use self::ipc::IpcCommand;
pub use self::runtime::RuntimeSnapshot;
#[cfg(test)]
use self::runtime::{
    ActionKind, FollowUpDispatchGuard, FollowUpKind, LifecycleEvent, LifecycleSource,
    TargetRefresh, TargetRefreshPhase, ThreadContext, ThreadLifecycle, ThreadRecord,
    apply_context_event, arm_target_refresh, authorize_action, authorize_cdp_follow_up,
    authorize_follow_up, authorize_ipc_follow_up_dispatch, build_runtime_snapshot,
    complete_control_action, control_snapshot, is_terminal_state, reduce_lifecycle,
};
use self::runtime::{
    RuntimeEvent, RuntimeState, SessionContextEvent, ThreadControl, compact_tail_text,
    compact_text, hash_id, snapshot_can_refresh_target,
};
use self::selection::AppLogSelectionAdapter;

const ACTIVE_WINDOW: Duration = Duration::from_secs(30 * 60);

struct CdpProcessLiveness {
    process_id: u32,
    token: u128,
    alive: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct RuntimeHandle {
    state: Arc<Mutex<RuntimeState>>,
    ipc: Arc<Mutex<Option<mpsc::UnboundedSender<IpcCommand>>>>,
    selection: Arc<Mutex<Option<AppLogSelectionAdapter>>>,
    cdp_launch_inflight: Arc<Mutex<bool>>,
    cdp_process_liveness: Arc<Mutex<Option<CdpProcessLiveness>>>,
    cdp_tracked_endpoint: Arc<Mutex<Option<CdpEndpoint>>>,
}

impl RuntimeHandle {
    fn new() -> Self {
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
                follow_up_inflight: HashMap::new(),
                follow_up_next_token: 0,
                dismissed: HashSet::new(),
                ipc_connected: false,
                control_transport: ControlTransport::IpcOnly,
                cdp_port: None,
                cdp_process_id: None,
                transport_generation: 0,
            })),
            ipc: Arc::new(Mutex::new(None)),
            selection: Arc::new(Mutex::new(AppLogSelectionAdapter::from_default_paths())),
            cdp_launch_inflight: Arc::new(Mutex::new(false)),
            cdp_process_liveness: Arc::new(Mutex::new(None)),
            cdp_tracked_endpoint: Arc::new(Mutex::new(None)),
        }
    }

    pub(super) fn remember_cdp_endpoint(&self, endpoint: CdpEndpoint) {
        *self
            .cdp_tracked_endpoint
            .lock()
            .expect("CDP tracked endpoint poisoned") = Some(endpoint);
    }

    fn cdp_endpoint_matches(&self, endpoint: CdpEndpoint) -> bool {
        self.cdp_tracked_endpoint
            .lock()
            .expect("CDP tracked endpoint poisoned")
            .as_ref()
            .is_some_and(|current| *current == endpoint)
    }

    pub(super) fn cdp_endpoint_for_retry(&self) -> Result<CdpEndpoint, String> {
        let endpoint = self
            .cdp_tracked_endpoint
            .lock()
            .expect("CDP tracked endpoint poisoned")
            .as_ref()
            .copied()
            .ok_or_else(|| {
                "The CoPets bridge cannot be retried. Connect or launch Codex with the bridge again."
                    .to_owned()
            })?;
        let retryable = {
            let state = self.state.lock().expect("runtime state poisoned");
            state.control_transport == ControlTransport::CdpDegraded
                && state.cdp_port.is_none()
                && state.cdp_process_id == Some(endpoint.process_id)
        };
        if !retryable || !self.cdp_process_is_live(endpoint.process_id) {
            return Err(
                "The CoPets bridge cannot be retried. Connect or launch Codex with the bridge again."
                    .to_owned(),
            );
        }
        Ok(endpoint)
    }

    pub(super) fn has_tracked_cdp_endpoint(&self) -> bool {
        self.cdp_tracked_endpoint
            .lock()
            .expect("CDP tracked endpoint poisoned")
            .is_some()
    }

    fn clear_cdp_endpoint(&self, endpoint: CdpEndpoint) {
        let mut tracked_endpoint = self
            .cdp_tracked_endpoint
            .lock()
            .expect("CDP tracked endpoint poisoned");
        if tracked_endpoint
            .as_ref()
            .is_some_and(|current| *current == endpoint)
        {
            *tracked_endpoint = None;
        }
    }

    pub(super) fn arm_cdp_process_liveness(&self, process_id: u32) -> u128 {
        let token = uuid::Uuid::new_v4().as_u128();
        let mut liveness = self
            .cdp_process_liveness
            .lock()
            .expect("CDP process liveness poisoned");
        *liveness = Some(CdpProcessLiveness {
            process_id,
            token,
            alive: Arc::new(AtomicBool::new(true)),
        });
        token
    }

    pub(super) fn cdp_liveness_token_for_endpoint(&self, endpoint: CdpEndpoint) -> Option<u128> {
        if !self.cdp_endpoint_matches(endpoint) {
            return None;
        }
        self.cdp_process_liveness
            .lock()
            .expect("CDP process liveness poisoned")
            .as_ref()
            .filter(|liveness| {
                liveness.process_id == endpoint.process_id && liveness.alive.load(Ordering::Acquire)
            })
            .map(|liveness| liveness.token)
    }

    fn cdp_process_is_live(&self, process_id: u32) -> bool {
        self.cdp_process_liveness
            .lock()
            .expect("CDP process liveness poisoned")
            .as_ref()
            .is_some_and(|liveness| {
                liveness.process_id == process_id && liveness.alive.load(Ordering::Acquire)
            })
    }

    fn revoke_cdp_process_liveness(&self, process_id: u32, token: u128) -> bool {
        let mut liveness = self
            .cdp_process_liveness
            .lock()
            .expect("CDP process liveness poisoned");
        let matches = liveness
            .as_ref()
            .is_some_and(|current| current.process_id == process_id && current.token == token);
        if matches {
            liveness
                .as_ref()
                .expect("matching process liveness must exist")
                .alive
                .store(false, Ordering::Release);
            *liveness = None;
        }
        matches
    }

    fn cdp_endpoint_for_attempt(
        &self,
        expected_generation: u64,
        expected_thread: &str,
        expected_target: &ControlTarget,
        operation: &str,
    ) -> Result<CdpEndpoint, String> {
        let (port, process_id) = {
            let state = self.state.lock().expect("runtime state poisoned");
            let expected_kind = match operation {
                "steer-turn-for-host" => runtime::FollowUpKind::Steer,
                "send-follow-up-message" => runtime::FollowUpKind::Start,
                _ => {
                    return Err(
                        "The CoPets bridge rejected an unknown follow-up operation".to_owned()
                    );
                }
            };
            if state.control_transport != ControlTransport::CdpReady
                || state.transport_generation != expected_generation
            {
                return Err(
                    "The selected Codex task changed before this follow-up could send".to_owned(),
                );
            }
            let (authorized, kind) = runtime::authorize_cdp_follow_up(&state).map_err(|_| {
                "The selected Codex task changed before this follow-up could send".to_owned()
            })?;
            if kind != expected_kind
                || authorized.thread != expected_thread
                || authorized.target != *expected_target
            {
                return Err(
                    "The selected Codex task changed before this follow-up could send".to_owned(),
                );
            }
            let port = state
                .cdp_port
                .ok_or_else(|| "The CoPets bridge is unavailable".to_owned())?;
            let process_id = state
                .cdp_process_id
                .ok_or_else(|| "The CoPets bridge is unavailable".to_owned())?;
            (port, process_id)
        };
        let endpoint = self
            .cdp_tracked_endpoint
            .lock()
            .expect("CDP tracked endpoint poisoned")
            .as_ref()
            .copied()
            .filter(|endpoint| endpoint.process_id == process_id && endpoint.port == port)
            .ok_or_else(|| "The CoPets bridge is unavailable".to_owned())?;
        if !self.cdp_process_is_live(endpoint.process_id) {
            return Err("The CoPets bridge is unavailable".to_owned());
        }
        if !self.cdp_endpoint_matches(endpoint) {
            return Err("The CoPets bridge is unavailable".to_owned());
        }
        Ok(endpoint)
    }

    pub(super) async fn cdp_rf_call(
        &self,
        expected_generation: u64,
        expected_thread: &str,
        expected_target: &ControlTarget,
        operation: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        let endpoint = self.cdp_endpoint_for_attempt(
            expected_generation,
            expected_thread,
            expected_target,
            operation,
        )?;
        crate::cdp::verify_tracked_listener(endpoint).await?;
        let final_endpoint = self.cdp_endpoint_for_attempt(
            expected_generation,
            expected_thread,
            expected_target,
            operation,
        )?;
        if final_endpoint != endpoint {
            return Err(
                "The selected Codex task changed before this follow-up could send".to_owned(),
            );
        }
        crate::cdp::call_rf(endpoint.port, operation, params).await
    }
}

impl Default for RuntimeHandle {
    fn default() -> Self {
        Self::new()
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

fn emit_runtime_effects(app: &AppHandle, effects: runtime::RuntimeEffects, _source: &str) {
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
    emit_runtime_effects(app, effects, _source);
}

pub(super) fn set_control_transport(
    app: &AppHandle,
    runtime: &RuntimeHandle,
    transport: ControlTransport,
    port: Option<u16>,
    process_id: Option<u32>,
) {
    apply_runtime_event(
        app,
        runtime,
        RuntimeEvent::ControlTransport {
            transport,
            port,
            process_id,
        },
        "cdp-bridge",
    );
}

pub(super) fn set_cdp_ready(
    app: &AppHandle,
    runtime: &RuntimeHandle,
    endpoint: CdpEndpoint,
) -> bool {
    if !runtime.cdp_process_is_live(endpoint.process_id) || !runtime.cdp_endpoint_matches(endpoint)
    {
        return false;
    }
    let effects = {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        if state.control_transport != ControlTransport::CdpDegraded
            || state.cdp_process_id != Some(endpoint.process_id)
        {
            return false;
        }
        state.reduce(RuntimeEvent::ControlTransport {
            transport: ControlTransport::CdpReady,
            port: Some(endpoint.port),
            process_id: Some(endpoint.process_id),
        })
    };
    emit_runtime_effects(app, effects, "cdp-bridge");
    true
}

pub(super) fn clear_cdp_process(
    app: &AppHandle,
    runtime: &RuntimeHandle,
    endpoint: CdpEndpoint,
    liveness_token: u128,
) {
    if !runtime.revoke_cdp_process_liveness(endpoint.process_id, liveness_token) {
        return;
    }
    runtime.clear_cdp_endpoint(endpoint);
    apply_runtime_event(
        app,
        runtime,
        RuntimeEvent::CdpProcessExited {
            process_id: endpoint.process_id,
        },
        "cdp-bridge",
    );
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
