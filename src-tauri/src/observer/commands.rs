use std::{collections::HashMap, time::Duration};

use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::time::{Instant, sleep};

use crate::control::{ControlSnapshot, ControlTarget, build_control_request, build_follow_up};

use super::RuntimeHandle;

const TARGET_REFRESH_TIMEOUT: Duration = Duration::from_secs(3);
const TARGET_REFRESH_POLL: Duration = Duration::from_millis(40);
use super::runtime::{
    ActionKind, RuntimeSnapshot, RuntimeState, TargetRefresh, authorize_action,
    complete_control_action, control_snapshot,
};
use super::selection::refresh_foreground_selection;

struct FollowUpInflightGuard {
    runtime: RuntimeHandle,
    thread: String,
}

impl Drop for FollowUpInflightGuard {
    fn drop(&mut self) {
        self.runtime
            .state
            .lock()
            .expect("runtime state poisoned")
            .follow_up_inflight
            .remove(&self.thread);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlActionInput {
    pub action_id: String,
    pub action: String,
    #[serde(default)]
    pub answers: HashMap<String, Vec<String>>,
}

#[tauri::command]
pub fn get_runtime_state(runtime: tauri::State<'_, RuntimeHandle>) -> RuntimeSnapshot {
    runtime
        .state
        .lock()
        .expect("runtime state poisoned")
        .snapshot
        .clone()
}

fn emit_control_snapshot(app: &AppHandle, state: &RuntimeState) {
    let _ = app.emit("control-state", control_snapshot(state));
}

#[tauri::command]
pub fn get_control_state(runtime: tauri::State<'_, RuntimeHandle>) -> ControlSnapshot {
    let state = runtime.state.lock().expect("runtime state poisoned");
    control_snapshot(&state)
}

#[tauri::command]
pub async fn perform_control_action(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
    input: ControlActionInput,
) -> Result<(), String> {
    let pending = {
        let state = runtime.state.lock().expect("runtime state poisoned");
        authorize_action(&state, ActionKind::Respond(&input.action_id))?
            .pending
            .expect("response authorization must include a pending request")
    };
    let (method, params) = build_control_request(&pending, &input.action, &input.answers)?;
    runtime
        .request(method, params, Some(pending.owner_client_id.clone()))
        .await?;
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    complete_control_action(&mut state, &pending);
    emit_control_snapshot(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn dismiss_control_notification(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
    action_id: String,
) {
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.dismissed.insert(action_id);
    emit_control_snapshot(&app, &state);
}

#[tauri::command]
pub async fn stop_current_task(runtime: tauri::State<'_, RuntimeHandle>) -> Result<(), String> {
    let target = {
        let state = runtime.state.lock().expect("runtime state poisoned");
        authorize_action(&state, ActionKind::Stop)?.target
    };
    dispatch_stop(runtime.inner(), &target).await
}

pub(super) async fn dispatch_stop(
    runtime: &RuntimeHandle,
    target: &ControlTarget,
) -> Result<(), String> {
    runtime
        .request(
            "thread-follower-interrupt-turn".into(),
            json!({ "conversationId": target.conversation_id }),
            Some(target.owner_client_id.clone()),
        )
        .await?;
    Ok(())
}

pub(super) fn is_stale_client_error(error: &str) -> bool {
    let error = error.trim().to_ascii_lowercase().replace(['-', '_'], " ");
    error == "not found"
        || error.contains("no client found")
        || error.contains("client not found")
        || error.contains("thread not found")
        || error.contains("conversation not found")
}

fn mark_target_stale(state: &mut RuntimeState, thread: &str, target: &ControlTarget) {
    let Some(record) = state.threads.get_mut(thread) else {
        return;
    };
    if let Some(control) = record.control.as_mut()
        && control.target.owner_client_id == target.owner_client_id
    {
        control.stale = true;
    }
    if let Some(host_id) = target.host_id.clone() {
        record.target_refresh = Some(TargetRefresh {
            stale_owner_client_id: target.owner_client_id.clone(),
            host_id,
        });
    }
}

pub(super) async fn dispatch_steering(
    runtime: &RuntimeHandle,
    target: &ControlTarget,
    prompt: &str,
) -> Result<(), String> {
    let (steer_method, steer_params) = build_follow_up(target, prompt)?;
    runtime
        .request(
            steer_method,
            steer_params,
            Some(target.owner_client_id.clone()),
        )
        .await
        .map(|_| ())
}

pub(super) async fn refresh_and_retry_follow_up(
    runtime: &RuntimeHandle,
    thread: &str,
    stale_target: &ControlTarget,
    prompt: &str,
) -> Result<(), String> {
    runtime.refresh_following(stale_target).await?;
    let target = wait_for_refreshed_follow_up(runtime, thread, stale_target).await?;
    dispatch_steering(runtime, &target, prompt).await
}

async fn wait_for_refreshed_follow_up(
    runtime: &RuntimeHandle,
    thread: &str,
    stale_target: &ControlTarget,
) -> Result<ControlTarget, String> {
    let deadline = Instant::now() + TARGET_REFRESH_TIMEOUT;
    loop {
        {
            let state = runtime.state.lock().expect("runtime state poisoned");
            if state.selected.as_deref() != Some(thread) {
                return Err("The selected Codex task changed while reconnecting".to_owned());
            }
            match authorize_action(&state, ActionKind::Steer) {
                Ok(authorized) => {
                    let target = authorized.target;
                    if target.conversation_id != stale_target.conversation_id
                        || target.host_id != stale_target.host_id
                        || target.owner_client_id == stale_target.owner_client_id
                    {
                        return Err("The refreshed Codex task owner did not match".to_owned());
                    }
                    return Ok(target);
                }
                Err(error) => {
                    let still_waiting = state.ipc_connected
                        && state.threads.get(thread).is_some_and(|record| {
                            !record.lifecycle.terminal
                                && record.lifecycle.state.as_deref() == Some("working")
                                && record.control.as_ref().is_none_or(|control| control.stale)
                        });
                    if !still_waiting {
                        return Err(error);
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(
                "Codex task owner is reconnecting. Focus the task in Codex, then retry.".to_owned(),
            );
        }
        sleep(TARGET_REFRESH_POLL).await;
    }
}

#[tauri::command]
pub async fn send_follow_up(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
    prompt: String,
) -> Result<(), String> {
    refresh_foreground_selection(&app, runtime.inner()).await?;
    let (thread, target) = {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        let authorized = authorize_action(&state, ActionKind::Steer)?;
        if !state.follow_up_inflight.insert(authorized.thread.clone()) {
            return Err("A reply is already being sent".to_owned());
        }
        (authorized.thread, authorized.target)
    };
    let _inflight = FollowUpInflightGuard {
        runtime: runtime.inner().clone(),
        thread: thread.clone(),
    };
    match dispatch_steering(runtime.inner(), &target, &prompt).await {
        Err(error) if is_stale_client_error(&error) => {
            {
                let mut state = runtime.state.lock().expect("runtime state poisoned");
                mark_target_stale(&mut state, &thread, &target);
                emit_control_snapshot(&app, &state);
            }
            refresh_and_retry_follow_up(runtime.inner(), &thread, &target, &prompt).await
        }
        result => result,
    }
}
