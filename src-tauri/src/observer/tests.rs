use super::{
    ActionKind, FollowUpDispatchGuard, FollowUpKind, IpcCommand, LifecycleEvent, LifecycleSource,
    OWNER_UNAVAILABLE_MESSAGE, RuntimeEvent, RuntimeHandle, SessionContextEvent, TargetRefresh,
    TargetRefreshPhase, ThreadContext, ThreadControl, ThreadLifecycle, ThreadRecord,
    apply_context_event, arm_target_refresh, authorize_action, authorize_cdp_follow_up,
    authorize_follow_up, authorize_ipc_follow_up_dispatch, build_runtime_snapshot,
    cdp_verification_within_deadline, compact_tail_text, complete_control_action, control_snapshot,
    dispatch_ready_follow_up, dispatch_steering, dispatch_stop, follow_up_error_class, hash_id,
    is_stale_client_error, is_terminal_state, mark_target_stale, poll_cdp_verification_until_ready,
    prepare_follow_up_after_foreground_selection, prepare_follow_up_attempt,
    reauthorize_refreshed_follow_up, reduce_lifecycle, refresh_and_retry_follow_up,
    release_follow_up_inflight, snapshot_can_refresh_target,
};
use crate::cdp::{CdpEndpoint, ControlTransport};
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
    assert_eq!(
        OWNER_UNAVAILABLE_MESSAGE,
        "Codex reported this task's owner unavailable. Keep it open in Codex, then retry."
    );
}

#[test]
fn classifies_follow_up_errors_without_retaining_error_text() {
    assert_eq!(follow_up_error_class("no client found"), "owner-not-found");
    assert_eq!(
        follow_up_error_class("Codex task owner is reconnecting"),
        "owner-reconnecting"
    );
    assert_eq!(
        follow_up_error_class("The selected Codex task has no owner"),
        "owner-missing"
    );
    assert_eq!(
        follow_up_error_class("The selected Codex task is not running"),
        "lifecycle"
    );
    assert_eq!(
        follow_up_error_class("Codex IPC is disconnected"),
        "ipc-unavailable"
    );
}

#[tokio::test]
async fn selected_ready_task_waits_for_its_first_matching_owner_snapshot() {
    let runtime = RuntimeHandle::default();
    let thread = hash_id("selected-conversation");
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.selected = Some(thread.clone());
        state.ipc_connected = true;
        let mut lifecycle = ThreadLifecycle::default();
        reduce_lifecycle(
            &mut lifecycle,
            LifecycleEvent::State("completed", LifecycleSource::Ipc),
        );
        state.threads.insert(
            thread.clone(),
            ThreadRecord {
                lifecycle,
                ..Default::default()
            },
        );
    }

    let wait_for_owner = {
        let runtime = runtime.clone();
        let thread = thread.clone();
        tokio::spawn(async move {
            prepare_follow_up_after_foreground_selection(&runtime, &thread).await
        })
    };
    tokio::time::sleep(Duration::from_millis(80)).await;

    let expected_target = ControlTarget {
        conversation_id: "selected-conversation".into(),
        owner_client_id: "selected-owner".into(),
        host_id: Some("selected-host".into()),
        cwd: None,
    };
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.reduce(RuntimeEvent::IpcSnapshot {
            thread: thread.clone(),
            control: ThreadControl {
                target: expected_target.clone(),
                pending: Default::default(),
                notifications: Vec::new(),
                stale: false,
            },
            state: Some("completed"),
        });
    }

    let attempt = tokio::time::timeout(Duration::from_secs(1), wait_for_owner)
        .await
        .expect("owner discovery did not finish")
        .expect("owner discovery task panicked")
        .expect("matching owner snapshot should authorize follow-up");
    assert_eq!(attempt.thread, thread);
    assert_eq!(attempt.kind, FollowUpKind::Start);
    assert_eq!(
        attempt.target.conversation_id,
        expected_target.conversation_id
    );
    assert_eq!(
        attempt.target.owner_client_id,
        expected_target.owner_client_id
    );
    assert_eq!(attempt.target.host_id, expected_target.host_id);
}

