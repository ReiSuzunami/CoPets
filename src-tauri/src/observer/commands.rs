use std::{
    collections::HashMap,
    future::Future,
    sync::mpsc::{self, TryRecvError},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::time::{Instant, sleep, timeout};

use crate::cdp::{
    CdpEndpoint, CdpEndpointOrigin, ControlTransport, discover_existing_codex, launch_codex,
    official_codex_process_is_running, request_codex_restart, reserve_port,
    restartable_codex_process, running_codex_process, verify_tracked_listener,
};
use crate::control::{
    ControlSnapshot, ControlTarget, build_cdp_ready_params, build_cdp_steer_params,
    build_control_request, build_follow_up, build_ready_follow_up,
};

use super::RuntimeHandle;

const TARGET_REFRESH_TIMEOUT: Duration = Duration::from_secs(3);
const TARGET_REFRESH_POLL: Duration = Duration::from_millis(40);
const CDP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);
const CDP_LAUNCH_POLL: Duration = Duration::from_millis(350);
const CODEX_RESTART_EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const CODEX_RESTART_EXIT_POLL: Duration = Duration::from_millis(100);
const CDP_RETRY_TIMEOUT: Duration = Duration::from_secs(8);
const CDP_RETRY_POLL: Duration = Duration::from_millis(250);
const CDP_VERIFICATION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(3);
const CDP_RETRY_FAILURE_MESSAGE: &str =
    "CoPets could not verify the tracked Codex bridge. Keep Codex open, then retry.";
pub(super) const OWNER_UNAVAILABLE_MESSAGE: &str =
    "Codex reported this task's owner unavailable. Keep it open in Codex, then retry.";
const OWNER_DISCOVERY_TIMEOUT_MESSAGE: &str =
    "Codex has not exposed an owner for the selected task yet. Keep it open in Codex, then retry.";
use super::runtime::{
    ActionKind, FollowUpDispatchGuard, FollowUpKind, OWNER_MISSING_MESSAGE, RuntimeSnapshot,
    RuntimeState, TargetRefresh, TargetRefreshPhase, authorize_action, authorize_follow_up,
    complete_control_action, control_snapshot, turn_is_live, turn_is_ready_for_follow_up,
};
use super::selection::refresh_foreground_selection;

struct FollowUpInflightGuard {
    runtime: RuntimeHandle,
    thread: String,
    token: u64,
}

struct CdpLaunchGuard {
    runtime: RuntimeHandle,
}

pub(super) struct FollowUpAttempt {
    pub(super) thread: String,
    pub(super) target: ControlTarget,
    pub(super) kind: FollowUpKind,
    pub(super) transport: ControlTransport,
    pub(super) transport_generation: u64,
    pub(super) inflight_token: u64,
    pub(super) refresh_before_dispatch: bool,
}

struct FollowUpDispatchFailure {
    error: String,
    dispatched_target: Option<ControlTarget>,
}

impl FollowUpDispatchFailure {
    fn before_dispatch(error: String) -> Self {
        Self {
            error,
            dispatched_target: None,
        }
    }

    fn after_dispatch(error: String, target: ControlTarget) -> Self {
        Self {
            error,
            dispatched_target: Some(target),
        }
    }
}

impl Drop for FollowUpInflightGuard {
    fn drop(&mut self) {
        let mut state = self.runtime.state.lock().expect("runtime state poisoned");
        release_follow_up_inflight(&mut state, &self.thread, self.token);
    }
}

impl Drop for CdpLaunchGuard {
    fn drop(&mut self) {
        *self
            .runtime
            .cdp_launch_inflight
            .lock()
            .expect("CDP launch state poisoned") = false;
    }
}

fn reserve_cdp_bridge_operation(runtime: &RuntimeHandle) -> Result<CdpLaunchGuard, String> {
    let mut launch_inflight = runtime
        .cdp_launch_inflight
        .lock()
        .expect("CDP launch state poisoned");
    if *launch_inflight {
        return Err("A CoPets bridge operation is already in progress.".to_owned());
    }
    *launch_inflight = true;
    Ok(CdpLaunchGuard {
        runtime: runtime.clone(),
    })
}

