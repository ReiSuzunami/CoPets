use super::{
    ActionKind, IpcCommand, LifecycleEvent, LifecycleSource, RuntimeEvent, RuntimeHandle,
    SessionContextEvent, TargetRefresh, ThreadContext, ThreadControl, ThreadLifecycle,
    ThreadRecord, apply_context_event, authorize_action, build_runtime_snapshot, compact_tail_text,
    complete_control_action, control_snapshot, dispatch_steering, dispatch_stop, hash_id,
    is_stale_client_error, is_terminal_state, reduce_lifecycle, refresh_and_retry_follow_up,
    snapshot_can_refresh_target,
};
use crate::control::{ControlTarget, PendingControl, PendingKind};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;

#[test]
fn hashes_are_short_and_stable() {
    assert_eq!(hash_id("thread-secret"), hash_id("thread-secret"));
    assert_eq!(hash_id("thread-secret").len(), 12);
}

#[test]
fn tail_compaction_prefixes_a_real_ellipsis_character() {
    assert_eq!(
        compact_tail_text("0123456789", 6).as_deref(),
        Some("\u{2026}56789")
    );
}

#[test]
fn task_context_switches_from_question_to_progress() {
    let mut context = ThreadContext::default();
    apply_context_event(
        &mut context,
        SessionContextEvent::UserQuestion("Make the pet clearer".into()),
    );
    assert!(!context.response_started);
    assert_eq!(context.question.as_deref(), Some("Make the pet clearer"));

    apply_context_event(&mut context, SessionContextEvent::ResponseStarted);
    apply_context_event(
        &mut context,
        SessionContextEvent::AssistantUpdate("Rebuilding the HD atlas".into()),
    );
    assert!(context.response_started);
    assert_eq!(
        context.task_summary.as_deref(),
        Some("Make the pet clearer")
    );
    assert_eq!(
        context.latest_update.as_deref(),
        Some("Rebuilding the HD atlas")
    );
}

