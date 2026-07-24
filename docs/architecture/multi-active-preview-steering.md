# Multi-active task preview and targeted steering

> Status: Proposed
> Owns: Planned multi-active task projection, activity tray, targeted steering interface, migration order, and acceptance gates
> Update when: Product scope, task eligibility, projection fields, steering authorization, delivery phases, or acceptance evidence changes
> Last verified: 2026-07-24

## Decision status

This document is an implementation specification, not a current support claim. Current behavior
remains the selected-task-only contract in
[multi-session arbitration](multi-session-state.md) and the
[feature catalog](../features/catalog.md).

Implementation must begin with a proposed ADR because the feature changes the control interface and
allows bounded background-task context to cross the native/WebView seam. The ADR must replace the
selected-only wording without weakening exact-target, explicit-action, or fail-closed guarantees.

The external behavior assumed by this specification is pinned by the dated
[Codex multi-active evidence snapshot](../research/codex-multi-active-steering-2026-07-21.md).
That snapshot confirms static protocol shape, not live background-steering success.

## Outcome

The target outcome is a bounded activity tray for several local Codex tasks plus steering for an
explicitly chosen live task without first bringing that task to the foreground in Codex App.
CoPets must not advertise the steering half as compatible until the two-task live gate passes on a
supported App build.

The implementation deepens the existing runtime module. It does not add another task store,
selection authority, transcript reader, or harness abstraction:

- `RuntimeState.threads` remains the only reduced task-state store.
- `RuntimeState.selected` remains the only authority that drives the main pet, bubbles, selected
  approvals, and stop control.
- Activity-tray focus is ephemeral presentation state, not task selection.
- Targeted steering resolves one opaque task ID in native memory and revalidates that task's current
  epoch and exact IPC owner.
- Raw conversation IDs, owner IDs, host IDs, request IDs, paths, and private payloads remain native.

## Goals

1. Preview concurrent local tasks without allowing background updates to replace the main pet.
2. Use the official activity priority: needs input, blocked, ready, then running.
3. Send steering to a user-chosen live background task through its exact follower owner.
4. Keep preview payloads bounded, deduplicated, non-persistent, and opaque.
5. Degrade preview and steering independently when JSONL, selection, IPC, or owner data is missing.
6. Preserve current selected-task approval, answer, stop, animation, and terminal-settle behavior.

## Non-goals

- Changing the main pet to follow the highest-priority background task.
- Creating a second task-selection scanner or changing Codex App foreground selection.
- Starting a new turn after terminal state.
- Background approval, answer, stop, retry, pause, resume, or MCP form submission.
- Opening or focusing a Codex chat from the activity tray.
- Subscribing to every streaming conversation at connection time.
- Remote-host, cloud, web, or cross-device session control.
- Persisting previews, unread state, activity history, or steering drafts.
- Introducing official labels as new lifecycle states.
- Freezing a generic Claude Code/Pi/Codex `HarnessAdapter` before a second real adapter passes the
  roadmap conformance suite.

## Current codebase facts

| Area | Current implementation | Required change |
| --- | --- | --- |
| Task state | `RuntimeState.threads` in [`runtime.rs`](../../src-tauri/src/observer/runtime.rs) stores lifecycle, context, control, and owner refresh per opaque task key | Add activity metadata and a bounded multi-task projection; do not add another store |
| Selection | `RuntimeState.selected` and `AppLogSelectionAdapter` select one confirmed foreground task | Keep unchanged as main-pet authority |
| Renderer projection | `RuntimeSnapshot` and `ControlSnapshot` derive only from `threads[selected]` | Add a separate `ActiveTasksSnapshot`; do not widen `pet-state` |
| IPC observation | [`ipc.rs`](../../src-tauri/src/observer/ipc.rs) accepts snapshots for any conversation and stores exact owner, host, conversation, and workspace in native memory | Stamp owner data to the lifecycle epoch and expose only derived capability |
| Steering | `send_follow_up` in [`commands.rs`](../../src-tauri/src/observer/commands.rs) refreshes foreground selection, authorizes selected task, then dispatches to its exact owner | Add explicit-target authorization and a retry path independent of `selected` |
| Concurrency | `follow_up_inflight` is already keyed by opaque task ID | Reuse it: serialize one task, allow different tasks concurrently |
| UI | [`PetWindow.svelte`](../../ui/PetWindow.svelte) owns one runtime snapshot, one control snapshot, and one selected-task draft | Add a tray controller and key a targeted draft by task ID and epoch |

