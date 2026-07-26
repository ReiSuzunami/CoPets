# Multi-session state arbitration

> Status: Normative
> Owns: Per-task lifecycle, selection arbitration, control isolation, and fallback invariants
> Update when: Selection authority, epoch reduction, task context ownership, or control routing changes
> Last verified: 2026-07-26

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
2. Background conversation updates never change the visible pet. Their native lifecycle, control,
   and follower-registration state remain retained per task so switching back does not discard a
   known exact conversation/host target.
3. Focused, visible foreground activity is the primary selection authority. A known
   `/local/<conversation>` owner route is only a compatibility fallback while no accepted foreground
   activity exists; an explicit `ownerRoutePath=/` invalidates the cached route and activity. An unindexed canonical-UUID
   conversation additionally requires explicit owner-stream evidence.
4. `active=false` is a weak negative signal. It never clears the visible pet by itself.
5. A terminal state seals its current epoch. Late JSONL progress cannot reopen it.
6. A new user question starts a new epoch. An authoritative IPC `active` snapshot may also reopen a
   terminal conversation when JSONL has not arrived yet.
7. Equivalent states from different sources do not produce duplicate UI events.
8. Approval, answer, stop, steering, and Ready follow-up actions resolve only against the exact
   selected conversation.
   There is no global "last controllable conversation" fallback.
9. Active steering and Ready follow-up revalidate the last confirmed foreground selection
   immediately before dispatch. In IPC transport they target only that conversation's exact follower
   owner. Stale-owner recovery writes an explicit follow refresh before accepting a replacement state
   snapshot; that snapshot must retain the exact selected conversation and host. If the snapshot
   omits `hostId`, the adapter may use only the exact host recorded when that refresh was
   successfully written. In explicit local `CdpReady`, Ready/Steer instead require the same selected
   retained conversation, host, workspace, bridge generation, and tracked official App PID/listener
   proof; they do not use a global owner or an IPC recovery fallback.
10. In `IpcOnly`/`CdpDegraded`, steering dispatch exists only while the selected turn is active and
    its IPC owner is connected. Selected `working` tasks keep an explicit Steer affordance visible,
    and selected terminal `completed` tasks keep an explicit Ready follow-up affordance visible, but
    each may dispatch only through that same fresh owner. In `CdpReady`, those two actions may use
    the verified Pets `Rf` path after the stricter retained-target gates above; approval, answer, and
    stop never switch transports. Failed and interrupted tasks hide follow-up. CoPets never changes
    a failed steer into a start turn, chooses another target, or falls back after a CDP send failure.
    A newly foreground-selected eligible task may wait for its first matching owner snapshot for a
    bounded three-second window, but never borrows a background owner or survives a selection change.
    A same-owner snapshot is recoverable only after that written refresh has armed the exact target;
    it is not evidence that an older queued snapshot is current. The frozen native follow-up guard
    is checked again immediately before queueing and by the IPC writer immediately before its frame.
11. Native follower retention is process-memory only. CoPets reannounces known task registrations
    after its own IPC reconnect and answers a follower-status request only for the exact remembered
    conversation and host. It never persists transcript data or makes a background task visible.
    CoPets never invokes, emulates, patches, or automates private owner resume. After a bounded
    stale-owner retry fails, the selected control stays stale until the user opens that exact task in
    the unmodified Codex App and CoPets observes a fresh exact owner.

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
| Reduced task state | `RuntimeState.threads` of `ThreadRecord` | Lifecycle, epoch, bounded context, control owner/requests, native follow registration | Background tasks remain isolated from presentation |
| Operation state | Command and presentation controllers | Owner refresh, in-flight steering, drag, pet loading, terminal settle | Never becomes lifecycle vocabulary |
| WebView projection | `RuntimeSnapshot` and `ControlSnapshot` | Selected lifecycle, bounded preview, opaque control IDs and capabilities | Derived from the selected task only |

## Lifecycle state census