pub(super) async fn cdp_verification_within_deadline<F>(deadline: Instant, verification: F) -> bool
where
    F: Future<Output = Result<(), String>>,
{
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return false;
    }
    timeout(remaining, verification)
        .await
        .is_ok_and(|result| result.is_ok())
}

/// Retry readiness probes without ever extending the command's absolute deadline.
///
/// A CDP target can be present while its renderer is still initializing, and the
/// DevTools socket itself can briefly reject a connection. Each attempt receives
/// a small slice of the caller's budget so a transient failure does not consume
/// the whole operation, while no attempt can renew the deadline.
pub(super) async fn poll_cdp_verification_until_ready<F, Fut>(
    deadline: Instant,
    attempt_timeout: Duration,
    poll_interval: Duration,
    mut verification: F,
) -> bool
where
    F: FnMut(Instant) -> Fut,
    Fut: Future<Output = bool>,
{
    loop {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let attempt_deadline = std::cmp::min(deadline, now + attempt_timeout);
        if verification(attempt_deadline).await {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        sleep(poll_interval.min(remaining)).await;
    }
}

async fn verify_cdp_within_deadline(deadline: Instant, endpoint: CdpEndpoint) -> bool {
    cdp_verification_within_deadline(deadline, async {
        verify_tracked_listener(endpoint).await?;
        crate::cdp::verify_rf(endpoint.port, deadline).await
    })
    .await
}

async fn verify_cdp_until_ready(deadline: Instant, endpoint: CdpEndpoint) -> bool {
    poll_cdp_verification_until_ready(
        deadline,
        CDP_VERIFICATION_ATTEMPT_TIMEOUT,
        CDP_RETRY_POLL,
        |attempt_deadline| verify_cdp_within_deadline(attempt_deadline, endpoint),
    )
    .await
}

fn track_cdp_endpoint(app: &AppHandle, runtime: &RuntimeHandle, endpoint: CdpEndpoint) -> u128 {
    let liveness_token = runtime.arm_cdp_process_liveness(endpoint.process_id);
    runtime.remember_cdp_endpoint(endpoint);
    super::set_control_transport(
        app,
        runtime,
        ControlTransport::CdpDegraded,
        None,
        Some(endpoint.process_id),
    );
    liveness_token
}

fn monitor_user_attached_cdp_endpoint(
    app: AppHandle,
    runtime: RuntimeHandle,
    endpoint: CdpEndpoint,
    liveness_token: u128,
) {
    if endpoint.origin != CdpEndpointOrigin::UserAttached {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            sleep(Duration::from_secs(1)).await;
            if verify_tracked_listener(endpoint).await.is_err() {
                super::clear_cdp_process(&app, &runtime, endpoint, liveness_token);
                return;
            }
        }
    });
}

fn monitor_verified_user_attached_cdp_endpoint(
    app: AppHandle,
    runtime: RuntimeHandle,
    endpoint: CdpEndpoint,
) {
    let Some(liveness_token) = runtime.cdp_liveness_token_for_endpoint(endpoint) else {
        return;
    };
    monitor_user_attached_cdp_endpoint(app, runtime, endpoint, liveness_token);
}

fn reserve_follow_up_inflight(state: &mut RuntimeState, thread: &str) -> Result<u64, String> {
    if state.follow_up_inflight.contains_key(thread) {
        return Err("A reply is already being sent".to_owned());
    }
    state.follow_up_next_token = state.follow_up_next_token.wrapping_add(1);
    let token = state.follow_up_next_token;
    state.follow_up_inflight.insert(thread.to_owned(), token);
    Ok(token)
}