#[tokio::test]
async fn selected_owner_discovery_stops_when_the_foreground_task_changes() {
    let runtime = RuntimeHandle::default();
    let thread = hash_id("selected-conversation");
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.selected = Some(thread.clone());
        state.ipc_connected = true;
        let mut lifecycle = ThreadLifecycle::default();
        reduce_lifecycle(
            &mut lifecycle,
            LifecycleEvent::State("completed", LifecycleSource::Ipc),
        );
        state.threads.insert(
            thread.clone(),
            ThreadRecord {
                lifecycle,
                ..Default::default()
            },
        );
    }

    let wait_for_owner = {
        let runtime = runtime.clone();
        let thread = thread.clone();
        tokio::spawn(async move {
            prepare_follow_up_after_foreground_selection(&runtime, &thread).await
        })
    };
    tokio::time::sleep(Duration::from_millis(80)).await;
    runtime
        .state
        .lock()
        .expect("runtime state poisoned")
        .selected = Some("other-thread".into());

    let result = tokio::time::timeout(Duration::from_secs(1), wait_for_owner)
        .await
        .expect("owner discovery did not finish")
        .expect("owner discovery task panicked");
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("selection change must reject the pending follow-up"),
    };
    assert_eq!(
        error,
        "The selected Codex task changed before its owner appeared"
    );
}

#[test]
fn stale_error_never_invalidates_a_replaced_owner() {
    let runtime = RuntimeHandle::default();
    let thread = hash_id("conversation");
    let fallback_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "old-owner".into(),
        host_id: Some("host".into()),
        cwd: None,
    };
    let current_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "current-owner".into(),
        host_id: Some("host".into()),
        cwd: None,
    };
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some(thread.clone());
    state.threads.insert(
        thread.clone(),
        ThreadRecord {
            control: Some(ThreadControl {
                target: current_target.clone(),
                pending: Default::default(),
                notifications: Vec::new(),
                stale: false,
            }),
            ..Default::default()
        },
    );
    assert!(!mark_target_stale(&mut state, &thread, &fallback_target));
    let record = state.threads.get(&thread).unwrap();
    assert!(!record.control.as_ref().unwrap().stale);
    assert!(record.target_refresh.is_none());

    assert!(mark_target_stale(&mut state, &thread, &current_target));
    let record = state.threads.get(&thread).unwrap();
    assert!(record.control.as_ref().unwrap().stale);
    assert_eq!(
        record
            .target_refresh
            .as_ref()
            .map(|refresh| refresh.stale_owner_client_id.as_str()),
        Some("current-owner")
    );
}

