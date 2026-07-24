# Multi-session state arbitration

> Status: Normative
> Owns: Per-task lifecycle, selection arbitration, control isolation, and fallback invariants
> Update when: Selection authority, epoch reduction, task context ownership, or control routing changes
> Last verified: 2026-07-24

## Problem

Codex App can keep several conversations alive at once. CoPets receives three independent,
partially overlapping streams:

| Source | Authority | Purpose |
| --- | --- | --- |
| App owner route/activity log | Selection only | Identifies the routed foreground conversation view |
| App IPC follower snapshot | Current runtime and controls | Authoritative active/terminal state, approvals, input, stop and reply target |
| Session JSONL | Turn lifecycle and display context | User question, task start, progress summary and terminal events |

These streams are not globally ordered. Treating the last event as global truth allows a background
conversation or a late JSONL record to overwrite the visible task.

Only the native adapters and reducer implement this policy. The standalone Node probes emit
sanitized compatibility evidence and have no lifecycle reducer or selected-task authority.

## Invariants

1. Every conversation owns an independent lifecycle record and display context.
2. Background conversation updates never change the visible pet.
3. A known `/local/<conversation>` owner route is the primary selection authority. Focused, visible
   activity is only a compatibility fallback when no owner route is current; an explicit
   `ownerRoutePath=/` invalidates the cached route and activity. An unindexed canonical-UUID
   conversation additionally requires explicit owner-stream evidence.
4. `active=false` is a weak negative signal. It never clears the visible pet by itself.
5. A terminal state seals its current epoch. Late JSONL progress cannot reopen it.
6. A new user question starts a new epoch. An authoritative IPC `active` snapshot may also reopen a
   terminal conversation when JSONL has not arrived yet.
7. Equivalent states from different sources do not produce duplicate UI events.
8. Approval, answer, stop and follow-up actions resolve only against the exact selected conversation.
   There is no global "last controllable conversation" fallback.
9. Steering revalidates the last confirmed foreground selection immediately before dispatch and
   targets only that conversation's live follower owner.
10. Steering exists only while the selected turn is active and its IPC owner is connected. A
    terminal, stale, or disconnected task hides the control; CoPets never starts a new turn or
    activates Codex App as a fallback.

## State model

Each hashed conversation ID maps to:

```text
ThreadRecord
  lifecycle
    state: working | reviewing | completed | failed | interrupted
    epoch: monotonically increasing local turn generation
    terminal: whether the current epoch is sealed
    source: JSONL | IPC
  context
    question
    task_summary
    latest_update
    response_started
  control
    owner client
    pending requests
    notifications
```

Selection is stored separately as one hashed conversation ID. A snapshot is rendered from
`threads[selected]`; no reducer event changes another thread's record.

The implementation keeps four kinds of state distinct:

| Layer | Owner | Contents | Boundary |
| --- | --- | --- | --- |
| Source fact | Native adapters | IPC status, JSONL signal, app activity, pending request | Private payload stays native |
| Reduced task state | `RuntimeState.threads` of `ThreadRecord` | Lifecycle, epoch, bounded context, control owner and requests | Background tasks remain isolated |
| Operation state | Command and presentation controllers | Owner refresh, in-flight steering, drag, pet loading, terminal settle | Never becomes lifecycle vocabulary |
| WebView projection | `RuntimeSnapshot` and `ControlSnapshot` | Selected lifecycle, bounded preview, opaque control IDs and capabilities | Derived from the selected task only |

## Lifecycle state census

The complete renderer vocabulary is `idle`, `working`, `reviewing`, `completed`, `failed`,
`interrupted`, and `disconnected`. Only the five middle task states are stored in
`ThreadLifecycle`; `idle` and `disconnected` are presentation projections.

| State | Producer and reduction | Presentation | Controls | Behavioral evidence |
| --- | --- | --- | --- | --- |
| `idle` | Selecting a known task before any lifecycle fact; terminal settle is UI-only | Idle row | Hidden | `select_thread`; terminal presentation tests |
| `working` | New question, JSONL progress, pending request, or IPC active; opens a non-terminal epoch | Working row and strongest status breath | Available only with selected live owner and IPC | `maps_session_states_without_content`; `active_task_exposes_steering_only_with_a_live_owner` |
| `reviewing` | JSONL `entered_review_mode`; `exited_review_mode` returns to `working` | Review row | Hidden | Session mapping test; last observed in the dated 2026-07-19 compatibility snapshot |
| `completed` | JSONL task completion or IPC idle/completed turn | Completion row once, then idle presentation | Hidden | Terminal and IPC mapping tests |
| `failed` | JSONL error or IPC system error/failed turn | Failure row once, then idle presentation | Hidden | Session and IPC mapping tests |
| `interrupted` | JSONL abort or IPC interrupted/cancelled turn | Failure row once, then idle presentation | Hidden | Session and IPC mapping tests |
| `disconnected` | Startup before any selected lifecycle fact; disconnect later removes controls without overwriting a known task | Idle row with disconnected status | Hidden | `ipc_disconnect_does_not_replace_an_active_pet_state` |

