use std::{collections::HashMap, time::Duration};

use serde_json::{Value, json};
use tauri::AppHandle;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    sync::{mpsc, oneshot},
    time::sleep,
};

use crate::control::{ControlNotification, ControlTarget, parse_controls};
use crate::local_trust::{validate_owned_socket, validate_same_user_peer};

use super::runtime::{
    FollowUpDispatchGuard, RuntimeState, arm_target_refresh, authorize_ipc_follow_up_dispatch,
};
use super::{
    RuntimeEvent, RuntimeHandle, ThreadControl, apply_runtime_event, codex_home, hash_id,
    snapshot_can_refresh_target,
};

// Complete follower snapshots can exceed 32 MiB on long-running Codex tasks.
const MAX_FRAME: usize = 128 * 1024 * 1024;
const IPC_CLIENT_TYPE: &str = "codex-pet-sidecar";

pub(super) enum IpcCommand {
    Request {
        method: String,
        params: Value,
        version: u64,
        target_client_id: Option<String>,
        follow_up_guard: Option<FollowUpDispatchGuard>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Broadcast {
        method: String,
        params: Value,
        version: u64,
        follow_refresh: Option<FollowRefresh>,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Clone)]
pub(super) struct FollowRefresh {
    pub(super) thread: String,
    pub(super) target: ControlTarget,
}

struct PendingIpcResponse {
    method: String,
    target_client_id: Option<String>,
    reply: oneshot::Sender<Result<Value, String>>,
}

impl RuntimeHandle {
    pub(super) async fn request(
        &self,
        method: String,
        params: Value,
        target_client_id: Option<String>,
    ) -> Result<Value, String> {
        self.enqueue_request(method, params, target_client_id, None)
            .await
    }

    pub(super) async fn request_follow_up(
        &self,
        method: String,
        params: Value,
        guard: FollowUpDispatchGuard,
    ) -> Result<Value, String> {
        {
            let state = self.state.lock().expect("runtime state poisoned");
            authorize_ipc_follow_up_dispatch(&state, &guard)?;
        }
        self.enqueue_request(
            method,
            params,
            Some(guard.target.owner_client_id.clone()),
            Some(guard),
        )
        .await
    }

    async fn enqueue_request(
        &self,
        method: String,
        params: Value,
        target_client_id: Option<String>,
        follow_up_guard: Option<FollowUpDispatchGuard>,
    ) -> Result<Value, String> {
        let sender = self
            .ipc
            .lock()
            .expect("IPC sender poisoned")
            .clone()
            .ok_or_else(|| "Codex control channel is not connected".to_owned())?;
        let (reply, response) = oneshot::channel();
        sender
            .send(IpcCommand::Request {
                method,
                params,
                version: 0,
                target_client_id,
                follow_up_guard,
                reply,
            })
            .map_err(|_| "Codex control channel closed".to_owned())?;
        tokio::time::timeout(Duration::from_secs(20), response)
            .await
            .map_err(|_| "Codex control request timed out".to_owned())?
            .map_err(|_| "Codex control response was dropped".to_owned())?
    }

    async fn broadcast(
        &self,
        method: String,
        params: Value,
        version: u64,
        follow_refresh: Option<FollowRefresh>,
    ) -> Result<(), String> {
        let sender = self
            .ipc
            .lock()
            .expect("IPC sender poisoned")
            .clone()
            .ok_or_else(|| "Codex control channel is not connected".to_owned())?;
        let (reply, response) = oneshot::channel();
        sender
            .send(IpcCommand::Broadcast {
                method,
                params,
                version,
                follow_refresh,
                reply,
            })
            .map_err(|_| "Codex control channel closed".to_owned())?;
        tokio::time::timeout(Duration::from_secs(3), response)
            .await
            .map_err(|_| "Codex follow refresh timed out".to_owned())?
            .map_err(|_| "Codex follow refresh was dropped".to_owned())?
    }