#[test]
fn refresh_snapshot_accepts_same_owner_only_after_follow_refresh() {
    let mut refresh = TargetRefresh {
        stale_owner_client_id: "old-owner".into(),
        host_id: "expected-host".into(),
        phase: TargetRefreshPhase::Pending,
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
    refresh.phase = TargetRefreshPhase::AwaitingSnapshot;
    assert!(snapshot_can_refresh_target(
        Some(&refresh),
        "old-owner",
        Some("expected-host")
    ));
    assert!(!snapshot_can_refresh_target(
        Some(&refresh),
        "old-owner",
        None
    ));
    assert!(snapshot_can_refresh_target(None, "owner", None));
}

#[test]
fn follow_refresh_arms_only_the_selected_stale_target() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    let thread = insert_controlled_thread(&runtime, lifecycle, true, true);
    let target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.threads.get_mut(&thread).unwrap().target_refresh = Some(TargetRefresh {
        stale_owner_client_id: target.owner_client_id.clone(),
        host_id: "host".into(),
        phase: TargetRefreshPhase::Pending,
    });
    arm_target_refresh(&mut state, &thread, &target).unwrap();
    assert_eq!(
        state.threads[&thread]
            .target_refresh
            .as_ref()
            .unwrap()
            .phase,
        TargetRefreshPhase::AwaitingSnapshot
    );
    state.threads.get_mut(&thread).unwrap().target_refresh = Some(TargetRefresh {
        stale_owner_client_id: target.owner_client_id.clone(),
        host_id: "host".into(),
        phase: TargetRefreshPhase::Pending,
    });
    state.selected = Some("other-thread".into());
    assert!(arm_target_refresh(&mut state, &thread, &target).is_err());
    assert_eq!(
        state.threads[&thread]
            .target_refresh
            .as_ref()
            .unwrap()
            .phase,
        TargetRefreshPhase::Pending
    );
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
            refresh_and_retry_follow_up(
                &runtime,
                &thread,
                &stale_target,
                "continue",
                FollowUpKind::Steer,
            )
            .await
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
        ..
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
async fn refreshes_stale_owner_before_retrying_ready_follow_up() {
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
            LifecycleEvent::State("completed", LifecycleSource::Ipc),
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
            refresh_and_retry_follow_up(
                &runtime,
                &thread,
                &stale_target,
                "continue",
                FollowUpKind::Start,
            )
            .await
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
        .expect("ready follow-up retry timed out")
        .expect("ready follow-up retry missing");
    let IpcCommand::Request {
        method,
        params,
        version,
        target_client_id,
        reply,
        ..
    } = request
    else {
        panic!("expected ready follow-up request");
    };
    assert_eq!(method, "thread-follower-start-turn");
    assert_eq!(version, 0);
    assert_eq!(params["turnStartParams"]["input"][0]["text"], "continue");
    assert_eq!(target_client_id.as_deref(), Some("new-owner"));
    reply.send(Ok(json!({}))).expect("request reply dropped");
    retry.await.expect("retry task panicked").unwrap();
}

#[test]
fn refreshed_follow_up_rechecks_exact_selection_before_dispatch() {
    let runtime = RuntimeHandle::default();
    let thread = hash_id("conversation");
    let stale_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "stale-owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    let refreshed_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "fresh-owner".into(),
        host_id: Some("host".into()),
        cwd: Some("/workspace".into()),
    };
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some(thread.clone());
    state.ipc_connected = true;
    state.threads.insert(
        thread.clone(),
        ThreadRecord {
            lifecycle,
            control: Some(ThreadControl {
                target: refreshed_target.clone(),
                pending: Default::default(),
                notifications: Vec::new(),
                stale: false,
            }),
            ..Default::default()
        },
    );
    assert!(
        reauthorize_refreshed_follow_up(
            &state,
            &thread,
            &stale_target,
            &refreshed_target,
            FollowUpKind::Start,
        )
        .is_ok()
    );

    state.selected = Some("another-task".into());
    assert_eq!(
        reauthorize_refreshed_follow_up(
            &state,
            &thread,
            &stale_target,
            &refreshed_target,
            FollowUpKind::Start,
        )
        .unwrap_err(),
        "The selected Codex task changed while reconnecting"
    );
}

#[tokio::test]
async fn ready_follow_up_retries_after_a_same_owner_follow_refresh() {
    let runtime = RuntimeHandle::default();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    *runtime.ipc.lock().expect("IPC sender poisoned") = Some(sender);
    let thread = hash_id("conversation");
    let stale_target = ControlTarget {
        conversation_id: "conversation".into(),
        owner_client_id: "owner".into(),
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
            LifecycleEvent::State("completed", LifecycleSource::Ipc),
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
                target_refresh: Some(TargetRefresh {
                    stale_owner_client_id: stale_target.owner_client_id.clone(),
                    host_id: "host".into(),
                    phase: TargetRefreshPhase::Pending,
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
            refresh_and_retry_follow_up(
                &runtime,
                &thread,
                &stale_target,
                "continue",
                FollowUpKind::Start,
            )
            .await
        })
    };

    let refresh = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("refresh command timed out")
        .expect("refresh command missing");
    let IpcCommand::Broadcast {
        follow_refresh,
        reply,
        ..
    } = refresh
    else {
        panic!("expected following refresh broadcast");
    };
    let follow_refresh = follow_refresh.expect("follow refresh marker missing");
    assert_eq!(follow_refresh.thread, thread);
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        arm_target_refresh(&mut state, &follow_refresh.thread, &follow_refresh.target).unwrap();
        let record = state.threads.get_mut(&thread).unwrap();
        assert!(snapshot_can_refresh_target(
            record.target_refresh.as_ref(),
            "owner",
            Some("host")
        ));
        record.control = Some(ThreadControl {
            target: stale_target.clone(),
            pending: Default::default(),
            notifications: Vec::new(),
            stale: false,
        });
        record.target_refresh = None;
    }
    reply.send(Ok(())).expect("refresh reply dropped");

    let request = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("ready follow-up retry timed out")
        .expect("ready follow-up retry missing");
    let IpcCommand::Request {
        method,
        target_client_id,
        reply,
        ..
    } = request
    else {
        panic!("expected ready follow-up request");
    };
    assert_eq!(method, "thread-follower-start-turn");
    assert_eq!(target_client_id.as_deref(), Some("owner"));
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
            refresh_and_retry_follow_up(
                &runtime,
                &old_thread,
                &stale_target,
                "continue",
                FollowUpKind::Steer,
            )
            .await
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
fn completed_task_exposes_only_ready_follow_up_with_a_fresh_owner() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    insert_controlled_thread(&runtime, lifecycle, false, true);

    let state = runtime.state.lock().expect("runtime state poisoned");
    let snapshot = control_snapshot(&state);
    assert!(!snapshot.can_stop);
    assert!(!snapshot.can_reply);
    assert!(snapshot.can_start_follow_up);
    assert!(!snapshot.show_working_follow_up);
    assert!(snapshot.show_ready_follow_up);
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap()["showReadyFollowUp"],
        true
    );
    assert!(authorize_action(&state, ActionKind::Steer).is_err());
    assert!(authorize_action(&state, ActionKind::Stop).is_err());
    assert!(authorize_action(&state, ActionKind::StartFollowUp).is_ok());
    assert_eq!(authorize_follow_up(&state).unwrap().1, FollowUpKind::Start);
}

