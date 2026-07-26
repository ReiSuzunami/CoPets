use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    cdp::ControlTransport,
    control::{ControlNotification, ControlSnapshot, ControlTarget, PendingControl},
};

const SUMMARY_LIMIT: usize = 120;

pub(super) const OWNER_MISSING_MESSAGE: &str = "The selected Codex task has no owner";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub state: String,
    pub connected: bool,
    pub thread_id_hash: Option<String>,
    pub epoch: u64,
    pub current_question: Option<String>,
    pub task_summary: Option<String>,
    pub latest_update: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct ThreadContext {
    pub(super) question: Option<String>,
    pub(super) task_summary: Option<String>,
    pub(super) latest_update: Option<String>,
    pub(super) response_started: bool,
}

#[derive(Debug, PartialEq)]
pub(super) enum SessionContextEvent {
    UserQuestion(String),
    ResponseStarted,
    AssistantUpdate(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LifecycleSource {
    Jsonl,
    Ipc,
}

#[derive(Clone, Default)]
pub(super) struct ThreadLifecycle {
    pub(super) state: Option<String>,
    pub(super) epoch: u64,
    pub(super) terminal: bool,
    pub(super) source: Option<LifecycleSource>,
}

pub(super) enum LifecycleEvent<'a> {
    NewTurn,
    Progress,
    State(&'a str, LifecycleSource),
}

pub(super) fn is_terminal_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "interrupted")
}

pub(super) fn reduce_lifecycle(lifecycle: &mut ThreadLifecycle, event: LifecycleEvent<'_>) -> bool {
    let (next_state, source) = match event {
        LifecycleEvent::NewTurn => {
            lifecycle.epoch += 1;
            lifecycle.terminal = false;
            ("working", LifecycleSource::Jsonl)
        }
        LifecycleEvent::Progress => {
            if lifecycle.terminal {
                return false;
            }
            ("working", LifecycleSource::Jsonl)
        }
        LifecycleEvent::State(state, source) => {
            if state == "working" && lifecycle.terminal {
                if source == LifecycleSource::Jsonl {
                    return false;
                }
                lifecycle.epoch += 1;
                lifecycle.terminal = false;
            }
            lifecycle.terminal = is_terminal_state(state);
            (state, source)
        }
    };
    let changed = lifecycle.state.as_deref() != Some(next_state);
    lifecycle.state = Some(next_state.to_owned());
    lifecycle.source = Some(source);
    changed
}

pub(super) struct RuntimeState {
    pub(super) selected: Option<String>,
    pub(super) threads: HashMap<String, ThreadRecord>,
    pub(super) snapshot: RuntimeSnapshot,
    pub(super) follow_up_inflight: HashMap<String, u64>,
    pub(super) follow_up_next_token: u64,
    pub(super) dismissed: HashSet<String>,
    pub(super) ipc_connected: bool,
    pub(super) control_transport: ControlTransport,
    pub(super) cdp_port: Option<u16>,
    pub(super) cdp_process_id: Option<u32>,
    pub(super) transport_generation: u64,
}

pub(super) struct ThreadControl {
    pub(super) target: ControlTarget,
    pub(super) pending: HashMap<String, PendingControl>,
    pub(super) notifications: Vec<ControlNotification>,
    pub(super) stale: bool,
}

#[derive(Default)]
pub(super) struct ThreadRecord {
    pub(super) lifecycle: ThreadLifecycle,
    pub(super) context: ThreadContext,
    pub(super) control: Option<ThreadControl>,
    pub(super) target_refresh: Option<TargetRefresh>,
}

pub(super) fn turn_is_live(state: &RuntimeState, thread: &str) -> bool {
    state.threads.get(thread).is_some_and(|record| {
        !record.lifecycle.terminal && record.lifecycle.state.as_deref() == Some("working")
    })
}