## Product semantics

### Activity classification

Activity is a presentation projection. It does not change `ThreadLifecycle`.

| Activity | Derived condition | Rank | Steering |
| --- | --- | ---: | --- |
| `needsInput` | Current task record has one or more pending control notifications | 0 | Only if the task also passes live-owner authorization |
| `blocked` | Current epoch is `failed` and its activity revision has not been acknowledged | 1 | No |
| `ready` | Current epoch is `completed` or `interrupted` and its activity revision has not been acknowledged | 2 | No |
| `running` | Lifecycle is `working` or `reviewing`, with no higher-ranked activity | 3 | `working` only, subject to exact-owner authorization |

An idle, acknowledged terminal, or unknown task has no tray activity. IPC disconnect disables
steering but does not erase a JSONL-backed preview.
The selected task may continue to drive the main pet even when it has no tray activity.

Pending requests remain lifecycle `working`. `needsInput`, `blocked`, `ready`, and `running` must not
enter the lifecycle census or animation protocol.

### Seen state and ordering

Each `ThreadRecord` gains native-only activity metadata:

```rust
struct ThreadActivity {
    meaningful_revision: u64,
    seen_revision: u64,
}
```

- `RuntimeState` owns one monotonic `activity_revision`. A meaningful lifecycle, bounded-context,
  attention, steering-capability, or acknowledgement change increments it. Meaningful changes also
  copy the value into that task's `meaningful_revision`; acknowledgement updates only
  `seen_revision`. Equivalent source metadata or IPC snapshots do not bump it.
- Owner state separately records the lifecycle epoch in which an authoritative IPC snapshot was
  reduced. Steering requires that owner epoch to equal the current lifecycle epoch, so owner data
  from an earlier turn cannot authorize a new JSONL-created epoch.
- A task becomes seen when its confirmed Codex foreground route is selected or the user explicitly
  opens that activity card. Opening the tray alone does not acknowledge every card.
- New meaningful activity after acknowledgement raises the revision and may surface the task again.

Eligible tasks sort by:

1. activity rank;
2. selected task first within the same rank;
3. descending `meaningful_revision`;
4. opaque task ID as a deterministic final tie-breaker.

The initial renderer projection uses `MAX_ACTIVE_TASK_CARDS: usize = 4` and reports an
`overflowCount`. P1 does not add another retention policy or clock: `RuntimeState` is already
process-local, the projection is bounded, and pruning is orthogonal to proving the tray. If stale
working records become observable in live tests, freshness and retention require a follow-up design
with measured evidence rather than an arbitrary timeout.

### Preview content

Each card contains at most two bounded text fields:

- `headline`: current question, falling back to task summary;
- `detail`: latest assistant update when distinct from the headline.

Both fields use existing whitespace compaction and source-owned length constants. There is no
conversation title source in the current runtime, so the feature must not invent or persist one.
The initial limits therefore remain `QUESTION_LIMIT = 240`, `UPDATE_LIMIT = 180`, and
`SUMMARY_LIMIT = 120`; UI line clamping does not enlarge those native bounds.

## Native interface

Every type, command, event, and field in this section is proposed work and is absent from the current
source unless the text explicitly says it reuses an existing symbol.

### Projection types

The runtime module owns these serialized models:

```rust
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTasksSnapshot {
    pub revision: u64,
    pub enabled: bool,
    pub selected_task_id: Option<String>,
    pub tasks: Vec<ActiveTaskPreview>,
    pub overflow_count: usize,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTaskPreview {
    pub task_id: String,
    pub epoch: u64,
    pub activity_revision: u64,
    pub lifecycle: String,
    pub activity: String,
    pub headline: Option<String>,
    pub detail: Option<String>,
    pub can_steer: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteeringReceipt {
    pub task_id: String,
    pub epoch: u64,
}
```