#[test]
fn cdp_ready_allows_selected_completed_task_without_a_fresh_owner() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    insert_controlled_thread(&runtime, lifecycle, true, false);

    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: Some(52_000),
        process_id: Some(4_242),
    });
    let snapshot = control_snapshot(&state);
    assert!(snapshot.can_start_follow_up);
    assert!(!snapshot.can_reply);
    assert!(!snapshot.can_stop);
    assert_eq!(snapshot.transport, ControlTransport::CdpReady);
    assert_eq!(
        authorize_cdp_follow_up(&state).unwrap().1,
        FollowUpKind::Start
    );
    assert_eq!(authorize_follow_up(&state).unwrap().1, FollowUpKind::Start);
}

#[test]
fn cdp_ready_allows_selected_working_task_without_a_fresh_owner() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    insert_controlled_thread(&runtime, lifecycle, true, false);

    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: Some(52_001),
        process_id: Some(4_243),
    });
    let snapshot = control_snapshot(&state);
    assert!(snapshot.can_reply);
    assert!(!snapshot.can_start_follow_up);
    assert!(!snapshot.can_stop);
    assert_eq!(
        authorize_cdp_follow_up(&state).unwrap().1,
        FollowUpKind::Steer
    );
}

#[test]
fn cdp_degraded_keeps_exact_ipc_owner_requirement() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    insert_controlled_thread(&runtime, lifecycle, true, false);

    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpDegraded,
        port: None,
        process_id: None,
    });
    assert!(!control_snapshot(&state).can_start_follow_up);
    assert!(authorize_follow_up(&state).is_err());
}

#[tokio::test]
async fn cdp_verification_deadline_is_hard() {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1);
    let verified = cdp_verification_within_deadline(deadline, async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok::<(), String>(())
    })
    .await;

    assert!(!verified, "a completed-after-deadline probe must fail");
}

#[tokio::test]
async fn cdp_verification_retries_transient_failures_within_one_hard_deadline() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let attempts = Arc::new(AtomicUsize::new(0));
    let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
    let verified = poll_cdp_verification_until_ready(
        deadline,
        Duration::from_millis(20),
        Duration::from_millis(1),
        |attempt_deadline| {
            let attempts = attempts.clone();
            async move {
                assert!(attempt_deadline <= deadline);
                attempts.fetch_add(1, Ordering::SeqCst) >= 1
            }
        },
    )
    .await;

    assert!(verified);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[test]
fn cdp_degraded_retry_requires_the_same_live_tracked_endpoint() {
    let runtime = RuntimeHandle::default();
    let process_id = 4_242;
    let port = 52_000;
    let endpoint = CdpEndpoint::attached(process_id, port);
    let liveness_token = runtime.arm_cdp_process_liveness(process_id);
    runtime.remember_cdp_endpoint(endpoint);
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.reduce(RuntimeEvent::ControlTransport {
            transport: ControlTransport::CdpDegraded,
            port: None,
            process_id: Some(process_id),
        });
        assert!(state.cdp_port.is_none());
    }

    assert_eq!(
        runtime.cdp_endpoint_for_retry().unwrap().port,
        port,
        "the retry endpoint remains native-only while the public state is degraded"
    );
    assert!(runtime.cdp_endpoint_matches(endpoint));
    assert!(!runtime.cdp_endpoint_matches(CdpEndpoint::attached(process_id, port + 1)));

    assert!(runtime.revoke_cdp_process_liveness(process_id, liveness_token));
    assert!(runtime.cdp_endpoint_for_retry().is_err());
}