pub(super) fn release_follow_up_inflight(state: &mut RuntimeState, thread: &str, token: u64) {
    if state.follow_up_inflight.get(thread) == Some(&token) {
        state.follow_up_inflight.remove(thread);
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpLaunchResult {
    transport: ControlTransport,
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
pub async fn launch_codex_with_cdp(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
    custom_port: Option<u16>,
) -> Result<CdpLaunchResult, String> {
    let _launch = reserve_cdp_bridge_operation(runtime.inner())?;
    if runtime.inner().has_tracked_cdp_endpoint() {
        return Err(
            "CoPets is already tracking a Codex bridge. Retry it or close that Codex before connecting another one."
                .to_owned(),
        );
    }
    if running_codex_process()? {
        return Err(
            "Quit Codex before launching the CoPets bridge. CoPets will not close it for you."
                .to_owned(),
        );
    }
    launch_codex_bridge(&app, runtime.inner(), custom_port).await
}

async fn launch_codex_bridge(
    app: &AppHandle,
    runtime: &RuntimeHandle,
    custom_port: Option<u16>,
) -> Result<CdpLaunchResult, String> {
    let port = reserve_port(custom_port)?;
    let mut launched = launch_codex(port)?;
    let endpoint = CdpEndpoint::launched(launched.pid, port);
    let liveness_token = track_cdp_endpoint(app, runtime, endpoint);
    let (exited_sender, exited_receiver) = mpsc::sync_channel(1);
    let exit_app = app.clone();
    let exit_runtime = runtime.clone();
    std::thread::spawn(move || {
        let _ = launched.child.wait();
        let _ = exited_sender.send(());
        super::clear_cdp_process(&exit_app, &exit_runtime, endpoint, liveness_token);
    });

    let deadline = Instant::now() + CDP_LAUNCH_TIMEOUT;
    loop {
        if !matches!(exited_receiver.try_recv(), Err(TryRecvError::Empty)) {
            return Err(
                "CoPets could not verify Codex bridge. Standard IPC controls remain available."
                    .to_owned(),
            );
        }
        if Instant::now() >= deadline {
            super::set_control_transport(
                app,
                runtime,
                ControlTransport::CdpDegraded,
                None,
                Some(endpoint.process_id),
            );
            return Err(
                "CoPets could not verify Codex bridge. Standard IPC controls remain available."
                    .to_owned(),
            );
        }
        if verify_cdp_within_deadline(deadline, endpoint).await
            && super::set_cdp_ready(app, runtime, endpoint)
        {
            return Ok(CdpLaunchResult {
                transport: ControlTransport::CdpReady,
            });
        }
        if Instant::now() >= deadline {
            super::set_control_transport(
                app,
                runtime,
                ControlTransport::CdpDegraded,
                None,
                Some(endpoint.process_id),
            );
            return Err(
                "CoPets could not verify Codex bridge. Standard IPC controls remain available."
                    .to_owned(),
            );
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        sleep(CDP_LAUNCH_POLL.min(remaining)).await;
    }
}

async fn wait_for_codex_exit(process_id: u32) -> Result<(), String> {
    let deadline = Instant::now() + CODEX_RESTART_EXIT_TIMEOUT;
    loop {
        let still_running = tokio::task::spawn_blocking(move || {
            official_codex_process_is_running(process_id)
        })
        .await
        .map_err(|_| {
            "CoPets could not verify that Codex closed. Close it yourself, then use Launch Codex."
                .to_owned()
        })??;
        if !still_running {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(
                "Codex did not close in time. Close it yourself, then use Launch Codex.".to_owned(),
            );
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        sleep(CODEX_RESTART_EXIT_POLL.min(remaining)).await;
    }
}

/// Restarts one explicitly confirmed normal Codex App through the existing
/// loopback-only bridge launcher. It never force-kills the App or restarts one
/// that already exposes CDP; all target selection remains native-only.
#[tauri::command]
pub async fn restart_codex_with_cdp(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
    custom_port: Option<u16>,
) -> Result<CdpLaunchResult, String> {
    let _restart = reserve_cdp_bridge_operation(runtime.inner())?;
    if runtime.inner().has_tracked_cdp_endpoint() {
        return Err(
            "CoPets is already tracking a Codex bridge. Retry it or close that Codex before restarting another one."
                .to_owned(),
        );
    }
    let process_id = restartable_codex_process()?;
    // Validate and choose the bridge port before closing the user's App. The
    // launch path checks availability again after the old process exits, so a
    // later local port race still fails closed instead of choosing another port.
    let port = reserve_port(custom_port)?;
    request_codex_restart(process_id)?;
    wait_for_codex_exit(process_id).await?;
    if running_codex_process()? {
        return Err(
            "Another Codex App is still open. Close it yourself, then use Launch Codex.".to_owned(),
        );
    }
    launch_codex_bridge(&app, runtime.inner(), Some(port)).await
}

#[tauri::command]
pub async fn connect_existing_codex_cdp(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
    port: Option<u16>,
) -> Result<CdpLaunchResult, String> {
    let _attach = reserve_cdp_bridge_operation(runtime.inner())?;
    if runtime.inner().has_tracked_cdp_endpoint() {
        return Err(
            "CoPets is already tracking a Codex bridge. Retry it or close that Codex before connecting another one."
                .to_owned(),
        );
    }
    let endpoint = discover_existing_codex(port).await?;
    track_cdp_endpoint(&app, runtime.inner(), endpoint);
    let verified = verify_cdp_until_ready(Instant::now() + CDP_RETRY_TIMEOUT, endpoint).await
        && super::set_cdp_ready(&app, runtime.inner(), endpoint);
    if !verified {
        return Err(
            "CoPets could not verify this Codex bridge. Standard IPC controls remain available."
                .to_owned(),
        );
    }
    monitor_verified_user_attached_cdp_endpoint(app.clone(), runtime.inner().clone(), endpoint);
    Ok(CdpLaunchResult {
        transport: ControlTransport::CdpReady,
    })
}

#[tauri::command]
pub async fn retry_cdp_bridge(
    app: AppHandle,
    runtime: tauri::State<'_, RuntimeHandle>,
) -> Result<CdpLaunchResult, String> {
    let _retry = reserve_cdp_bridge_operation(runtime.inner())?;
    let endpoint = runtime.inner().cdp_endpoint_for_retry()?;
    let verified = verify_cdp_until_ready(Instant::now() + CDP_RETRY_TIMEOUT, endpoint).await
        && super::set_cdp_ready(&app, runtime.inner(), endpoint);
    if !verified {
        return Err(CDP_RETRY_FAILURE_MESSAGE.to_owned());
    }
    monitor_verified_user_attached_cdp_endpoint(app.clone(), runtime.inner().clone(), endpoint);
    Ok(CdpLaunchResult {
        transport: ControlTransport::CdpReady,
    })
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

#[cfg(any(debug_assertions, test))]
pub(super) fn follow_up_error_class(error: &str) -> &'static str {
    if is_stale_client_error(error) {
        "owner-not-found"
    } else if error.contains("reconnecting") {
        "owner-reconnecting"
    } else if error.contains("has no owner") {
        "owner-missing"
    } else if error.contains("not running") || error.contains("not ready") {
        "lifecycle"
    } else if error.contains("IPC") || error.contains("disconnected") || error.contains("connected")
    {
        "ipc-unavailable"
    } else if error.contains("timed out") {
        "timeout"
    } else {
        "other"
    }
}

fn follow_up_diagnostic(stage: &str, kind: Option<FollowUpKind>, error: Option<&str>) {
    #[cfg(debug_assertions)]
    {
        let kind = match kind {
            Some(FollowUpKind::Steer) => "steer",
            Some(FollowUpKind::Start) => "start",
            None => "unknown",
        };
        let outcome = error.map_or_else(
            || {
                if stage.ends_with("begin") {
                    "started"
                } else {
                    "ok"
                }
            },
            follow_up_error_class,
        );
        eprintln!("copets-follow-up stage={stage} kind={kind} outcome={outcome}");
    }
    #[cfg(not(debug_assertions))]
    let _ = (stage, kind, error);
}

fn selected_target_matches(state: &RuntimeState, thread: &str, target: &ControlTarget) -> bool {
    state.selected.as_deref() == Some(thread)
        && state
            .threads
            .get(thread)
            .and_then(|record| record.control.as_ref())
            .is_some_and(|control| control.target == *target)
}

pub(super) fn mark_target_stale(
    state: &mut RuntimeState,
    thread: &str,
    target: &ControlTarget,
) -> bool {
    if !selected_target_matches(state, thread, target) {
        return false;
    }
    let record = state
        .threads
        .get_mut(thread)
        .expect("selected control target must have a thread record");
    record
        .control
        .as_mut()
        .expect("selected control target must have control state")
        .stale = true;
    if let Some(host_id) = target.host_id.clone() {
        record.target_refresh = Some(TargetRefresh {
            stale_owner_client_id: target.owner_client_id.clone(),
            host_id,
            phase: TargetRefreshPhase::Pending,
        });
    }
    true
}

#[cfg(test)]
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

#[cfg(test)]
pub(super) async fn dispatch_ready_follow_up(
    runtime: &RuntimeHandle,
    target: &ControlTarget,
    prompt: &str,
) -> Result<(), String> {
    let (method, params) = build_ready_follow_up(target, prompt)?;
    runtime
        .request(method, params, Some(target.owner_client_id.clone()))
        .await
        .map(|_| ())
}

async fn dispatch_follow_up(
    runtime: &RuntimeHandle,
    prompt: &str,
    guard: &FollowUpDispatchGuard,
) -> Result<(), String> {
    if guard.transport == ControlTransport::CdpReady {
        let (operation, params) = match guard.kind {
            FollowUpKind::Steer => (
                "steer-turn-for-host",
                build_cdp_steer_params(&guard.target, prompt)?,
            ),
            FollowUpKind::Start => (
                "send-follow-up-message",
                build_cdp_ready_params(&guard.target, prompt)?,
            ),
        };
        return runtime
            .cdp_rf_call(
                guard.transport_generation,
                &guard.thread,
                &guard.target,
                operation,
                params,
            )
            .await;
    }
    let (method, params) = match guard.kind {
        FollowUpKind::Steer => build_follow_up(&guard.target, prompt)?,
        FollowUpKind::Start => build_ready_follow_up(&guard.target, prompt)?,
    };
    runtime
        .request_follow_up(method, params, guard.clone())
        .await
        .map(|_| ())
}

#[cfg(test)]
pub(super) async fn refresh_and_retry_follow_up(
    runtime: &RuntimeHandle,
    thread: &str,
    stale_target: &ControlTarget,
    prompt: &str,
    kind: FollowUpKind,
) -> Result<(), String> {
    refresh_and_retry_follow_up_with_target(runtime, thread, stale_target, prompt, kind)
        .await
        .map_err(|failure| failure.error)
}

async fn refresh_and_retry_follow_up_with_target(
    runtime: &RuntimeHandle,
    thread: &str,
    stale_target: &ControlTarget,
    prompt: &str,
    kind: FollowUpKind,
) -> Result<(), FollowUpDispatchFailure> {
    {
        let state = runtime.state.lock().expect("runtime state poisoned");
        if !selected_target_matches(&state, thread, stale_target) {
            return Err(FollowUpDispatchFailure::before_dispatch(
                "The selected Codex task changed while reconnecting".to_owned(),
            ));
        }
    }
    runtime
        .refresh_following(thread, stale_target)
        .await
        .map_err(FollowUpDispatchFailure::before_dispatch)?;
    let target =
        wait_for_refreshed_follow_up(runtime, thread, stale_target, kind, TARGET_REFRESH_TIMEOUT)
            .await
            .map_err(FollowUpDispatchFailure::before_dispatch)?;
    let guard = {
        let state = runtime.state.lock().expect("runtime state poisoned");
        reauthorize_refreshed_follow_up(&state, thread, stale_target, &target, kind)
            .map_err(FollowUpDispatchFailure::before_dispatch)?;
        FollowUpDispatchGuard {
            thread: thread.to_owned(),
            target: target.clone(),
            kind,
            transport: state.control_transport,
            transport_generation: state.transport_generation,
        }
    };
    dispatch_follow_up(runtime, prompt, &guard)
        .await
        .map_err(|error| FollowUpDispatchFailure::after_dispatch(error, target))
}

pub(super) fn reauthorize_refreshed_follow_up(
    state: &RuntimeState,
    thread: &str,
    stale_target: &ControlTarget,
    target: &ControlTarget,
    kind: FollowUpKind,
) -> Result<(), String> {
    if state.control_transport == ControlTransport::CdpReady
        || !selected_target_matches(state, thread, target)
        || target.conversation_id != stale_target.conversation_id
        || target.host_id != stale_target.host_id
    {
        return Err("The selected Codex task changed while reconnecting".to_owned());
    }
    let (authorized, authorized_kind) = authorize_follow_up(state)?;
    if authorized_kind != kind || authorized.thread != thread || authorized.target != *target {
        return Err("The selected Codex task changed while reconnecting".to_owned());
    }
    Ok(())
}

async fn wait_for_refreshed_follow_up(
    runtime: &RuntimeHandle,
    thread: &str,
    stale_target: &ControlTarget,
    kind: FollowUpKind,
    timeout: Duration,
) -> Result<ControlTarget, String> {
    let deadline = Instant::now() + timeout;
    loop {
        {
            let state = runtime.state.lock().expect("runtime state poisoned");
            if state.selected.as_deref() != Some(thread) {
                return Err("The selected Codex task changed while reconnecting".to_owned());
            }
            let refresh_pending = state
                .threads
                .get(thread)
                .is_some_and(|record| record.target_refresh.is_some());
            match authorize_follow_up(&state) {
                Ok((authorized, refreshed_kind)) if !refresh_pending => {
                    if refreshed_kind != kind {
                        return Err("The selected Codex task changed while reconnecting".to_owned());
                    }
                    let target = authorized.target;
                    if target.conversation_id != stale_target.conversation_id
                        || target.host_id != stale_target.host_id
                    {
                        return Err("The refreshed Codex task owner did not match".to_owned());
                    }
                    return Ok(target);
                }
                Ok(_) => {}
                Err(error) => {
                    let lifecycle_matches = match kind {
                        FollowUpKind::Steer => turn_is_live(&state, thread),
                        FollowUpKind::Start => turn_is_ready_for_follow_up(&state, thread),
                    };
                    let still_waiting = state.ipc_connected
                        && state.threads.get(thread).is_some_and(|record| {
                            lifecycle_matches
                                && (record.target_refresh.is_some()
                                    || record.control.as_ref().is_none_or(|control| control.stale))
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

fn stale_follow_up_attempt(state: &mut RuntimeState) -> Option<Result<FollowUpAttempt, String>> {
    if !state.ipc_connected || state.control_transport == ControlTransport::CdpReady {
        return None;
    }
    let thread = state.selected.clone()?;
    let target = state
        .threads
        .get(&thread)
        .and_then(|record| record.control.as_ref())
        .filter(|control| control.stale)
        .map(|control| control.target.clone())?;
    let kind = if turn_is_live(state, &thread) {
        FollowUpKind::Steer
    } else if turn_is_ready_for_follow_up(state, &thread) {
        FollowUpKind::Start
    } else {
        return None;
    };
    if target.host_id.is_none() {
        return Some(Err(OWNER_UNAVAILABLE_MESSAGE.to_owned()));
    }
    if !mark_target_stale(state, &thread, &target) {
        return Some(Err(
            "The selected Codex task changed while reconnecting".to_owned()
        ));
    }
    Some(Ok(FollowUpAttempt {
        thread,
        target,
        kind,
        transport: state.control_transport,
        transport_generation: state.transport_generation,
        inflight_token: 0,
        refresh_before_dispatch: true,
    }))
}

pub(super) fn prepare_follow_up_attempt(
    state: &mut RuntimeState,
) -> Result<FollowUpAttempt, String> {
    let mut attempt = match stale_follow_up_attempt(state) {
        Some(result) => result?,
        None => {
            let (authorized, kind) = authorize_follow_up(state)?;
            FollowUpAttempt {
                thread: authorized.thread,
                target: authorized.target,
                kind,
                transport: state.control_transport,
                transport_generation: state.transport_generation,
                inflight_token: 0,
                refresh_before_dispatch: false,
            }
        }
    };
    attempt.inflight_token = reserve_follow_up_inflight(state, &attempt.thread)?;
    Ok(attempt)
}

fn can_wait_for_selected_owner(state: &RuntimeState, thread: &str, error: &str) -> bool {
    error == OWNER_MISSING_MESSAGE
        && state.ipc_connected
        && state.selected.as_deref() == Some(thread)
        && (turn_is_live(state, thread) || turn_is_ready_for_follow_up(state, thread))
}

pub(super) async fn prepare_follow_up_after_foreground_selection(
    runtime: &RuntimeHandle,
    selected_thread: &str,
) -> Result<FollowUpAttempt, String> {
    let deadline = Instant::now() + TARGET_REFRESH_TIMEOUT;
    loop {
        let wait_for_owner = {
            let mut state = runtime.state.lock().expect("runtime state poisoned");
            if state.selected.as_deref() != Some(selected_thread) {
                return Err("The selected Codex task changed before its owner appeared".to_owned());
            }
            match prepare_follow_up_attempt(&mut state) {
                Ok(attempt) => return Ok(attempt),
                Err(error) if can_wait_for_selected_owner(&state, selected_thread, &error) => true,
                Err(error) => return Err(error),
            }
        };
        debug_assert!(wait_for_owner);
        if Instant::now() >= deadline {
            return Err(OWNER_DISCOVERY_TIMEOUT_MESSAGE.to_owned());
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
    let selected_thread = match refresh_foreground_selection(&app, runtime.inner()).await {
        Ok(thread) => thread,
        Err(error) => {
            follow_up_diagnostic("selection", None, Some(&error));
            return Err(error);
        }
    };
    let attempt =
        match prepare_follow_up_after_foreground_selection(runtime.inner(), &selected_thread).await
        {
            Ok(attempt) => attempt,
            Err(error) => {
                follow_up_diagnostic("authorize", None, Some(&error));
                return Err(error);
            }
        };
    follow_up_diagnostic("dispatch-begin", Some(attempt.kind), None);
    let _inflight = FollowUpInflightGuard {
        runtime: runtime.inner().clone(),
        thread: attempt.thread.clone(),
        token: attempt.inflight_token,
    };
    let result = if attempt.transport != ControlTransport::CdpReady
        && attempt.refresh_before_dispatch
    {
        refresh_and_retry_follow_up_with_target(
            runtime.inner(),
            &attempt.thread,
            &attempt.target,
            &prompt,
            attempt.kind,
        )
        .await
    } else {
        let guard = FollowUpDispatchGuard {
            thread: attempt.thread.clone(),
            target: attempt.target.clone(),
            kind: attempt.kind,
            transport: attempt.transport,
            transport_generation: attempt.transport_generation,
        };
        match dispatch_follow_up(runtime.inner(), &prompt, &guard).await {
            Err(error)
                if attempt.transport != ControlTransport::CdpReady
                    && is_stale_client_error(&error) =>
            {
                let marked = {
                    let mut state = runtime.state.lock().expect("runtime state poisoned");
                    let marked = mark_target_stale(&mut state, &attempt.thread, &attempt.target);
                    if marked {
                        emit_control_snapshot(&app, &state);
                    }
                    marked
                };
                if marked {
                    refresh_and_retry_follow_up_with_target(
                        runtime.inner(),
                        &attempt.thread,
                        &attempt.target,
                        &prompt,
                        attempt.kind,
                    )
                    .await
                } else {
                    Err(FollowUpDispatchFailure::before_dispatch(
                        "The selected Codex task changed while reconnecting".to_owned(),
                    ))
                }
            }
            result => result.map_err(|error| {
                FollowUpDispatchFailure::after_dispatch(error, attempt.target.clone())
            }),
        }
    };
    match result {
        Err(failure)
            if attempt.transport != ControlTransport::CdpReady
                && is_stale_client_error(&failure.error) =>
        {
            if let Some(target) = failure.dispatched_target.as_ref() {
                let mut state = runtime.state.lock().expect("runtime state poisoned");
                if mark_target_stale(&mut state, &attempt.thread, target) {
                    emit_control_snapshot(&app, &state);
                }
            }
            follow_up_diagnostic("recovery", Some(attempt.kind), Some(&failure.error));
            Err(OWNER_UNAVAILABLE_MESSAGE.to_owned())
        }
        Err(failure) => {
            follow_up_diagnostic("dispatch-failed", Some(attempt.kind), Some(&failure.error));
            Err(failure.error)
        }
        Ok(()) => {
            follow_up_diagnostic("dispatch-complete", Some(attempt.kind), None);
            Ok(())
        }
    }
}