The complete renderer vocabulary is `idle`, `working`, `reviewing`, `completed`, `failed`,
`interrupted`, and `disconnected`. Only the five middle task states are stored in
`ThreadLifecycle`; `idle` and `disconnected` are presentation projections.

| State | Producer and reduction | Presentation | Controls | Behavioral evidence |
| --- | --- | --- | --- | --- |
| `idle` | Selecting a known task before any lifecycle fact; terminal settle is UI-only | Idle row | Hidden | `select_thread`; terminal presentation tests |
| `working` | New question, JSONL progress, pending request, or IPC active; opens a non-terminal epoch | Working row and strongest status breath | Steer affordance visible; dispatch only with selected live owner and IPC | `maps_session_states_without_content`; `active_task_exposes_steering_only_with_a_live_owner` |
| `reviewing` | JSONL `entered_review_mode`; `exited_review_mode` returns to `working` | Review row | Hidden | Session mapping test; last observed in the dated 2026-07-19 compatibility snapshot |
| `completed` | JSONL task completion or IPC idle/completed turn | Completion row once, then idle presentation | Continue affordance remains visible; default/degraded transport requires selected, connected, fresh exact owner. Verified `CdpReady` may use the exact retained native target; stop and active steering hidden | Terminal, IPC mapping, and selected-Ready control tests |
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

The App-log parser treats an accepted `thread_stream_view_activity_changed` record with focused and
visible foreground evidence as the direct selection signal. `ownerRoutePath=/local/<conversation>`
from the owner sync is a sidebar-router hint, not stream-view truth: it is retained only as a
compatibility fallback when no foreground activity is available and cannot displace a confirmed
foreground conversation. During the adapter's initial tail reconciliation, and whenever a newly
discovered or reset log cursor first exposes its tail, selection lines are merged by their fixed UTC
event timestamp rather than their file modification time. A retained event-time watermark prevents
the later-read historical tail from overriding a newer foreground conversation. Undated initial
evidence remains provisional and is processed before timestamped evidence. An unindexed owner route
is not accepted by UUID shape alone because the initial log tail may contain an older route. An
owner sync with `ownerRoutePath=/` is an explicit route reset: the selector drops both cached route
and activity authority, so the next accepted foreground activity can establish a new conversation
while an immediate steering refresh fails closed during the gap.

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
They share file cursors, the known-thread index, foreground-activity priority, and the last confirmed selection;
there is no second synchronous scanner with independent state.

Each observed control target also stays in its own native `ThreadRecord` while CoPets runs. This is
not a second selection authority or a transcript store: it is the exact conversation/host state
needed to reannounce a follower registration after a task switch, a CoPets IPC reconnect, or an App
follower-status request. A background record may refresh itself, but its changes still cannot replace
the selected pet projection.

## Control isolation

The UI control snapshot is derived only from `controls[selected]`. User actions are validated again
against the current selection in Rust before sending follower IPC. This prevents a stale card or a
new background approval from routing an action to the wrong conversation.

Capability projection and command dispatch share one selected-task authorization model. Approval,
answer, and stop require a `working` non-terminal lifecycle, connected IPC, and a non-stale exact
owner; approval and answer additionally require the still-pending opaque action ID. In default IPC
transport, Steer/Ready use the same owner proof. In `CdpReady`, only Steer/Ready can instead use the
verified `Rf` route, after selected lifecycle plus retained conversation, host, workspace,
transport-generation, and tracked official PID/listener checks. The selected working Steer affordance and
terminal completed Ready follow-up affordance remain visible while the owner reconnects. During IPC recovery, a matching
current snapshot may retain the same owner only after an explicit follow refresh was written for the
selected conversation and host. A second unavailable-owner response leaves the IPC control stale.
An explicit retry must wait for the user to open that exact task in the unmodified Codex App and for
CoPets to observe a fresh selected owner before it sends an IPC prompt. Neither channel falls back to
another task or invents a resume request.

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