Approval and question requests are control facts while lifecycle remains `working`. The former
`needs-input`, `needs-approval`, `needs-attention`, and `error` aliases have no product producer and
are not lifecycle states. Unknown states fall back to idle presentation rather than widening the
contract. Current-version review and private-control behavior still require the dated live gate in
the maintenance procedure before compatibility claims are refreshed.

## Lifecycle reduction

```text
new user question
  -> epoch + 1, working, terminal=false

JSONL progress while non-terminal
  -> working

JSONL progress or display context after terminal
  -> ignored

terminal event
  -> completed/failed/interrupted, terminal=true

IPC active after terminal
  -> epoch + 1, working, terminal=false
```

Source changes alone are metadata changes, not visible state changes. Context changes may still emit
a snapshot while the lifecycle remains `working`, so the current task summary can advance without
restarting the animation.

## Selection arbitration

The App-log parser first accepts a known `ownerRoutePath=/local/<conversation>` from the owner sync
record. The route remains authoritative even if a background turn is still running. An unindexed
owner route is not accepted by UUID shape alone because the initial log tail may contain an older
route. An owner sync with `ownerRoutePath=/` is an explicit route reset: the selector drops both
cached route and activity authority, so the next accepted foreground activity can establish a new
conversation while an immediate steering refresh fails closed during the gap.

When route metadata is unavailable, the parser falls back to activity records retaining
`rendererWindowFocused`, `rendererWindowVisible`, `rendererWindowId`, and `streamRole`. An indexed
fallback conversation is selected when:

```text
active == true
windowFocused != false
windowVisible != false
conversation exists in the Codex thread index
```

An unindexed canonical-UUID conversation is selected only under the stricter conjunction:

```text
active == true
windowFocused == true
windowVisible == true
streamRole == owner
```

Project membership is not runtime state and is never required for lifecycle reduction or WebView
projection. Unindexed non-UUID IDs, missing foreground evidence, follower streams, hidden views, and
historical owner routes remain rejected.

During a normal tab switch Codex emits `old active=false`, may emit an explicit root-route reset, then
emits `new active=true`. Ignoring the weak activity negative avoids an intermediate idle frame; the
root-route reset still removes stale selection authority inside the adapter. Runtime presentation
keeps its last snapshot until a new selection arrives. If Codex is hidden or its private log format
changes, CoPets keeps the last rendered task rather than guessing from task activity.

Background polling and the explicit pre-steering refresh call the same native selection adapter.
They share file cursors, the known-thread index, route priority, and the last confirmed selection;
there is no second synchronous scanner with independent state.

## Control isolation

The UI control snapshot is derived only from `controls[selected]`. User actions are validated again
against the current selection in Rust before sending follower IPC. This prevents a stale card or a
new background approval from routing an action to the wrong conversation.

Capability projection and command dispatch share the same live-turn predicate: selected task,
`working` non-terminal lifecycle, connected IPC, and a non-stale exact owner. Approval and answer
dispatch additionally require the still-pending opaque action ID. Steering and stop never fall back
to another task or start a turn.

## Failure and fallback behavior

- IPC disconnect changes control availability only; it does not erase the selected lifecycle.
- JSONL continues to provide state and bounded display context while IPC reconnects.
- Unknown selection keeps the last confirmed task. The strict projectless UUID exception above is
  owner/foreground evidence, not a most-recent-activity fallback.
- Private activity-log field loss leaves the last native-confirmed selection unchanged; the Node
  diagnostic probe emits no selected candidate when its thread index is unavailable.
- A different existing Codex draft is never overwritten. Empty composer placeholder values are not
  treated as user drafts; keyboard delivery is directed to the confirmed Codex PID so web input
  handlers receive normal key events.

## Extension path

The next protocol layer can expose `activeThreadCount`, a user-invoked conversation switcher and
per-thread attention badges. These features should read the same thread map and must not introduce a
second selection authority. Cross-window ordering can later use a monotonic selection revision if
Codex exposes a stable view-focus sequence.