#[test]
fn runtime_snapshot_projects_only_the_requested_task_record() {
    let runtime = RuntimeHandle::default();
    let selected_hash = hash_id("selected");
    let state = {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.threads.insert(
            selected_hash.clone(),
            ThreadRecord {
                lifecycle: ThreadLifecycle {
                    state: Some("working".into()),
                    epoch: 4,
                    terminal: false,
                    source: Some(LifecycleSource::Jsonl),
                },
                context: ThreadContext {
                    question: Some("Selected question".into()),
                    task_summary: Some("Selected task".into()),
                    latest_update: Some("Selected update".into()),
                    response_started: true,
                },
                ..Default::default()
            },
        );
        state.threads.insert(
            "background".into(),
            ThreadRecord {
                context: ThreadContext {
                    question: Some("Background question".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        state.selected = Some(selected_hash.clone());
        build_runtime_snapshot(&state)
    };

    assert_eq!(
        state.thread_id_hash.as_deref(),
        Some(selected_hash.as_str())
    );
    assert_ne!(state.thread_id_hash.as_deref(), Some("selected"));
    assert_eq!(state.epoch, 4);
    assert_eq!(state.current_question.as_deref(), Some("Selected question"));
    assert_eq!(state.task_summary.as_deref(), Some("Selected task"));
    assert_eq!(state.latest_update.as_deref(), Some("Selected update"));
    assert_ne!(
        state.current_question.as_deref(),
        Some("Background question")
    );
    let serialized = serde_json::to_value(&state).unwrap();
    assert!(serialized.get("source").is_none());
    assert!(serialized.get("contextMode").is_none());
    assert!(serialized.get("observedAtMs").is_none());
}

#[test]
fn unknown_state_is_not_a_product_terminal_state() {
    assert!(!is_terminal_state("unknown-state"));
}

#[test]
fn background_threads_do_not_drive_an_unselected_pet() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some("visible".into());
    state.snapshot.state = "idle".into();
    let effects = state.reduce(RuntimeEvent::Session {
        thread: "background".into(),
        context: None,
        state: Some("working"),
    });
    assert!(effects.snapshot.is_none());
    assert_eq!(state.snapshot.state, "idle");
}

#[test]
fn session_event_updates_one_record_and_projection_atomically() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some("selected".into());
    let effects = state.reduce(RuntimeEvent::Session {
        thread: "selected".into(),
        context: Some(SessionContextEvent::UserQuestion(
            "Keep this task isolated".into(),
        )),
        state: Some("working"),
    });

    let record = state.threads.get("selected").unwrap();
    assert_eq!(record.lifecycle.state.as_deref(), Some("working"));
    assert_eq!(
        record.context.question.as_deref(),
        Some("Keep this task isolated")
    );
    let snapshot = effects.snapshot.unwrap();
    assert_eq!(snapshot.state, "working");
    assert_eq!(
        snapshot.current_question.as_deref(),
        Some("Keep this task isolated")
    );
    assert_eq!(state.snapshot.current_question, snapshot.current_question);
}

#[test]
fn ipc_snapshot_updates_owner_lifecycle_and_capabilities_atomically() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some("selected".into());
    state.ipc_connected = true;
    let effects = state.reduce(RuntimeEvent::IpcSnapshot {
        thread: "selected".into(),
        control: ThreadControl {
            target: ControlTarget {
                conversation_id: "conversation".into(),
                owner_client_id: "owner".into(),
                host_id: Some("host".into()),
                cwd: None,
            },
            pending: Default::default(),
            notifications: Vec::new(),
            stale: false,
        },
        state: Some("working"),
    });

    assert_eq!(effects.snapshot.unwrap().state, "working");
    let controls = effects.control.unwrap();
    assert!(controls.can_stop);
    assert!(controls.can_reply);
    assert_eq!(
        state.threads["selected"]
            .control
            .as_ref()
            .unwrap()
            .target
            .owner_client_id,
        "owner"
    );
}

#[test]
fn terminal_record_rejects_late_session_context() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some("selected".into());
    state.threads.insert(
        "selected".into(),
        ThreadRecord {
            lifecycle: ThreadLifecycle {
                state: Some("completed".into()),
                terminal: true,
                ..Default::default()
            },
            context: ThreadContext {
                latest_update: Some("Final visible update".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let effects = state.reduce(RuntimeEvent::Session {
        thread: "selected".into(),
        context: Some(SessionContextEvent::AssistantUpdate(
            "Late background output".into(),
        )),
        state: Some("working"),
    });
    let record = state.threads.get("selected").unwrap();
    assert_eq!(record.lifecycle.state.as_deref(), Some("completed"));
    assert_eq!(
        record.context.latest_update.as_deref(),
        Some("Final visible update")
    );
    assert!(effects.snapshot.is_none());
}

#[test]
fn terminal_turn_ignores_late_jsonl_progress_until_a_new_turn() {
    let mut lifecycle = ThreadLifecycle::default();
    assert!(reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Jsonl),
    ));
    assert!(!reduce_lifecycle(&mut lifecycle, LifecycleEvent::Progress,));
    assert_eq!(lifecycle.state.as_deref(), Some("completed"));

    assert!(reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn,));
    assert_eq!(lifecycle.state.as_deref(), Some("working"));
    assert!(!lifecycle.terminal);
    assert_eq!(lifecycle.epoch, 1);
}

#[test]
fn authoritative_ipc_can_reopen_a_terminal_thread() {
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Jsonl),
    );
    assert!(reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("working", LifecycleSource::Ipc),
    ));
    assert_eq!(lifecycle.state.as_deref(), Some("working"));
    assert!(!lifecycle.terminal);
    assert_eq!(lifecycle.epoch, 1);
}

#[test]
fn equivalent_working_signals_from_multiple_sources_do_not_churn() {
    let mut lifecycle = ThreadLifecycle::default();
    assert!(reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("working", LifecycleSource::Jsonl),
    ));
    assert!(!reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("working", LifecycleSource::Ipc),
    ));
    assert!(!reduce_lifecycle(&mut lifecycle, LifecycleEvent::Progress,));
}

#[test]
fn background_control_never_becomes_the_selected_target() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some("visible".into());
    state.threads.insert(
        "background".into(),
        ThreadRecord {
            control: Some(ThreadControl {
                target: ControlTarget {
                    conversation_id: "background-secret".into(),
                    owner_client_id: "owner".into(),
                    host_id: Some("host".into()),
                    cwd: None,
                },
                pending: Default::default(),
                notifications: Vec::new(),
                stale: false,
            }),
            ..Default::default()
        },
    );
    let snapshot = control_snapshot(&state);
    assert!(!snapshot.can_reply);
    let serialized = serde_json::to_value(&snapshot).unwrap();
    assert!(serialized.get("available").is_none());
    assert!(serialized.get("threadIdHash").is_none());
}

#[test]
fn ipc_disconnect_does_not_replace_an_active_pet_state() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.snapshot.state = "working".into();
    state.snapshot.connected = true;
    let effects = state.reduce(RuntimeEvent::IpcConnectivity { connected: false });
    assert!(effects.snapshot.is_none());
    assert!(effects.control.is_some());
    assert_eq!(state.snapshot.state, "working");
    assert!(state.snapshot.connected);
    assert!(!state.ipc_connected);
}