`taskId` is the existing opaque task-map key. It is not a raw Codex ID. `canSteer` is a rendering
hint, never authorization.

### Commands and event

| Interface | Role |
| --- | --- |
| `get_active_tasks() -> ActiveTasksSnapshot` | Atomic initial gate state plus bounded snapshot |
| `set_multi_active_enabled({ enabled }) -> ActiveTasksSnapshot` | Explicitly change the native gate and return its resulting snapshot |
| `active-tasks` event | Deduplicated updates after reducer effects |
| `acknowledge_task_activity({ taskId, expectedActivityRevision })` | Marks one exact observed activity revision as seen; stale acknowledgements do nothing |
| `send_follow_up_to({ taskId, expectedEpoch, prompt }) -> SteeringReceipt` | Sends to one exact live task after native revalidation |

`pet-state` remains selected-task presentation. `control-state` remains selected-task approval,
answer, stop, and current-task reply capability. Keeping a separate event prevents background
progress from restarting main-pet animation or closing selected-task controls.

`ActiveTasksSnapshot.revision` copies the current global activity revision. The native emitter
compares the semantic payload without that revision and emits only when the visible projection
changes; revision gaps are valid. The WebView ignores an equal or older revision.

### Experimental enablement and kill switch

Multi-active projection and targeted steering are disabled at every process start while the feature
is Experimental. A shared Settings-panel toggle invokes a native runtime gate; the opt-in is not
persisted in this phase.

When disabled, `get_active_tasks` succeeds with `enabled=false`, `selectedTaskId=null`, an empty
`tasks` array, and `overflowCount=0`; it never exposes the cached background projection and does not
return an error. A real gate change runs under the runtime-state lock, increments
`activity_revision`, updates the cached snapshot, then unlocks and emits exactly one `active-tasks`
event; setting the existing value returns the cached snapshot without an event. Both WebViews
must register the event listener before invoking `get_active_tasks` and pass event/initial results
through the same revision reducer, so a window opened during a gate transition cannot resurrect an
older enabled snapshot.

The native disable transition must:

1. reject targeted steering with `featureDisabled`;
2. emit an empty newer `active-tasks` snapshot with `enabled=false` so both WebViews clear cards and
   synchronize the shared toggle;
3. close the tray and clear its one in-memory draft; and
4. leave the existing selected-task observer, pet, and controls unchanged.

A follower request already handed to Codex IPC cannot be recalled. Disable cancels local owner
refresh/retry work and prevents every subsequent targeted dispatch; it does not report an already
dispatched request as cancelled.

Native state owns the gate. A renderer payload can request a change only through the explicit
settings command; acknowledgement and targeted-steer commands re-check the gate under the same lock.
Persisting the opt-in or enabling it by default requires a later ADR update after the P5 privacy and
live gates.

### Owner epoch lifecycle

`ThreadControl` gains native-only `owner_epoch: Option<u64>`:

- an IPC snapshot first reduces any lifecycle state, then stamps the replacement control with
  `Some(record.lifecycle.epoch)`;
- a JSONL new-turn transition or any other epoch increment sets the existing owner epoch to `None`
  before effects are projected;
- IPC disconnect sets every owner epoch to `None` and cancels target-refresh waits, so reconnecting
  cannot briefly reuse a pre-disconnect owner;
- only a later authoritative IPC snapshot can restore `Some(current_epoch)`; and
- authorization compares equality exactly. Missing, older, or newer owner epochs fail closed.

### Targeted authorization

Parameterize the existing `authorize_action` / `ActionKind` seam rather than creating a second set
of live-owner predicates:

```rust
fn authorize_action_for(
    state: &RuntimeState,
    task_id: &str,
    expected_epoch: Option<u64>,
    kind: ActionKind<'_>,
) -> Result<AuthorizedAction, ActivityError>;
```