pub(super) fn turn_is_ready_for_follow_up(state: &RuntimeState, thread: &str) -> bool {
    state.threads.get(thread).is_some_and(|record| {
        record.lifecycle.terminal && record.lifecycle.state.as_deref() == Some("completed")
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TargetRefreshPhase {
    Pending,
    AwaitingSnapshot,
}

#[derive(Clone)]
pub(super) struct TargetRefresh {
    pub(super) stale_owner_client_id: String,
    pub(super) host_id: String,
    pub(super) phase: TargetRefreshPhase,
}

pub(super) fn arm_target_refresh(
    state: &mut RuntimeState,
    thread: &str,
    stale_target: &ControlTarget,
) -> Result<(), String> {
    if state.selected.as_deref() != Some(thread) {
        return Err("The selected Codex task changed while reconnecting".to_owned());
    }
    let record = state
        .threads
        .get_mut(thread)
        .ok_or_else(|| "The selected Codex task changed while reconnecting".to_owned())?;
    let target_is_fresh = record.control.as_ref().is_some_and(|control| {
        !control.stale
            && control.target.conversation_id == stale_target.conversation_id
            && control.target.host_id == stale_target.host_id
    });
    let target_is_stale = record.control.as_ref().is_some_and(|control| {
        control.stale
            && control.target.conversation_id == stale_target.conversation_id
            && control.target.owner_client_id == stale_target.owner_client_id
            && control.target.host_id == stale_target.host_id
    });
    let Some(refresh) = record.target_refresh.as_mut() else {
        return target_is_fresh
            .then_some(())
            .ok_or_else(|| "The selected Codex task changed while reconnecting".to_owned());
    };
    if refresh.stale_owner_client_id != stale_target.owner_client_id
        || stale_target.host_id.as_deref() != Some(refresh.host_id.as_str())
        || !target_is_stale
    {
        return Err("The selected Codex task changed while reconnecting".to_owned());
    }
    refresh.phase = TargetRefreshPhase::AwaitingSnapshot;
    Ok(())
}

pub(super) enum RuntimeEvent {
    Session {
        thread: String,
        context: Option<SessionContextEvent>,
        state: Option<&'static str>,
    },
    IpcSnapshot {
        thread: String,
        control: ThreadControl,
        state: Option<&'static str>,
    },
    Select {
        thread: String,
    },
    IpcConnectivity {
        connected: bool,
    },
    ControlTransport {
        transport: ControlTransport,
        port: Option<u16>,
        process_id: Option<u32>,
    },
    CdpProcessExited {
        process_id: u32,
    },
}

#[derive(Default)]
pub(super) struct RuntimeEffects {
    pub(super) snapshot: Option<RuntimeSnapshot>,
    pub(super) control: Option<ControlSnapshot>,
}

impl RuntimeState {
    pub(super) fn reduce(&mut self, event: RuntimeEvent) -> RuntimeEffects {
        let (snapshot_changed, control_changed) = match event {
            RuntimeEvent::Session {
                thread,
                context,
                state,
            } => {
                let selected = self.selected.as_deref() == Some(thread.as_str());
                let record = self.threads.entry(thread).or_default();
                let mut changed = false;
                if let Some(context_event) = context {
                    let starts_new_turn =
                        matches!(context_event, SessionContextEvent::UserQuestion(_));
                    if !record.lifecycle.terminal || starts_new_turn {
                        let previous = record.context.clone();
                        let new_turn = apply_context_event(&mut record.context, context_event);
                        changed |= record.context != previous;
                        changed |= reduce_lifecycle(
                            &mut record.lifecycle,
                            if new_turn {
                                LifecycleEvent::NewTurn
                            } else {
                                LifecycleEvent::Progress
                            },
                        );
                    }
                }
                if let Some(state) = state {
                    changed |= reduce_lifecycle(
                        &mut record.lifecycle,
                        LifecycleEvent::State(state, LifecycleSource::Jsonl),
                    );
                }
                (selected && changed, selected && changed)
            }
            RuntimeEvent::IpcSnapshot {
                thread,
                control,
                state,
            } => {
                let selected = self.selected.as_deref() == Some(thread.as_str());
                let record = self.threads.entry(thread).or_default();
                record.control = Some(control);
                record.target_refresh = None;
                let lifecycle_changed = state.is_some_and(|state| {
                    reduce_lifecycle(
                        &mut record.lifecycle,
                        LifecycleEvent::State(state, LifecycleSource::Ipc),
                    )
                });
                let active_ids: HashSet<_> = self
                    .threads
                    .values()
                    .filter_map(|record| record.control.as_ref())
                    .flat_map(|control| control.pending.keys().cloned())
                    .collect();
                self.dismissed.retain(|id| active_ids.contains(id));
                (selected && lifecycle_changed, true)
            }
            RuntimeEvent::Select { thread } => {
                self.selected = Some(thread);
                (true, true)
            }
            RuntimeEvent::IpcConnectivity { connected } => {
                self.ipc_connected = connected;
                (false, true)
            }
            RuntimeEvent::ControlTransport {
                transport,
                port,
                process_id,
            } => {
                let cdp_ready = transport == ControlTransport::CdpReady
                    && port.is_some()
                    && process_id.is_some();
                self.cdp_port = cdp_ready.then_some(port).flatten();
                self.cdp_process_id = if transport == ControlTransport::IpcOnly {
                    None
                } else {
                    process_id
                };
                self.control_transport = if transport == ControlTransport::CdpReady && !cdp_ready {
                    ControlTransport::CdpDegraded
                } else {
                    transport
                };
                self.transport_generation = self.transport_generation.wrapping_add(1);
                (false, true)
            }
            RuntimeEvent::CdpProcessExited { process_id } => {
                if self.cdp_process_id != Some(process_id) {
                    (false, false)
                } else {
                    self.cdp_port = None;
                    self.cdp_process_id = None;
                    self.control_transport = ControlTransport::CdpDegraded;
                    self.transport_generation = self.transport_generation.wrapping_add(1);
                    (false, true)
                }
            }
        };

        let snapshot = snapshot_changed.then(|| {
            let snapshot = build_runtime_snapshot(self);
            self.snapshot = snapshot.clone();
            snapshot
        });
        let control = control_changed.then(|| control_snapshot(self));
        RuntimeEffects { snapshot, control }
    }
}

pub(super) fn compact_text(value: &str, limit: usize) -> Option<String> {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    if compact.chars().count() <= limit {
        return Some(compact);
    }
    let mut truncated: String = compact.chars().take(limit.saturating_sub(3)).collect();
    truncated.push_str("...");
    Some(truncated)
}

pub(super) fn compact_tail_text(value: &str, limit: usize) -> Option<String> {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() || limit == 0 {
        return None;
    }
    if compact.chars().count() <= limit {
        return Some(compact);
    }
    let mut tail = compact
        .chars()
        .rev()
        .take(limit.saturating_sub(1))
        .collect::<Vec<_>>();
    tail.reverse();
    Some(format!("\u{2026}{}", tail.into_iter().collect::<String>()))
}

pub(super) fn apply_context_event(context: &mut ThreadContext, event: SessionContextEvent) -> bool {
    match event {
        SessionContextEvent::UserQuestion(question) => {
            let starts_new_turn =
                context.question.as_deref() != Some(&question) || context.response_started;
            context.task_summary = compact_text(&question, SUMMARY_LIMIT);
            context.question = Some(question);
            context.latest_update = None;
            context.response_started = false;
            starts_new_turn
        }
        SessionContextEvent::ResponseStarted => {
            context.response_started = true;
            false
        }
        SessionContextEvent::AssistantUpdate(update) => {
            context.response_started = true;
            context.latest_update = Some(update);
            false
        }
    }
}

pub(super) fn hash_id(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn build_runtime_snapshot(state: &RuntimeState) -> RuntimeSnapshot {
    let thread = state.selected.clone();
    let record = thread.as_ref().and_then(|thread| state.threads.get(thread));
    let context = record
        .map(|record| record.context.clone())
        .unwrap_or_default();
    let epoch = record
        .map(|record| record.lifecycle.epoch)
        .unwrap_or_default();
    let pet_state = record
        .and_then(|record| record.lifecycle.state.clone())
        .unwrap_or_else(|| {
            if thread.is_some() {
                "idle".into()
            } else {
                "disconnected".into()
            }
        });
    RuntimeSnapshot {
        state: pet_state,
        connected: thread.is_some(),
        thread_id_hash: thread,
        epoch,
        current_question: context.question,
        task_summary: context.task_summary,
        latest_update: context.latest_update,
    }
}

#[derive(Clone, Copy)]
pub(super) enum ActionKind<'a> {
    Stop,
    Steer,
    StartFollowUp,
    Respond(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FollowUpKind {
    Steer,
    Start,
}

#[derive(Clone)]
pub(super) struct FollowUpDispatchGuard {
    pub(super) thread: String,
    pub(super) target: ControlTarget,
    pub(super) kind: FollowUpKind,
    pub(super) transport: ControlTransport,
    pub(super) transport_generation: u64,
}

pub(super) struct AuthorizedAction {
    pub(super) thread: String,
    pub(super) target: ControlTarget,
    pub(super) pending: Option<PendingControl>,
}

pub(super) fn authorize_action(
    state: &RuntimeState,
    kind: ActionKind<'_>,
) -> Result<AuthorizedAction, String> {
    let thread = state
        .selected
        .as_ref()
        .ok_or_else(|| "No controllable Codex task is selected".to_owned())?;
    let allowed_lifecycle = match kind {
        ActionKind::StartFollowUp => turn_is_ready_for_follow_up(state, thread),
        ActionKind::Stop | ActionKind::Steer | ActionKind::Respond(_) => {
            turn_is_live(state, thread)
        }
    };
    if !allowed_lifecycle {
        return Err(match kind {
            ActionKind::StartFollowUp => "The selected Codex task is not ready for a follow-up",
            ActionKind::Stop | ActionKind::Steer | ActionKind::Respond(_) => {
                "The selected Codex task is not running"
            }
        }
        .to_owned());
    }
    if !state.ipc_connected {
        return Err("The selected Codex task is reconnecting".to_owned());
    }
    let control = state
        .threads
        .get(thread)
        .and_then(|record| record.control.as_ref())
        .ok_or_else(|| OWNER_MISSING_MESSAGE.to_owned())?;
    if control.stale {
        return Err("The selected Codex task is reconnecting".to_owned());
    }
    let pending = match kind {
        ActionKind::Respond(action_id) => Some(
            control
                .pending
                .get(action_id)
                .cloned()
                .ok_or_else(|| "This request is no longer pending".to_owned())?,
        ),
        ActionKind::Stop | ActionKind::Steer | ActionKind::StartFollowUp => None,
    };
    Ok(AuthorizedAction {
        thread: thread.clone(),
        target: control.target.clone(),
        pending,
    })
}

pub(super) fn authorize_follow_up(
    state: &RuntimeState,
) -> Result<(AuthorizedAction, FollowUpKind), String> {
    if state.control_transport == ControlTransport::CdpReady {
        return authorize_cdp_follow_up(state);
    }
    let thread = state
        .selected
        .as_deref()
        .ok_or_else(|| "No controllable Codex task is selected".to_owned())?;
    let kind = if turn_is_live(state, thread) {
        FollowUpKind::Steer
    } else if turn_is_ready_for_follow_up(state, thread) {
        FollowUpKind::Start
    } else {
        return Err("The selected Codex task cannot accept a follow-up".to_owned());
    };
    let action = match kind {
        FollowUpKind::Steer => ActionKind::Steer,
        FollowUpKind::Start => ActionKind::StartFollowUp,
    };
    Ok((authorize_action(state, action)?, kind))
}

/// Final native-only check immediately before an IPC follow-up is placed on
/// the IPC writer queue. The worker repeats this check immediately before it
/// writes the frame, so a selection, owner, lifecycle, connection, or
/// transport change cannot redirect a previously authorized follow-up.
pub(super) fn authorize_ipc_follow_up_dispatch(
    state: &RuntimeState,
    guard: &FollowUpDispatchGuard,
) -> Result<(), String> {
    if guard.transport == ControlTransport::CdpReady
        || state.control_transport != guard.transport
        || state.transport_generation != guard.transport_generation
    {
        return Err("The selected Codex task changed before this follow-up could send".to_owned());
    }
    let (authorized, kind) = authorize_follow_up(state).map_err(|_| {
        "The selected Codex task changed before this follow-up could send".to_owned()
    })?;
    if kind != guard.kind || authorized.thread != guard.thread || authorized.target != guard.target
    {
        return Err("The selected Codex task changed before this follow-up could send".to_owned());
    }
    Ok(())
}

pub(super) fn authorize_cdp_follow_up(
    state: &RuntimeState,
) -> Result<(AuthorizedAction, FollowUpKind), String> {
    if state.control_transport != ControlTransport::CdpReady
        || state.cdp_port.is_none()
        || state.cdp_process_id.is_none()
    {
        return Err("The CoPets bridge is unavailable".to_owned());
    }
    let thread = state
        .selected
        .as_ref()
        .ok_or_else(|| "No controllable Codex task is selected".to_owned())?;
    let kind = if turn_is_live(state, thread) {
        FollowUpKind::Steer
    } else if turn_is_ready_for_follow_up(state, thread) {
        FollowUpKind::Start
    } else {
        return Err("The selected Codex task cannot accept a follow-up".to_owned());
    };
    let target = state
        .threads
        .get(thread)
        .and_then(|record| record.control.as_ref())
        .map(|control| control.target.clone())
        .ok_or_else(|| OWNER_MISSING_MESSAGE.to_owned())?;
    if target.conversation_id.trim().is_empty()
        || target
            .cwd
            .as_deref()
            .is_none_or(|cwd| cwd.trim().is_empty())
        || target
            .host_id
            .as_deref()
            .is_none_or(|host| host.trim().is_empty())
    {
        return Err("The selected Codex task is missing bridge target details".to_owned());
    }
    Ok((
        AuthorizedAction {
            thread: thread.clone(),
            target,
            pending: None,
        },
        kind,
    ))
}

pub(super) fn control_snapshot(state: &RuntimeState) -> ControlSnapshot {
    let can_stop = authorize_action(state, ActionKind::Stop).is_ok();
    let can_ipc_reply = authorize_action(state, ActionKind::Steer).is_ok();
    let can_ipc_start_follow_up = authorize_action(state, ActionKind::StartFollowUp).is_ok();
    let cdp_follow_up = (state.control_transport == ControlTransport::CdpReady)
        .then(|| authorize_cdp_follow_up(state).ok())
        .flatten();
    let can_reply = can_ipc_reply
        || cdp_follow_up
            .as_ref()
            .is_some_and(|(_, kind)| *kind == FollowUpKind::Steer);
    let can_start_follow_up = can_ipc_start_follow_up
        || cdp_follow_up
            .as_ref()
            .is_some_and(|(_, kind)| *kind == FollowUpKind::Start);
    let show_working_follow_up = state
        .selected
        .as_deref()
        .is_some_and(|thread| turn_is_live(state, thread));
    let show_ready_follow_up = state
        .selected
        .as_deref()
        .is_some_and(|thread| turn_is_ready_for_follow_up(state, thread));
    let notifications = state
        .selected
        .as_ref()
        .and_then(|thread| state.threads.get(thread))
        .and_then(|record| record.control.as_ref())
        .filter(|_| can_ipc_reply)
        .map(|control| {
            control
                .notifications
                .iter()
                .filter(|notification| !state.dismissed.contains(&notification.id))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    ControlSnapshot {
        can_stop,
        can_reply,
        can_start_follow_up,
        show_working_follow_up,
        show_ready_follow_up,
        transport: state.control_transport,
        notifications,
    }
}

pub(super) fn complete_control_action(state: &mut RuntimeState, pending: &PendingControl) -> bool {
    let thread = hash_id(&pending.conversation_id);
    let Some(control) = state
        .threads
        .get_mut(&thread)
        .and_then(|record| record.control.as_mut())
    else {
        return false;
    };
    let is_same_request = control.pending.get(&pending.id).is_some_and(|current| {
        current.request_id == pending.request_id
            && current.owner_client_id == pending.owner_client_id
            && current.conversation_id == pending.conversation_id
    });
    if !is_same_request {
        return false;
    }
    control.pending.remove(&pending.id);
    control
        .notifications
        .retain(|notification| notification.id != pending.id);
    true
}

pub(super) fn snapshot_can_refresh_target(
    refresh: Option<&TargetRefresh>,
    source_client_id: &str,
    host_id: Option<&str>,
) -> bool {
    refresh.is_none_or(|refresh| {
        host_id == Some(refresh.host_id.as_str())
            && (source_client_id != refresh.stale_owner_client_id
                || refresh.phase == TargetRefreshPhase::AwaitingSnapshot)
    })
}