#[test]
fn detects_stale_owner_router_errors() {
    assert!(is_stale_client_error("no client found"));
    assert!(is_stale_client_error("no-client-found"));
    assert!(is_stale_client_error("No client found for targetClientId"));
    assert!(is_stale_client_error("Not Found"));
    assert!(!is_stale_client_error(
        "cannot steer without an active turn"
    ));
}

#[test]
fn refresh_snapshot_rejects_the_stale_owner_and_wrong_host() {
    let refresh = TargetRefresh {
        stale_owner_client_id: "old-owner".into(),
        host_id: "expected-host".into(),
    };
    assert!(!snapshot_can_refresh_target(
        Some(&refresh),
        "old-owner",
        Some("expected-host")
    ));
    assert!(!snapshot_can_refresh_target(
        Some(&refresh),
        "new-owner",
        Some("wrong-host")
    ));
    assert!(snapshot_can_refresh_target(
        Some(&refresh),
        "new-owner",
        Some("expected-host")
    ));
    assert!(snapshot_can_refresh_target(None, "owner", None));
}

#[tokio::test]
async fn refreshes_stale_owner_before_retrying_live_follow_up() {
    let runtime = RuntimeHandle::default();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    *runtime.ipc.lock().expect("IPC sender poisoned") = Some(sender);
    let thread = hash_id("conversation");
    let stale_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "old-owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.selected = Some(thread.clone());
        state.ipc_connected = true;
        let mut lifecycle = ThreadLifecycle::default();
        reduce_lifecycle(
            &mut lifecycle,
            LifecycleEvent::State("working", LifecycleSource::Ipc),
        );
        state.threads.insert(
            thread.clone(),
            ThreadRecord {
                lifecycle,
                control: Some(ThreadControl {
                    target: stale_target.clone(),
                    pending: Default::default(),
                    notifications: Vec::new(),
                    stale: true,
                }),
                ..Default::default()
            },
        );
    }

    let retry = {
        let runtime = runtime.clone();
        let stale_target = stale_target.clone();
        let thread = thread.clone();
        tokio::spawn(async move {
            refresh_and_retry_follow_up(&runtime, &thread, &stale_target, "continue").await
        })
    };

    let refresh = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("refresh command timed out")
        .expect("refresh command missing");
    let IpcCommand::Broadcast {
        method,
        params,
        version,
        reply,
    } = refresh
    else {
        panic!("expected following refresh broadcast");
    };
    assert_eq!(method, "thread-stream-following-changed");
    assert_eq!(version, 1);
    assert_eq!(params["conversationId"], "conversation");
    assert_eq!(params["hostId"], "host");
    assert_eq!(params["following"], true);
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.threads.get_mut(&thread).unwrap().control = Some(ThreadControl {
            target: ControlTarget {
                conversation_id: "conversation".into(),
                owner_client_id: "new-owner".into(),
                host_id: Some("host".into()),
                cwd: Some("/workspace".into()),
            },
            pending: Default::default(),
            notifications: Vec::new(),
            stale: false,
        });
    }
    reply.send(Ok(())).expect("refresh reply dropped");

    let request = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("follow-up retry timed out")
        .expect("follow-up retry missing");
    let IpcCommand::Request {
        method,
        target_client_id,
        reply,
        ..
    } = request
    else {
        panic!("expected follow-up request");
    };
    assert_eq!(method, "thread-follower-steer-turn");
    assert_eq!(target_client_id.as_deref(), Some("new-owner"));
    reply.send(Ok(json!({}))).expect("request reply dropped");
    retry.await.expect("retry task panicked").unwrap();
}