#[test]
fn cdp_liveness_token_is_bound_to_the_exact_tracked_endpoint() {
    let runtime = RuntimeHandle::default();
    let endpoint = CdpEndpoint::attached(4_243, 52_001);
    let token = runtime.arm_cdp_process_liveness(endpoint.process_id);
    runtime.remember_cdp_endpoint(endpoint);

    assert_eq!(
        runtime.cdp_liveness_token_for_endpoint(endpoint),
        Some(token)
    );
    assert_eq!(
        runtime.cdp_liveness_token_for_endpoint(CdpEndpoint::attached(
            endpoint.process_id,
            endpoint.port + 1,
        )),
        None
    );
}

#[test]
fn cdp_ready_without_native_port_degrades_fail_closed() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: None,
        process_id: Some(4_244),
    });
    assert_eq!(state.control_transport, ControlTransport::CdpDegraded);
    assert!(state.cdp_port.is_none());
}

#[test]
fn cdp_ready_without_a_managed_process_degrades_fail_closed() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: Some(52_004),
        process_id: None,
    });
    assert_eq!(state.control_transport, ControlTransport::CdpDegraded);
    assert!(state.cdp_port.is_none());
    assert!(state.cdp_process_id.is_none());
}

#[test]
fn managed_cdp_process_exit_revokes_the_bridge_generation() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: Some(52_005),
        process_id: Some(4_247),
    });
    let generation = state.transport_generation;
    state.reduce(RuntimeEvent::CdpProcessExited { process_id: 4_247 });
    assert_eq!(state.control_transport, ControlTransport::CdpDegraded);
    assert!(state.cdp_port.is_none());
    assert!(state.cdp_process_id.is_none());
    assert_eq!(state.transport_generation, generation.wrapping_add(1));
}

#[test]
fn stale_cdp_liveness_token_cannot_revoke_a_newer_process_record() {
    let runtime = RuntimeHandle::default();
    let old = runtime.arm_cdp_process_liveness(4_248);
    let current = runtime.arm_cdp_process_liveness(4_248);
    assert!(!runtime.revoke_cdp_process_liveness(4_248, old));
    assert!(runtime.cdp_process_is_live(4_248));
    assert!(runtime.revoke_cdp_process_liveness(4_248, current));
    assert!(!runtime.cdp_process_is_live(4_248));
}

#[test]
fn cdp_ready_still_requires_exact_native_workspace_and_host() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    let thread = insert_controlled_thread(&runtime, lifecycle, true, false);

    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: Some(52_002),
        process_id: Some(4_245),
    });
    state
        .threads
        .get_mut(&thread)
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .target
        .cwd = None;
    assert!(!control_snapshot(&state).can_start_follow_up);
    assert!(authorize_cdp_follow_up(&state).is_err());
    state
        .threads
        .get_mut(&thread)
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .target
        .cwd = Some("/workspace".into());
    state
        .threads
        .get_mut(&thread)
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .target
        .host_id = None;
    assert!(authorize_cdp_follow_up(&state).is_err());
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
    assert!(!snapshot.can_start_follow_up);
    assert!(snapshot.show_working_follow_up);
    assert!(!snapshot.show_ready_follow_up);
    let target = authorize_action(&state, ActionKind::Steer).unwrap().target;
    assert_eq!(target.conversation_id, "conversation");
    assert_eq!(
        authorize_action(&state, ActionKind::Stop)
            .unwrap()
            .target
            .conversation_id,
        "conversation"
    );
    assert!(authorize_action(&state, ActionKind::StartFollowUp).is_err());
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
        assert!(
            !snapshot.can_start_follow_up,
            "{state_name} exposed ready follow-up"
        );
        assert!(
            !snapshot.show_ready_follow_up,
            "{state_name} exposed ready follow-up display"
        );
        assert!(
            !snapshot.show_working_follow_up,
            "{state_name} exposed working follow-up display"
        );
        assert!(authorize_action(&state, ActionKind::Steer).is_err());
        assert!(authorize_action(&state, ActionKind::StartFollowUp).is_err());
    }
}