The existing `authorize_action(state, kind)` remains the selected-task wrapper and preserves every
current command interface. `send_follow_up_to` passes its explicit task and `Some(expectedEpoch)`.
Approval, answer, and stop remain selected-only; the refactor does not expose targeted variants.

Targeted steering authorization requires, under one runtime-state lock:

1. the opaque task exists;
2. `expectedEpoch` equals its current lifecycle epoch;
3. lifecycle is `working` and non-terminal;
4. IPC is connected;
5. control owner exists, is non-stale, and was observed in the same epoch;
6. raw target conversation remains bound to the opaque task record;
7. workspace context needed by `build_follow_up` exists;
8. no steer is already in flight for that task.

There is no selected-task requirement for `send_follow_up_to`, no mutation of `selected`, and no
fallback to another task or owner.

### Dispatch and stale-owner recovery

The command sequence is:

```text
user click
  -> lock: resolve task + epoch + exact target; insert per-task in-flight guard
  -> unlock: build and dispatch thread-follower-steer-turn to exact owner
  -> stale-owner error only:
       lock: mark that same target stale
       broadcast following=true for that target's conversation/host
       wait for a different owner on same task + epoch + conversation + host
       reauthorize same task + epoch
       dispatch once to replacement owner
  -> return bounded receipt or typed error
```

The current retry path's selected-task check must be removed only for the new targeted path. A Codex
foreground switch must neither retarget nor cancel a valid background retry. An epoch change,
terminal transition, IPC disconnect, host mismatch, conversation mismatch, missing host, repeated
owner, or timeout must cancel it.

Only stale-client router errors enter refresh. Other owner errors pass through as bounded delivery
failure. `refresh_following` cannot bootstrap a JSONL-only task without `hostId`; such a task stays
previewable with `canSteer=false` and guidance to open it once in Codex.

The private follower request has no turn ID. `expectedEpoch` closes CoPets-local stale-preview
races but cannot make the check and remote dispatch globally atomic. Live verification must cover a
terminal or new-turn transition during send; unknown behavior fails closed rather than adding a
start-turn fallback.

### Error interface

The new command returns a bounded error code and user-facing message. It never returns raw private
identifiers, paths, router payloads, or native debug text.

| Code | Meaning |
| --- | --- |
| `featureDisabled` | Experimental native gate is off |
| `unknownTask` | Opaque task no longer exists |
| `stalePreview` | Steering card's expected epoch no longer matches the task |
| `notRunning` | Task is reviewing, terminal, idle, or otherwise non-steerable |
| `ownerUnavailable` | No same-epoch owner or host/workspace context exists |
| `ownerReconnecting` | Owner is stale or refresh is in progress |
| `alreadySending` | Same task already has an in-flight steer |
| `invalidPrompt` | Prompt is empty or exceeds the existing follow-up limit |
| `targetChanged` | Refresh produced a mismatched task, epoch, conversation, host, or owner |
| `deliveryFailed` | Exact-owner request failed without a safe refresh path |

Current string-returning commands may remain unchanged. The renderer maps codes to local copy and
uses the bounded native message only as fallback.

## Renderer interaction

### Activity tray

- The existing status orb reveals on hover as today; the tray opens only after explicit click.
- The tray and current controls/settings/follow-up surfaces are mutually exclusive.
- It stays open while pointer or keyboard focus is inside and closes on status-button toggle,
  Escape, settings open, drag, or resize.
- It renders at most four cards plus a compact `+N` overflow indicator.
- Each card shows activity, lifecycle label, bounded headline/detail, selected indicator, and
  steering availability.
- Choosing a card creates an ephemeral `previewTarget` and acknowledges that exact activity
  revision. It does not change `RuntimeState.selected` or the main pet.
- The activity tray does not emit an `aria-live` update for every progress delta. Only new attention
  count/status changes use a polite live announcement.