#[tokio::test]
async fn stale_owner_retry_stops_when_selection_changes() {
    let runtime = RuntimeHandle::default();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    *runtime.ipc.lock().expect("IPC sender poisoned") = Some(sender);
    let old_thread = hash_id("conversation");
    let new_thread = hash_id("other-conversation");
    let stale_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "old-owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("working", LifecycleSource::Ipc),
    );
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.selected = Some(old_thread.clone());
        state.ipc_connected = true;
        state.threads.insert(
            old_thread.clone(),
            ThreadRecord {
                lifecycle: lifecycle.clone(),
                control: Some(ThreadControl {
                    target: stale_target.clone(),
                    pending: Default::default(),
                    notifications: Vec::new(),
                    stale: true,
                }),
                ..Default::default()
            },
        );
    }

    let retry = {
        let runtime = runtime.clone();
        let stale_target = stale_target.clone();
        let old_thread = old_thread.clone();
        tokio::spawn(async move {
            refresh_and_retry_follow_up(&runtime, &old_thread, &stale_target, "continue").await
        })
    };
    let refresh = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("refresh command timed out")
        .expect("refresh command missing");
    let IpcCommand::Broadcast { reply, .. } = refresh else {
        panic!("expected following refresh broadcast");
    };
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.threads.get_mut(&old_thread).unwrap().control = Some(ThreadControl {
            target: ControlTarget {
                conversation_id: "conversation".into(),
                owner_client_id: "new-owner".into(),
                host_id: Some("host".into()),
                cwd: Some("/workspace".into()),
            },
            pending: Default::default(),
            notifications: Vec::new(),
            stale: false,
        });
        state.threads.insert(
            new_thread.clone(),
            ThreadRecord {
                lifecycle,
                control: Some(ThreadControl {
                    target: ControlTarget {
                        conversation_id: "other-conversation".into(),
                        owner_client_id: "other-owner".into(),
                        host_id: Some("other-host".into()),
                        cwd: Some("/other".into()),
                    },
                    pending: Default::default(),
                    notifications: Vec::new(),
                    stale: false,
                }),
                ..Default::default()
            },
        );
        state.selected = Some(new_thread);
    }
    reply.send(Ok(())).expect("refresh reply dropped");

    let error = retry
        .await
        .expect("retry task panicked")
        .expect_err("old task retry must stop after selection changes");
    assert!(error.contains("selected"));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await
            .is_err()
    );
}

fn insert_controlled_thread(
    runtime: &RuntimeHandle,
    lifecycle: ThreadLifecycle,
    stale: bool,
    connected: bool,
) -> String {
    let thread = hash_id("conversation");
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some(thread.clone());
    state.ipc_connected = connected;
    state.threads.insert(
        thread.clone(),
        ThreadRecord {
            lifecycle,
            control: Some(ThreadControl {
                target: ControlTarget {
                    conversation_id: "conversation".into(),
                    owner_client_id: "owner".into(),
                    host_id: Some("host".into()),
                    cwd: Some("/workspace".into()),
                },
                pending: Default::default(),
                notifications: Vec::new(),
                stale,
            }),
            ..Default::default()
        },
    );
    thread
}

#[test]
fn completed_task_hides_steering_even_with_a_fresh_owner() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    insert_controlled_thread(&runtime, lifecycle, false, true);

    let state = runtime.state.lock().expect("runtime state poisoned");
    assert!(!control_snapshot(&state).can_reply);
    assert!(authorize_action(&state, ActionKind::Steer).is_err());
    assert!(authorize_action(&state, ActionKind::Stop).is_err());
}

#[test]
fn active_task_exposes_steering_only_with_a_live_owner() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    insert_controlled_thread(&runtime, lifecycle, false, true);

    let state = runtime.state.lock().expect("runtime state poisoned");
    let snapshot = control_snapshot(&state);
    assert!(snapshot.can_stop);
    assert!(snapshot.can_reply);
    let target = authorize_action(&state, ActionKind::Steer).unwrap().target;
    assert_eq!(target.conversation_id, "conversation");
    assert_eq!(
        authorize_action(&state, ActionKind::Stop)
            .unwrap()
            .target
            .conversation_id,
        "conversation"
    );
}

#[test]
fn selected_pending_control_requires_the_exact_live_request() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    let thread = insert_controlled_thread(&runtime, lifecycle, false, true);
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state
            .threads
            .get_mut(&thread)
            .unwrap()
            .control
            .as_mut()
            .unwrap()
            .pending
            .insert(
                "action".into(),
                PendingControl {
                    id: "action".into(),
                    conversation_id: "conversation".into(),
                    owner_client_id: "owner".into(),
                    request_id: "request".into(),
                    kind: PendingKind::Command,
                },
            );
    }

    let state = runtime.state.lock().expect("runtime state poisoned");
    assert_eq!(
        authorize_action(&state, ActionKind::Respond("action"))
            .unwrap()
            .pending
            .unwrap()
            .request_id,
        "request"
    );
    assert!(authorize_action(&state, ActionKind::Respond("other")).is_err());
}