#[test]
fn stale_or_disconnected_owner_keeps_working_or_ready_follow_up_visible_but_unavailable() {
    for (ready, stale, connected) in [
        (false, true, true),
        (false, false, false),
        (true, true, true),
        (true, false, false),
    ] {
        let runtime = RuntimeHandle::default();
        let mut lifecycle = ThreadLifecycle::default();
        if ready {
            reduce_lifecycle(
                &mut lifecycle,
                LifecycleEvent::State("completed", LifecycleSource::Ipc),
            );
        } else {
            reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
        }
        insert_controlled_thread(&runtime, lifecycle, stale, connected);

        let state = runtime.state.lock().expect("runtime state poisoned");
        assert!(!control_snapshot(&state).can_reply);
        assert!(!control_snapshot(&state).can_start_follow_up);
        assert_eq!(control_snapshot(&state).show_working_follow_up, !ready);
        assert_eq!(control_snapshot(&state).show_ready_follow_up, ready);
        assert!(authorize_action(&state, ActionKind::Steer).is_err());
        assert!(authorize_action(&state, ActionKind::StartFollowUp).is_err());
        assert!(authorize_action(&state, ActionKind::Stop).is_err());
    }
}

#[test]
fn stale_selected_task_retries_the_exact_follow_target_before_follow_up() {
    for (lifecycle_state, expected_kind) in [
        ("working", FollowUpKind::Steer),
        ("completed", FollowUpKind::Start),
    ] {
        let runtime = RuntimeHandle::default();
        let thread = hash_id("conversation");
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "stale-owner".into(),
            host_id: Some("host".into()),
            cwd: None,
        };
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state.selected = Some(thread.clone());
        state.ipc_connected = true;
        let mut lifecycle = ThreadLifecycle::default();
        reduce_lifecycle(
            &mut lifecycle,
            LifecycleEvent::State(lifecycle_state, LifecycleSource::Ipc),
        );
        state.threads.insert(
            thread.clone(),
            ThreadRecord {
                lifecycle,
                control: Some(ThreadControl {
                    target: target.clone(),
                    pending: Default::default(),
                    notifications: Vec::new(),
                    stale: true,
                }),
                target_refresh: Some(TargetRefresh {
                    stale_owner_client_id: target.owner_client_id.clone(),
                    host_id: "host".into(),
                    phase: TargetRefreshPhase::AwaitingSnapshot,
                }),
                ..Default::default()
            },
        );

        let attempt = prepare_follow_up_attempt(&mut state).unwrap();
        assert!(attempt.refresh_before_dispatch);
        assert_eq!(attempt.thread, thread);
        assert_eq!(attempt.target.conversation_id, "conversation");
        assert_eq!(attempt.target.host_id.as_deref(), Some("host"));
        assert_eq!(attempt.kind, expected_kind);
        assert!(state.follow_up_inflight.contains_key(&thread));
        assert_eq!(
            state.threads[&thread]
                .target_refresh
                .as_ref()
                .unwrap()
                .phase,
            TargetRefreshPhase::Pending
        );
    }
}

#[test]
fn cdp_attempt_skips_stale_ipc_refresh_and_freezes_transport_generation() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    let thread = insert_controlled_thread(&runtime, lifecycle, true, false);
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.reduce(RuntimeEvent::ControlTransport {
        transport: ControlTransport::CdpReady,
        port: Some(52_003),
        process_id: Some(4_246),
    });

    let attempt = prepare_follow_up_attempt(&mut state).unwrap();
    assert_eq!(attempt.thread, thread);
    assert_eq!(attempt.transport, ControlTransport::CdpReady);
    assert_eq!(attempt.transport_generation, state.transport_generation);
    assert!(!attempt.refresh_before_dispatch);
    assert!(state.threads[&thread].target_refresh.is_none());
}