    pub(super) async fn refresh_following(
        &self,
        thread: &str,
        target: &ControlTarget,
    ) -> Result<(), String> {
        let host_id = target
            .host_id
            .as_deref()
            .ok_or_else(|| "Codex task owner cannot be refreshed yet".to_owned())?;
        self.broadcast(
            "thread-stream-following-changed".into(),
            json!({
                "conversationId": target.conversation_id,
                "hostId": host_id,
                "following": true
            }),
            1,
            Some(FollowRefresh {
                thread: thread.to_owned(),
                target: target.clone(),
            }),
        )
        .await
    }
}

fn following_envelope(client_id: &str, target: &ControlTarget) -> Option<Value> {
    let host_id = target.host_id.as_deref()?;
    Some(json!({
        "type": "broadcast",
        "sourceClientId": client_id,
        "version": 1,
        "method": "thread-stream-following-changed",
        "params": {
            "conversationId": target.conversation_id,
            "hostId": host_id,
            "following": true
        }
    }))
}

fn remember_followed_target(followed: &mut HashMap<String, String>, target: &ControlTarget) {
    if let Some(host_id) = target.host_id.as_deref() {
        followed.insert(target.conversation_id.clone(), host_id.to_owned());
    }
}

fn arm_written_follow_refresh(
    state: &mut RuntimeState,
    followed: &mut HashMap<String, String>,
    refresh: &FollowRefresh,
) -> Result<(), String> {
    arm_target_refresh(state, &refresh.thread, &refresh.target)?;
    remember_followed_target(followed, &refresh.target);
    Ok(())
}

fn snapshot_host_id(
    params: &Value,
    thread: &str,
    followed: &HashMap<String, String>,
) -> Option<String> {
    params
        .get("hostId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| followed.get(thread).cloned())
}

fn retained_follow_targets(runtime: &RuntimeHandle) -> Vec<ControlTarget> {
    runtime
        .state
        .lock()
        .expect("runtime state poisoned")
        .threads
        .values()
        .filter_map(|record| record.control.as_ref())
        .filter(|control| control.target.host_id.is_some())
        .map(|control| control.target.clone())
        .collect()
}

fn remembered_follow_target(
    runtime: &RuntimeHandle,
    thread: &str,
    host_id: &str,
) -> Option<ControlTarget> {
    runtime
        .state
        .lock()
        .expect("runtime state poisoned")
        .threads
        .get(&hash_id(thread))
        .and_then(|record| record.control.as_ref())
        .filter(|control| {
            control.target.conversation_id == thread
                && control.target.host_id.as_deref() == Some(host_id)
        })
        .map(|control| control.target.clone())
}

fn following_status_response(
    runtime: &RuntimeHandle,
    thread: &str,
    host_id: &str,
    client_id: &str,
    followed: &mut HashMap<String, String>,
) -> Option<Value> {
    let target = remembered_follow_target(runtime, thread, host_id)?;
    let response = following_envelope(client_id, &target)?;
    remember_followed_target(followed, &target);
    Some(response)
}

async fn write_frame<W: AsyncWrite + Unpin>(
    stream: &mut W,
    message: &Value,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    stream.write_all(&(body.len() as u32).to_le_bytes()).await?;
    stream.write_all(&body).await
}

async fn read_frame<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Value, String> {
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .await
        .map_err(|error| error.to_string())?;
    let length = u32::from_le_bytes(length) as usize;
    validate_frame_length(length)?;
    let mut body = vec![0_u8; length];
    stream
        .read_exact(&mut body)
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

fn validate_frame_length(length: usize) -> Result<(), String> {
    if length == 0 || length > MAX_FRAME {
        return Err(format!("invalid IPC frame length: {length}"));
    }
    Ok(())
}

fn set_ipc_connected(app: &AppHandle, runtime: &RuntimeHandle, connected: bool) {
    apply_runtime_event(
        app,
        runtime,
        RuntimeEvent::IpcConnectivity { connected },
        "codex-app-ipc",
    );
}

fn fail_command(command: IpcCommand, error: &str) {
    match command {
        IpcCommand::Request { reply, .. } => {
            let _ = reply.send(Err(error.into()));
        }
        IpcCommand::Broadcast { reply, .. } => {
            let _ = reply.send(Err(error.into()));
        }
    }
}

fn decode_ipc_response(message: &Value, pending: &PendingIpcResponse) -> Result<Value, String> {
    if message.get("resultType").and_then(Value::as_str) != Some("success") {
        return Err(message
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Codex rejected the control request")
            .to_owned());
    }
    if message.get("method").and_then(Value::as_str) != Some(pending.method.as_str()) {
        return Err("Codex IPC response method mismatch".into());
    }
    if let Some(expected_owner) = pending.target_client_id.as_deref() {
        let actual_owner = message
            .get("handledByClientId")
            .or_else(|| message.get("sourceClientId"))
            .and_then(Value::as_str);
        if actual_owner != Some(expected_owner) {
            return Err("Codex IPC response owner mismatch".into());
        }
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

fn resolve_pending_response(
    message: &Value,
    pending: &PendingIpcResponse,
) -> Option<Result<Value, String>> {
    Some(decode_ipc_response(message, pending))
}

async fn wait_before_reconnect(receiver: &mut mpsc::UnboundedReceiver<IpcCommand>) {
    tokio::select! {
        _ = sleep(Duration::from_secs(2)) => {}
        command = receiver.recv() => {
            if let Some(command) = command {
                fail_command(command, "Codex IPC is disconnected");
            }
        }
    }
}

fn initialize_request(request_id: String) -> Value {
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": "initializing-client",
        "version": 0,
        "method": "initialize",
        "params": { "clientType": IPC_CLIENT_TYPE }
    })
}

pub(super) async fn run(
    app: AppHandle,
    runtime: RuntimeHandle,
    mut receiver: mpsc::UnboundedReceiver<IpcCommand>,
) {
    let Some(socket) = codex_home().map(|home| home.join("ipc/ipc.sock")) else {
        return;
    };
    loop {
        if validate_owned_socket(&socket).is_err() {
            wait_before_reconnect(&mut receiver).await;
            continue;
        }
        if let Ok(stream) = UnixStream::connect(&socket).await {
            if validate_same_user_peer(&stream).is_err() {
                wait_before_reconnect(&mut receiver).await;
                continue;
            }
            let (mut reader, mut writer) = stream.into_split();
            let request_id = uuid::Uuid::new_v4().to_string();
            let initialize = initialize_request(request_id);
            if write_frame(&mut writer, &initialize).await.is_err() {
                wait_before_reconnect(&mut receiver).await;
                continue;
            }
            let mut client_id = "initializing-client".to_owned();
            let mut pending: HashMap<String, PendingIpcResponse> = HashMap::new();
            let mut followed = HashMap::new();
            'connection: loop {
                tokio::select! {
                    message = read_frame(&mut reader) => {
                        let Ok(message) = message else { break };
                        match message.get("type").and_then(Value::as_str) {
                            Some("response") => {
                                let response_id = message
                                    .get("requestId")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default();
                                if message.get("method").and_then(Value::as_str) == Some("initialize")
                                    && message.get("resultType").and_then(Value::as_str) == Some("success")
                                {
                                    if let Some(id) = message
                                        .get("result")
                                        .and_then(|result| result.get("clientId"))
                                        .and_then(Value::as_str)
                                    {
                                        client_id = id.to_owned();
                                        set_ipc_connected(&app, &runtime, true);
                                        for target in retained_follow_targets(&runtime) {
                                            let Some(envelope) = following_envelope(&client_id, &target) else {
                                                continue;
                                            };
                                            if write_frame(&mut writer, &envelope).await.is_err() {
                                                break 'connection;
                                            }
                                            remember_followed_target(&mut followed, &target);
                                        }
                                    }
                                } else if let Some(result) = pending
                                    .get(response_id)
                                    .and_then(|pending_response| resolve_pending_response(&message, pending_response))
                                {
                                    if let Some(pending_response) = pending.remove(response_id) {
                                        let _ = pending_response.reply.send(result);
                                    }
                                }
                            }
                            Some("broadcast") => {
                                for response in handle_broadcast(
                                    &app,
                                    &runtime,
                                    &message,
                                    &client_id,
                                    &mut followed,
                                ) {
                                    if write_frame(&mut writer, &response).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Some("client-discovery-request") => {
                                let response = json!({
                                    "type": "client-discovery-response",
                                    "requestId": message.get("requestId"),
                                    "response": { "canHandle": false }
                                });
                                if write_frame(&mut writer, &response).await.is_err() {
                                    break;
                                }
                            }
                            Some("request") => {
                                let response = json!({
                                    "type": "response",
                                    "requestId": message.get("requestId"),
                                    "resultType": "error",
                                    "error": "unsupported-sidecar-target"
                                });
                                if write_frame(&mut writer, &response).await.is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    command = receiver.recv() => {
                        let Some(command) = command else { return };
                        match command {
                            IpcCommand::Request { method, params, version, target_client_id, follow_up_guard, reply } => {
                                if client_id == "initializing-client" {
                                    let _ = reply.send(Err("Codex IPC is still initializing".into()));
                                    continue;
                                }
                                if let Some(guard) = follow_up_guard.as_ref() {
                                    let authorized = {
                                        let state = runtime.state.lock().expect("runtime state poisoned");
                                        authorize_ipc_follow_up_dispatch(&state, guard)
                                    };
                                    if let Err(error) = authorized {
                                        let _ = reply.send(Err(error));
                                        continue;
                                    }
                                }
                                let request_id = uuid::Uuid::new_v4().to_string();
                                let expected_method = method.clone();
                                let expected_target = target_client_id.clone();
                                let mut envelope = json!({
                                    "type": "request",
                                    "requestId": request_id,
                                    "sourceClientId": client_id,
                                    "version": version,
                                    "method": method,
                                    "params": params,
                                });
                                if let Some(target) = target_client_id {
                                    envelope["targetClientId"] = json!(target);
                                }
                                pending.insert(request_id.clone(), PendingIpcResponse {
                                    method: expected_method,
                                    target_client_id: expected_target,
                                    reply,
                                });
                                if let Err(error) = write_frame(&mut writer, &envelope).await {
                                    if let Some(pending_response) = pending.remove(&request_id) {
                                        let _ = pending_response.reply.send(Err(error.to_string()));
                                    }
                                    break;
                                }
                            }
                            IpcCommand::Broadcast { method, params, version, follow_refresh, reply } => {
                                if client_id == "initializing-client" {
                                    let _ = reply.send(Err("Codex IPC is still initializing".into()));
                                    continue;
                                }
                                let envelope = json!({
                                    "type": "broadcast",
                                    "sourceClientId": client_id,
                                    "version": version,
                                    "method": method,
                                    "params": params,
                                });
                                match write_frame(&mut writer, &envelope).await {
                                    Ok(()) => {
                                        let result = if let Some(follow_refresh) = follow_refresh {
                                            arm_written_follow_refresh(
                                                &mut runtime.state.lock().expect("runtime state poisoned"),
                                                &mut followed,
                                                &follow_refresh,
                                            )
                                        } else {
                                            Ok(())
                                        };
                                        let _ = reply.send(result);
                                    }
                                    Err(error) => {
                                        let _ = reply.send(Err(error.to_string()));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (_, pending_response) in pending.drain() {
                let _ = pending_response
                    .reply
                    .send(Err("Codex IPC disconnected".into()));
            }
            set_ipc_connected(&app, &runtime, false);
        }
        wait_before_reconnect(&mut receiver).await;
    }
}

fn patches_touch_requests(change: &Value) -> bool {
    change
        .get("patches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|patch| {
            patch
                .get("path")
                .and_then(Value::as_array)
                .is_some_and(|path| path.iter().any(|part| part.as_str() == Some("requests")))
        })
}

fn state_from_conversation(
    state: &Value,
    notifications: &[ControlNotification],
) -> Option<&'static str> {
    if !notifications.is_empty() {
        return Some("working");
    }
    let runtime = state.get("threadRuntimeStatus").unwrap_or(&Value::Null);
    match runtime.get("type").and_then(Value::as_str) {
        Some("active") => return Some("working"),
        Some("idle") => return Some("completed"),
        Some("systemError") => return Some("failed"),
        _ => {}
    }
    let status = state
        .get("turns")
        .and_then(Value::as_array)
        .and_then(|turns| turns.last())
        .and_then(|turn| turn.get("status"))
        .and_then(Value::as_str)?;
    match status {
        "inProgress" | "in_progress" | "active" => Some("working"),
        "completed" => Some("completed"),
        "interrupted" | "cancelled" => Some("interrupted"),
        "failed" => Some("failed"),
        _ => None,
    }
}

fn handle_broadcast(
    app: &AppHandle,
    runtime: &RuntimeHandle,
    message: &Value,
    client_id: &str,
    followed: &mut HashMap<String, String>,
) -> Vec<Value> {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Vec::new();
    };
    let params = message.get("params").unwrap_or(&Value::Null);
    let Some(thread) = params.get("conversationId").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(source) = message
        .get("sourceClientId")
        .and_then(Value::as_str)
        .filter(|source| !source.is_empty())
    else {
        return Vec::new();
    };
    if method == "thread-stream-following-status-requested" {
        let Some(host_id) = params.get("hostId").and_then(Value::as_str) else {
            return Vec::new();
        };
        if source == client_id {
            return Vec::new();
        }
        return following_status_response(runtime, thread, host_id, client_id, followed)
            .into_iter()
            .collect();
    }
    if method == "thread-stream-following-changed" {
        let following = params
            .get("following")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(host) = params.get("hostId").and_then(Value::as_str) else {
            return Vec::new();
        };
        if following && source != client_id {
            let changed_host = followed.get(thread).map(String::as_str) != Some(host);
            followed.insert(thread.to_owned(), host.to_owned());
            if changed_host {
                return vec![json!({
                "type": "broadcast",
                "sourceClientId": client_id,
                "version": 1,
                "method": "thread-stream-following-changed",
                "params": {
                    "conversationId": thread,
                    "hostId": host,
                    "following": true
                }
                })];
            }
        }
        return Vec::new();
    }
    if method != "thread-stream-state-changed" {
        return Vec::new();
    }
    let Some(change) = params.get("change") else {
        return Vec::new();
    };
    if change.get("type").and_then(Value::as_str) == Some("snapshot") {
        let Some(conversation_state) = change.get("conversationState") else {
            return Vec::new();
        };
        let thread_hash = hash_id(thread);
        let host_id = snapshot_host_id(params, thread, followed);
        {
            let state = runtime.state.lock().expect("runtime state poisoned");
            if !snapshot_can_refresh_target(
                state
                    .threads
                    .get(&thread_hash)
                    .and_then(|record| record.target_refresh.as_ref()),
                source,
                host_id.as_deref(),
            ) {
                return Vec::new();
            }
        }
        let parsed = parse_controls(thread, source, conversation_state);
        let pet_state = state_from_conversation(conversation_state, &parsed.notifications);
        let target = ControlTarget {
            conversation_id: thread.into(),
            owner_client_id: source.into(),
            host_id,
            cwd: conversation_state
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        #[cfg(debug_assertions)]
        let pending_count = parsed.pending.len();
        apply_runtime_event(
            app,
            runtime,
            RuntimeEvent::IpcSnapshot {
                thread: thread_hash.clone(),
                control: ThreadControl {
                    target,
                    pending: parsed.pending,
                    notifications: parsed.notifications,
                    stale: false,
                },
                state: pet_state,
            },
            "codex-app-ipc",
        );
        #[cfg(debug_assertions)]
        eprintln!("control-sync thread={thread_hash} pending={pending_count}");
        return Vec::new();
    }
    if change.get("type").and_then(Value::as_str) == Some("patches")
        && patches_touch_requests(change)
    {
        return vec![json!({
            "type": "request",
            "requestId": uuid::Uuid::new_v4().to_string(),
            "sourceClientId": client_id,
            "targetClientId": source,
            "version": 0,
            "method": "thread-follower-load-complete-history",
            "params": { "conversationId": thread }
        })];
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::super::{RuntimeHandle, ThreadControl, ThreadRecord, hash_id};
    use super::{
        FollowRefresh, PendingIpcResponse, arm_written_follow_refresh, decode_ipc_response,
        following_status_response, initialize_request, patches_touch_requests,
        retained_follow_targets, snapshot_host_id, state_from_conversation, validate_frame_length,
    };
    use crate::{
        control::ControlTarget,
        observer::{TargetRefresh, TargetRefreshPhase},
    };
    use serde_json::json;
    use std::collections::HashMap;
    use tokio::sync::oneshot;

    fn install_control(runtime: &RuntimeHandle, conversation: &str, host_id: Option<&str>) {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.threads.insert(
            hash_id(conversation),
            ThreadRecord {
                control: Some(ThreadControl {
                    target: ControlTarget {
                        conversation_id: conversation.into(),
                        owner_client_id: format!("{conversation}-owner"),
                        host_id: host_id.map(str::to_owned),
                        cwd: None,
                    },
                    pending: Default::default(),
                    notifications: Vec::new(),
                    stale: false,
                }),
                ..Default::default()
            },
        );
    }

    #[test]
    fn initialize_request_preserves_the_compatibility_client_type() {
        let request = initialize_request("request-id".into());
        assert_eq!(request["method"], "initialize");
        assert_eq!(request["requestId"], "request-id");
        assert_eq!(request["params"]["clientType"], "codex-pet-sidecar");
    }

    #[test]
    fn retained_follows_keep_selected_and_background_task_targets() {
        let runtime = RuntimeHandle::default();
        install_control(&runtime, "selected", Some("selected-host"));
        install_control(&runtime, "background", Some("background-host"));
        install_control(&runtime, "no-host", None);

        let mut targets = retained_follow_targets(&runtime);
        targets.sort_by(|left, right| left.conversation_id.cmp(&right.conversation_id));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].conversation_id, "background");
        assert_eq!(targets[0].host_id.as_deref(), Some("background-host"));
        assert_eq!(targets[1].conversation_id, "selected");
        assert_eq!(targets[1].host_id.as_deref(), Some("selected-host"));
    }

    #[test]
    fn following_status_request_uses_only_exact_remembered_task_state() {
        let runtime = RuntimeHandle::default();
        install_control(&runtime, "background", Some("background-host"));
        let mut followed = HashMap::new();

        let response = following_status_response(
            &runtime,
            "background",
            "background-host",
            "sidecar",
            &mut followed,
        )
        .expect("remembered background task should answer status request");
        assert_eq!(response["method"], "thread-stream-following-changed");
        assert_eq!(response["params"]["conversationId"], "background");
        assert_eq!(response["params"]["hostId"], "background-host");
        assert_eq!(response["params"]["following"], true);
        assert_eq!(
            followed.get("background").map(String::as_str),
            Some("background-host")
        );
        assert!(
            following_status_response(
                &runtime,
                "background",
                "wrong-host",
                "sidecar",
                &mut followed,
            )
            .is_none()
        );
        assert!(
            following_status_response(
                &runtime,
                "unknown",
                "background-host",
                "sidecar",
                &mut followed,
            )
            .is_none()
        );
    }

    #[test]
    fn written_follow_refresh_retains_exact_host_for_a_hostless_snapshot() {
        let runtime = RuntimeHandle::default();
        let thread = hash_id("conversation");
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "old-owner".into(),
            host_id: Some("host".into()),
            cwd: Some("/workspace".into()),
        };
        let mut followed = HashMap::new();
        {
            let mut state = runtime.state.lock().expect("runtime state poisoned");
            state.selected = Some(thread.clone());
            state.threads.insert(
                thread.clone(),
                ThreadRecord {
                    control: Some(ThreadControl {
                        target: target.clone(),
                        pending: Default::default(),
                        notifications: Vec::new(),
                        stale: true,
                    }),
                    target_refresh: Some(TargetRefresh {
                        stale_owner_client_id: target.owner_client_id.clone(),
                        host_id: "host".into(),
                        phase: TargetRefreshPhase::Pending,
                    }),
                    ..Default::default()
                },
            );

            arm_written_follow_refresh(
                &mut state,
                &mut followed,
                &FollowRefresh {
                    thread: thread.clone(),
                    target: target.clone(),
                },
            )
            .expect("exact written refresh should arm");

            assert_eq!(
                state.threads[&thread]
                    .target_refresh
                    .as_ref()
                    .expect("armed refresh missing")
                    .phase,
                TargetRefreshPhase::AwaitingSnapshot
            );
        }

        assert_eq!(
            snapshot_host_id(&json!({}), "conversation", &followed).as_deref(),
            Some("host")
        );
        assert_eq!(
            snapshot_host_id(&json!({}), "other-conversation", &followed),
            None
        );
        assert_eq!(
            snapshot_host_id(
                &json!({ "hostId": "explicit-host" }),
                "conversation",
                &followed
            )
            .as_deref(),
            Some("explicit-host")
        );
    }

    #[test]
    fn maps_ipc_conversation_states_to_the_product_vocabulary() {
        for (runtime_status, expected) in [
            ("active", Some("working")),
            ("idle", Some("completed")),
            ("systemError", Some("failed")),
            ("unknown", None),
        ] {
            assert_eq!(
                state_from_conversation(
                    &json!({"threadRuntimeStatus":{"type":runtime_status}}),
                    &[],
                ),
                expected,
                "unexpected mapping for {runtime_status}"
            );
        }
    }

    #[test]
    fn accepts_complete_conversation_control_snapshot() {
        assert!(validate_frame_length(36_175_028).is_ok());
    }

    #[test]
    fn refreshes_snapshot_only_when_request_patches_change() {
        assert!(patches_touch_requests(&json!({
            "patches": [{"op":"add","path":["requests",0],"value":{"id":"secret"}}]
        })));
        assert!(!patches_touch_requests(&json!({
            "patches": [{"op":"replace","path":["turns",0,"items"],"value":"stream text"}]
        })));
    }

    #[test]
    fn validates_ipc_response_method_and_owner() {
        let (reply, _response) = oneshot::channel();
        let pending = PendingIpcResponse {
            method: "thread-follower-interrupt-turn".into(),
            target_client_id: Some("owner".into()),
            reply,
        };
        assert!(
            decode_ipc_response(
                &json!({
                    "resultType":"success",
                    "method":"thread-follower-interrupt-turn",
                    "handledByClientId":"owner",
                    "result":{}
                }),
                &pending
            )
            .is_ok()
        );
        assert!(
            decode_ipc_response(
                &json!({
                    "resultType":"success",
                    "method":"thread-follower-start-turn",
                    "handledByClientId":"owner",
                    "result":{}
                }),
                &pending
            )
            .is_err()
        );
        assert!(
            decode_ipc_response(
                &json!({
                    "resultType":"success",
                    "method":"thread-follower-interrupt-turn",
                    "handledByClientId":"other-owner",
                    "result":{}
                }),
                &pending
            )
            .is_err()
        );
        assert!(
            decode_ipc_response(
                &json!({
                    "resultType":"success",
                    "method":"thread-follower-interrupt-turn",
                    "result":{}
                }),
                &pending
            )
            .is_err()
        );
    }
}