#[test]
fn late_control_response_does_not_clear_a_replaced_request() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    let thread = insert_controlled_thread(&runtime, lifecycle, false, true);
    let old = PendingControl {
        id: "action".into(),
        conversation_id: "conversation".into(),
        owner_client_id: "owner".into(),
        request_id: "old-request".into(),
        kind: PendingKind::Command,
    };
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        let pending = &mut state
            .threads
            .get_mut(&thread)
            .unwrap()
            .control
            .as_mut()
            .unwrap()
            .pending;
        pending.insert(old.id.clone(), old.clone());
        pending.insert(
            old.id.clone(),
            PendingControl {
                request_id: "new-request".into(),
                ..old.clone()
            },
        );
        assert!(!complete_control_action(&mut state, &old));
        assert_eq!(
            state.threads[&thread].control.as_ref().unwrap().pending["action"].request_id,
            "new-request"
        );
    }
}

#[test]
fn ipc_reconnect_restores_capabilities_from_the_same_authorization() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    insert_controlled_thread(&runtime, lifecycle, false, true);
    let mut state = runtime.state.lock().expect("runtime state poisoned");

    state.reduce(RuntimeEvent::IpcConnectivity { connected: false });
    assert!(!control_snapshot(&state).can_reply);
    assert!(authorize_action(&state, ActionKind::Steer).is_err());

    state.reduce(RuntimeEvent::IpcConnectivity { connected: true });
    assert!(control_snapshot(&state).can_reply);
    assert!(authorize_action(&state, ActionKind::Steer).is_ok());
}

#[test]
fn only_working_lifecycle_exposes_active_turn_controls() {
    for state_name in [
        "idle",
        "reviewing",
        "completed",
        "failed",
        "interrupted",
        "unknown-state",
    ] {
        let runtime = RuntimeHandle::default();
        let mut lifecycle = ThreadLifecycle::default();
        reduce_lifecycle(
            &mut lifecycle,
            LifecycleEvent::State(state_name, LifecycleSource::Ipc),
        );
        insert_controlled_thread(&runtime, lifecycle, false, true);

        let state = runtime.state.lock().expect("runtime state poisoned");
        let snapshot = control_snapshot(&state);
        assert!(!snapshot.can_stop, "{state_name} exposed stop");
        assert!(!snapshot.can_reply, "{state_name} exposed steering");
        assert!(authorize_action(&state, ActionKind::Steer).is_err());
    }
}

#[test]
fn stale_or_disconnected_owner_hides_steering() {
    for (stale, connected) in [(true, true), (false, false)] {
        let runtime = RuntimeHandle::default();
        let mut lifecycle = ThreadLifecycle::default();
        reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
        insert_controlled_thread(&runtime, lifecycle, stale, connected);

        let state = runtime.state.lock().expect("runtime state poisoned");
        assert!(!control_snapshot(&state).can_reply);
        assert!(authorize_action(&state, ActionKind::Steer).is_err());
        assert!(authorize_action(&state, ActionKind::Stop).is_err());
    }
}

#[tokio::test]
async fn steering_never_falls_back_to_starting_a_new_turn() {
    let runtime = RuntimeHandle::default();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    *runtime.ipc.lock().expect("IPC sender poisoned") = Some(sender);
    let target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    let task = {
        let runtime = runtime.clone();
        let target = target.clone();
        tokio::spawn(async move { dispatch_steering(&runtime, &target, "continue").await })
    };
    let request = receiver.recv().await.expect("steering request missing");
    let IpcCommand::Request { method, reply, .. } = request else {
        panic!("expected steering request");
    };
    assert_eq!(method, "thread-follower-steer-turn");
    reply
        .send(Err("cannot steer without an active turn".into()))
        .expect("request reply dropped");
    assert!(task.await.expect("steering task panicked").is_err());
    assert!(receiver.try_recv().is_err(), "start-turn must not be sent");
}

#[tokio::test]
async fn stop_dispatch_targets_only_the_selected_owner() {
    let runtime = RuntimeHandle::default();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    *runtime.ipc.lock().expect("IPC sender poisoned") = Some(sender);
    let target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    let task = {
        let runtime = runtime.clone();
        let target = target.clone();
        tokio::spawn(async move { dispatch_stop(&runtime, &target).await })
    };
    let request = receiver.recv().await.expect("stop request missing");
    let IpcCommand::Request {
        method,
        params,
        target_client_id,
        reply,
    } = request
    else {
        panic!("expected stop request");
    };
    assert_eq!(method, "thread-follower-interrupt-turn");
    assert_eq!(params["conversationId"], "conversation");
    assert_eq!(target_client_id.as_deref(), Some("owner"));
    reply.send(Ok(json!({}))).expect("request reply dropped");
    task.await.expect("stop task panicked").unwrap();
    assert!(receiver.try_recv().is_err());
}