#[test]
fn ipc_follow_up_dispatch_guard_rejects_late_selection_owner_and_transport_changes() {
    let runtime = RuntimeHandle::default();
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(&mut lifecycle, LifecycleEvent::NewTurn);
    let thread = insert_controlled_thread(&runtime, lifecycle, false, true);
    let guard = {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        let attempt = prepare_follow_up_attempt(&mut state).unwrap();
        FollowUpDispatchGuard {
            thread: attempt.thread,
            target: attempt.target,
            kind: attempt.kind,
            transport: attempt.transport,
            transport_generation: attempt.transport_generation,
        }
    };

    {
        let state = runtime.state.lock().expect("runtime state poisoned");
        assert!(authorize_ipc_follow_up_dispatch(&state, &guard).is_ok());
    }
    {
        let mut state = runtime.state.lock().expect("runtime state poisoned");
        state
            .threads
            .get_mut(&thread)
            .unwrap()
            .control
            .as_mut()
            .unwrap()
            .target
            .owner_client_id = "replacement-owner".into();
        assert!(authorize_ipc_follow_up_dispatch(&state, &guard).is_err());
        state
            .threads
            .get_mut(&thread)
            .unwrap()
            .control
            .as_mut()
            .unwrap()
            .target = guard.target.clone();
        state.reduce(RuntimeEvent::ControlTransport {
            transport: ControlTransport::CdpDegraded,
            port: None,
            process_id: None,
        });
        assert!(authorize_ipc_follow_up_dispatch(&state, &guard).is_err());
    }
}

#[test]
fn old_follow_up_guard_cannot_clear_a_newer_request_token() {
    let runtime = RuntimeHandle::default();
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.follow_up_inflight.insert("selected".into(), 22);
    release_follow_up_inflight(&mut state, "selected", 21);
    assert_eq!(state.follow_up_inflight.get("selected"), Some(&22));
    release_follow_up_inflight(&mut state, "selected", 22);
    assert!(!state.follow_up_inflight.contains_key("selected"));
}

#[test]
fn stale_selected_task_without_host_stays_fail_closed() {
    let runtime = RuntimeHandle::default();
    let thread = hash_id("conversation");
    let mut lifecycle = ThreadLifecycle::default();
    reduce_lifecycle(
        &mut lifecycle,
        LifecycleEvent::State("completed", LifecycleSource::Ipc),
    );
    let mut state = runtime.state.lock().expect("runtime state poisoned");
    state.selected = Some(thread.clone());
    state.ipc_connected = true;
    state.threads.insert(
        thread,
        ThreadRecord {
            lifecycle,
            control: Some(ThreadControl {
                target: ControlTarget {
                    conversation_id: "conversation".into(),
                    owner_client_id: "stale-owner".into(),
                    host_id: None,
                    cwd: None,
                },
                pending: Default::default(),
                notifications: Vec::new(),
                stale: true,
            }),
            ..Default::default()
        },
    );

    let error = match prepare_follow_up_attempt(&mut state) {
        Err(error) => error,
        Ok(_) => panic!("stale target without a host must not be retried"),
    };
    assert_eq!(error, OWNER_UNAVAILABLE_MESSAGE);
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
async fn ready_follow_up_dispatches_start_turn_to_the_selected_owner() {
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
        tokio::spawn(async move { dispatch_ready_follow_up(&runtime, &target, "continue").await })
    };
    let request = receiver
        .recv()
        .await
        .expect("ready follow-up request missing");
    let IpcCommand::Request {
        method,
        params,
        version,
        target_client_id,
        reply,
        ..
    } = request
    else {
        panic!("expected ready follow-up request");
    };
    assert_eq!(method, "thread-follower-start-turn");
    assert_eq!(version, 0);
    assert_eq!(params["conversationId"], "conversation");
    assert_eq!(params["turnStartParams"]["input"][0]["text"], "continue");
    assert_eq!(target_client_id.as_deref(), Some("owner"));
    reply.send(Ok(json!({}))).expect("request reply dropped");
    task.await.expect("ready follow-up task panicked").unwrap();
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
        version,
        target_client_id,
        reply,
        ..
    } = request
    else {
        panic!("expected stop request");
    };
    assert_eq!(method, "thread-follower-interrupt-turn");
    assert_eq!(version, 0);
    assert_eq!(params["conversationId"], "conversation");
    assert_eq!(target_client_id.as_deref(), Some("owner"));
    reply.send(Ok(json!({}))).expect("request reply dropped");
    task.await.expect("stop task panicked").unwrap();
    assert!(receiver.try_recv().is_err());
}