At small pet sizes, cards collapse to one headline line and omit detail before covering the pet.
Tray height may scroll when keyboard or accessibility text size requires it; scrollbar styling
follows the existing lightweight panel treatment. Reduced motion removes card reordering and
popover transitions.

### Targeted composer

The single targeted composer draft is keyed by `{taskId, epoch}`, not selected
`control-state.canReply`.

- The label identifies the bounded target headline and lifecycle.
- A selected-task `control-state` update cannot close or clear a background draft.
- If the target disappears, changes epoch, becomes terminal, or loses capability, preserve the one
  in-memory draft until the user closes/replaces it, disable send, and show the reason. Never
  silently retarget it.
- Choosing another card replaces an empty draft immediately; a non-empty draft requires explicit
  discard confirmation. There is never a hidden per-task draft map.
- Success closes the composer and shows a transient bounded confirmation for the chosen card.
- Failure keeps the draft and card target available where safe.
- Drafts are never written to local storage or logs.

A small pure controller under `ui/lib` should own snapshot revision rejection, tray target, the one
keyed draft, submit state, and invalidation. `PetWindow.svelte` remains the host for markup and native
event wiring rather than absorbing another independent state machine.

## Privacy and trust changes

This feature intentionally changes one current privacy statement: after explicit current-process
opt-in, bounded background-task preview content will cross the native/WebView seam. The existing
[runtime privacy boundary](runtime.md#privacy-boundary) remains authoritative except for this delta.

Required protections:

- only the four bounded projected cards cross the seam;
- no raw task, conversation, owner, host, request, question, workspace, route, or payload fields;
- no hidden reasoning, tool arguments, command output, full messages, or transcript history;
- no preview, seen state, activity history, or steering draft persistence;
- debug logs may include opaque task ID, lifecycle, activity, list count, and error code only;
- equivalent snapshots are deduplicated before emission;
- all commands revalidate native state and ignore renderer-supplied capability claims.
- a test proves every projected `taskId` is the existing non-reversible opaque hash, never a raw
  Codex conversation or request ID;
- disabling the native gate clears already-projected cards and drafts immediately.

The implementation ADR must update the privacy section in
[runtime architecture](runtime.md), the control invariants in
[multi-session arbitration](multi-session-state.md), and matching guidance in `AGENTS.md` /
`CLAUDE.md`.

## File change map

| File/module | Operation |
| --- | --- |
| [`runtime.rs`](../../src-tauri/src/observer/runtime.rs) | Add minimal activity metadata, bounded active projection, revision/deduplication, native enable gate, same-epoch owner predicate, acknowledgement, and target-parameterized `authorize_action_for` |
| [`commands.rs`](../../src-tauri/src/observer/commands.rs) | Add snapshot/ack/targeted-steer commands; share dispatch; parameterize stale-owner wait by task and epoch |
| [`ipc.rs`](../../src-tauri/src/observer/ipc.rs) | Preserve per-conversation snapshot routing; stamp/refresh exact owner facts; do not add follow-all before its research gate |
| [`mod.rs`](../../src-tauri/src/observer/mod.rs) | Store initial active snapshot and emit deduplicated `active-tasks` effects |
| [`lib.rs`](../../src-tauri/src/lib.rs) | Register new Tauri commands |
| [`control.rs`](../../src-tauri/src/control.rs) | Reuse `ControlTarget` and `build_follow_up`; no new wire method or raw-ID projection |
| [`PetWindow.svelte`](../../ui/PetWindow.svelte) | Subscribe to active snapshots; host tray and one targeted composer; stop clearing its draft from selected `control-state` |
| [`SettingsPanel.svelte`](../../ui/SettingsPanel.svelte) | Add the shared current-process Experimental opt-in/kill-switch control |
| [`SettingsWindow.svelte`](../../ui/SettingsWindow.svelte) | Read/listen/write the native gate for the standalone Settings window |
| `ui/lib/activity-tray.js` | New testable controller for snapshot revision, one target/draft owner, invalidation, and submit state |
| [`style.css`](../../ui/style.css) | Lightweight tray/card/composer layout, small-size behavior, focus, dark mode, reduced motion |
| [`observer/tests.rs`](../../src-tauri/src/observer/tests.rs) | Runtime projection, authorization, race, recovery, privacy, and concurrency tests |
| `test/activity-tray.test.mjs` | New renderer controller contract tests |
| Normative docs and changelog | Update only when implementation changes current behavior |

## Delivery plan

### P0 — Evidence and decision

1. Preserve the dated research snapshot linked above.
2. Create ADR 0002 for multi-task projection, explicit-target control, and the changed privacy seam.
3. Accept the ADR only when implementation begins.
4. Freeze the four-card/native text limits above and the default-off Experimental copy before P1
   tests.

Exit: accepted invariants name one state store, one main-pet selection authority, explicit targeted
steering, bounded background projection, and no fallback owner.

### P1 — Native read-only activity projection

1. Add minimal activity metadata and one monotonic revision sequence.
2. Implement classification, ordering, four-card cap, and acknowledgement without adding retention.
3. Add `ActiveTasksSnapshot`, cached projection comparison, initial command, and event.
4. Add Rust tests before renderer work.

Exit: fixtures can produce several ordered activities while `pet-state` and `control-state` remain
selected-only and equivalent events emit no duplicate activity snapshot.

### P2 — Read-only activity tray

1. Add the pure tray controller and Node tests.
2. Add status-orb tray markup, accessibility, responsive layout, dark mode, and reduced motion.
3. Add acknowledgement wiring. Do not add steering yet.

Exit: the tray previews concurrent tasks, never changes the main pet, and remains usable at minimum
window size and with keyboard/reduced motion.

### P3 — Targeted native steering

1. Add same-epoch owner stamping and target-parameterize the existing `authorize_action` seam.
2. Keep selected approval, answer, and stop command interfaces selected-only.
3. Add `send_follow_up_to` and typed bounded errors.
4. Re-key stale-owner refresh and retry to exact task and epoch.
5. Add same-task serialization, cross-task concurrency, and negative tests.

Exit: mocked IPC proves exact background targeting and rejection of stale epoch, wrong owner/host,
terminal, disconnected, missing-owner, duplicate, and target-change cases.

### P4 — Targeted composer

1. Bind the one composer draft to preview task and epoch.
2. Preserve draft on capability loss; never retarget.
3. Add success/failure feedback and controller/UI contract tests.

Exit: selected-task changes do not close or redirect a background composer, and all sends cross the
new native authorization interface.

### P5 — Current-App integration and release truth

1. Run the private-interface probes and two-live-task scenarios below.
2. Record a new sanitized evidence snapshot if the tested App build or protocol differs.
3. Update runtime architecture, multi-session invariants, `AGENTS.md`/`CLAUDE.md`, feature catalog,
   user guide, roadmap status, and `CHANGELOG.md` in the implementation change.
4. Treat this as a pre-1.0 minor capability.

Exit: mandatory automated, core live, packaged, privacy, and documentation gates agree. Conditional
owner-replacement evidence is recorded without converting an unavailable App behavior into a false
failure.

## Test matrix

### Native reducer and projection

- Background task updates change `active-tasks` but never selected `pet-state` or controls.
- Priority, selected tie-break, revision ordering, cap, and overflow are deterministic.
- Equivalent JSONL/IPC facts do not bump activity or snapshot revisions.
- Late JSONL progress cannot reopen terminal activity.
- Pending requests derive `needsInput` without changing lifecycle.
- Acknowledgement is revision-bound; stale acknowledgement cannot hide newer activity.
- Same-epoch IPC owner enables steering; prior-epoch owner does not.
- Serialized payloads contain no raw IDs, paths, owner data, or unbounded text.
- The Experimental gate defaults off; disabling it clears projection and rejects targeted commands.

### Targeted steering

- A live background task succeeds while another task remains selected.
- Unknown task, stale epoch, reviewing, terminal, disconnected, stale owner, missing host/workspace,
  and empty/oversized prompt fail closed.
- Same-task second send is rejected; different tasks may send concurrently.
- Owner refresh accepts only a different owner for the same task, epoch, conversation, and host.
- Foreground selection changes neither retarget nor cancel valid background recovery.
- Target epoch/lifecycle change during recovery cancels dispatch.
- Targeted steering never emits `thread-follower-start-turn`.

### Renderer

- Older/equal active snapshot revisions are ignored.
- Listener-before-snapshot bootstrap converges both WebViews across enable/disable races.
- Tray open/close, hover/focus retention, Escape, settings, drag, and resize transitions are stable.
- Card order, overflow, selected indicator, acknowledgement, and attention announcements are correct.
- Background composer survives unrelated selected-task `control-state` changes.
- Target invalidation preserves draft, disables send, and displays bounded guidance.
- Success clears only the targeted draft and emits bounded confirmation.
- Minimum-size, dark-mode, keyboard, screen-reader, and reduced-motion behavior passes.

### macOS live scenarios

Use a running supported Codex App and sanitized task labels:

1. Start local tasks A and B concurrently.
2. Keep A selected in Codex; verify both appear and A alone drives the pet.
3. Steer B from CoPets; verify B receives the text, A does not, and Codex App does not activate or
   switch foreground task.
4. Change Codex foreground selection while B steering is pending; verify no retarget.
5. Complete or restart B between composer open and send; verify stale epoch rejection.
6. Exercise an input-needed task and verify priority without changing lifecycle.
7. Disconnect/reconnect IPC and verify previews remain best effort while controls disable/recover.
8. Conditionally exercise owner replacement if the App build exposes a safe reproducible path;
   otherwise require mocked recovery coverage, fail-closed live behavior, and a recorded gap.
9. Inspect WebView events and logs for raw identifiers or unbounded content.

## Verification commands

Focused tests are added during each phase. Final local and macOS gates remain those owned by
[updating and release](../maintenance/updating.md):

```bash
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
npm run probe:ipc
npm run probe:sessions
npm run probe:logs
npm run build:macos:signed -- --bundles app
codesign --verify --deep --strict "src-tauri/target/release/bundle/macos/CoPets.app"
```

## Rollback

The feature adds no persistent schema and begins behind a native default-off gate. If current-App
live gates fail:

- keep selected-task `pet-state`, `control-state`, and `send_follow_up` unchanged;
- turn off the native gate immediately;
- retain read-only previews only if their separate evidence and privacy review pass;
- remove the tray/event as one vertical slice if background projection itself fails privacy or
  compatibility review;
- preserve research snapshots and supersede any accepted ADR rather than rewriting history.

## Acceptance criteria

1. At least two concurrent local tasks can be previewed without changing the main pet selection.
2. After the core live gate passes, the user can steer an explicitly chosen live background task
   without focusing or switching Codex.
3. Every dispatch targets one exact task, epoch, conversation, host, and owner, with no fallback.
4. JSONL-only, terminal, stale, disconnected, unknown, or changed targets remain previewable where
   appropriate but cannot fabricate steering availability or success.
5. Background previews are bounded, non-persistent, deduplicated, and contain no raw private IDs or
   payloads.
6. Existing selected approvals, answers, stop, bubbles, drag/resize, animation, and settings behavior
   remain unchanged.
7. The Experimental feature defaults off and its native kill switch immediately clears renderer
   state and blocks targeted commands.
8. Automated tests, mandatory two-task live evidence, packaged-app smoke tests, normative docs,
   feature status, and changelog agree before release; owner-replacement is conditional and any gap
   is explicit.

## Deferred questions

- Whether a future verified `thread-read-state-changed` adapter should replace CoPets-local seen
  revisions.
- Whether current Codex App exposes a safe initial enumeration/follow operation for every streaming
  conversation.
- Whether observed stale-working records justify adding time-based freshness or thread retention;
  P1 must not guess those policies.
- Whether remote-backed conversations appear on the local IPC router with sufficient host ownership
  evidence.
- Whether official priority should ever drive the main pet. That would require a separate ADR and
  must not reuse ephemeral tray focus as selection authority.
