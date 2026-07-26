# Runtime architecture

> Status: Normative
> Owns: Runtime modules, interfaces, seams, data flow, trust boundary, and degradation behavior
> Update when: Observation sources, Tauri commands/events, state ownership, renderer ownership, or privacy flow changes
> Last verified: 2026-07-26

## Scope

CoPets attaches to an already-running Codex App. It does not proxy model traffic or modify the Codex WebView. The native host observes local same-user signals, normalizes them per task, and sends a bounded presentation model to an independent pet window.

The native IPC initialize frame retains the compatibility client type `codex-pet-sidecar`. That
value is owned by the private Codex interface contract and is not the CoPets product identity.

The architecture favors deep modules: each external source is hidden behind an adapter, lifecycle complexity stays behind one reducer interface, and the WebView consumes stable snapshots rather than private Codex payloads.

## System flow

```mermaid
flowchart LR
    subgraph Codex["Already-running Codex App"]
        IPC["User-private IPC router"]
        JSONL["Session JSONL"]
        LOG["Activity log"]
        DB["Thread index"]
    end

    subgraph Native["Tauri native host"]
        OA["Observation adapters"]
        RS["Per-task RuntimeState"]
        CR["Control router"]
        PA["Pet package manager"]
        WM["Window and tray"]
    end

    subgraph Web["Independent WebView"]
        SV["Svelte presentation"]
        PX["Pixi pet renderer"]
    end

    IPC --> OA
    JSONL --> OA
    LOG --> OA
    DB --> OA
    OA --> RS
    RS -->|"pet-state / control-state"| SV
    SV --> PX
    SV -->|"explicit Tauri command"| CR
    CR -->|"selected owner only"| IPC
    PA <-->|"catalog / preview / install / remove"| SV
    PA --> PX
    WM <--> SV
```

## Modules and interfaces

### Observation adapters

Production observation lives under [`src-tauri/src/observer/`](../../src-tauri/src/observer).
[`mod.rs`](../../src-tauri/src/observer/mod.rs) owns the `RuntimeHandle` and starts the IPC,
session, and app-log adapters concurrently.

| Adapter | Input | Interface to reducer | Failure behavior |
| --- | --- | --- | --- |
| IPC follower | Framed messages from the current-user Unix socket | Owner snapshots, authoritative active/terminal state, pending controls | Rejects symlink/non-socket/foreign-owner paths and foreign peers before initialization; controls remain unavailable |
| Session tail | Same-user, non-symlink append-only session JSONL | Turn lifecycle and bounded display context | Existing selected state remains; no backward transcript scan |
| App-log selector | Same-user, non-symlink owner routes and accepted activity records | Selected task candidate | Focused, visible view activity outranks the sidebar-router route hint; every first-read/reset tail is ordered by event timestamp and bounded by a selection-event watermark |
| Thread index | Same-user, non-symlink read-only local index | Ordinary task membership for selection filtering | Strict foreground UUID activity can still confirm a projectless conversation absent from this index |

Session and app-log adapters share [`file_tail.rs`](../../src-tauri/src/file_tail.rs), whose cursor
preserves incomplete UTF-8 bytes and resets on truncation, same-size rewrite, or file identity
change. Local evidence discovery accepts only regular files owned by the effective user. Reads open
with no-follow semantics and revalidate the opened file. Path-based IPC and SQLite consumers also
require a root- or same-user-owned ancestor chain with no group/world-writable directory before the
SQLite index is revalidated and queried read-only. Their directory scans, file reads, and SQLite query execute on blocking workers. One
`AppLogSelectionAdapter` owns known threads, cursors, and confirmed selection for both background
polling and explicit steering refresh. An unindexed conversation is accepted only from an explicitly
active, focused, visible owner-stream activity record with a canonical UUID; weaker or malformed
unknown candidates and unindexed historical owner routes still fail closed. An explicit root owner
route clears cached route and activity authority before the next foreground candidate is evaluated.

The standalone Node adapters in [`src/`](../../src) are diagnostic probes, not the desktop runtime.
[`src/cli.mjs`](../../src/cli.mjs) selects the IPC, session, and log probes and emits sanitized
newline-delimited source evidence for inspection. [`append-follower.mjs`](../../src/append-follower.mjs)
owns their shared cursor, UTF-8 carry, truncation, and rotation mechanics. Session events expose
allowlisted record discriminators and hashed IDs without lifecycle states. App-log events expose
known-thread activity facts without selecting a task; a missing thread index fails closed.

### Runtime state module

[`runtime.rs`](../../src-tauri/src/observer/runtime.rs) owns `RuntimeState`, `RuntimeSnapshot`,
`ThreadLifecycle`, authorization predicates, and `reduce_lifecycle`.
Each task is one `ThreadRecord` containing lifecycle, display context, control owner, pending
requests, owner-refresh state, and native follower-registration memory. Session records, IPC
snapshots, selection, and connectivity enter through `RuntimeEvent`; one reducer transaction updates
the record before deriving WebView effects.

Its interface guarantees:

1. Every hashed task ID owns an independent lifecycle, context, and control record.
2. Selection is stored separately; only `threads[selected]` drives the visible pet.
3. A terminal epoch rejects late JSONL progress.
4. An authoritative IPC active snapshot or a new question may start a new epoch.
5. Equivalent state updates do not restart presentation animation.
6. Renderer payloads contain bounded previews and opaque identifiers, never raw private snapshots.
7. Known task controls retain their exact native conversation/host target across task switches. The
   IPC adapter reannounces those follows after its own reconnect and answers exact App status
   requests, while background records remain outside the WebView projection.

Detailed ordering and selection rules belong to [multi-session arbitration](multi-session-state.md).
Its lifecycle census is the canonical state vocabulary.

### Control router

[`src-tauri/src/control.rs`](../../src-tauri/src/control.rs) converts private pending requests into
compact control models. Command handlers in
[`commands.rs`](../../src-tauri/src/observer/commands.rs) use the authorization contract from
[`runtime.rs`](../../src-tauri/src/observer/runtime.rs) to revalidate selected task, live owner,
lifecycle, and request identity immediately before dispatch.

The interface exposes explicit approval/answer, steering, stop, and user-initiated follow-up
operations. Approval, answer, and stop always share the selected `working` task, connected IPC,
and exact non-stale owner predicate. In default `IpcOnly`/`CdpDegraded` transport, active-turn
steering and Ready follow-up retain that same predicate; Ready additionally requires terminal
`completed`. There is no global fallback owner. In an explicit CoPets-launched or user-attached
local `CdpReady` session, only Ready and active Steer may instead call the verified in-window Pets
`Rf` handler using the
same selected native conversation/host/workspace target, without a fresh follower owner. See the
[CDP channel contract](cdp-follow-up-channel.md). A failed steer never changes into a new turn,
and Channel B never falls back to IPC or starts Codex from a send action.

The control projection keeps a selected `working` task's Steer affordance and terminal `completed`
task's Continue affordance visible while their owner reconnects. In IPC transport these are
presentation-only signals; sending still requires the connected, exact, non-stale owner predicate.
In `CdpReady`, the same visible control can authorize only the selected retained native target and
only through the `Rf` path; no target, host, workspace, selection, transport-generation, or
tracked official-PID/listener mismatch is recoverable by fallback.

If an explicit foreground refresh has selected an otherwise eligible task before its first IPC
owner snapshot arrives, the router waits for that exact selected task for at most three seconds.
It accepts only its matching fresh owner snapshot; a selection or lifecycle change, a background
snapshot, or an elapsed window fails closed and sends no request.

When a follower reports its owner as stale, the IPC control router marks that exact target pending,
writes an explicit follow refresh to the IPC stream, then arms recovery only after the local write
succeeds. Recovery accepts only a subsequent state snapshot for the same selected conversation and
host. The App may legitimately reissue that snapshot with the same owner identity, so same-owner
acceptance is limited to this armed recovery phase; pre-existing or unrelated snapshots remain
rejected. A later explicit user retry of that same stale selected target reissues this exact refresh
before it considers any follower request; it never reuses the stale owner directly. Selection and
lifecycle are revalidated again before dispatch. IPC follow-up dispatch carries that frozen native
guard to the writer, which rechecks selection, lifecycle, exact owner, IPC connectivity, transport,
and generation immediately before emitting its frame.

Follower retention is native process memory rather than a transcript cache. The IPC adapter keeps
every known task's exact conversation/host registration through background switches, reannounces it
after CoPets reconnects, and replies only to an exact matching follower-status request. It does not
persist raw IDs or content, expose background state to the WebView, or create an owner. CoPets never
calls, emulates, patches, or automates the App's private resume operation. When the selected owner
remains unavailable after its bounded exact follow refresh, the control stays stale and tells the user
to open that exact task in the unmodified Codex App before retrying. This stale-owner recovery does
not apply to an already verified `CdpReady` send; Channel B revalidates the selected retained target,
its bridge generation, and the tracked official App PID's ownership of the loopback listener
instead. CoPets still requires a fresh selected owner and an immediate
final selection check before it sends an IPC follow-up.

After a written exact `following:true` refresh, a state snapshot that omits `hostId` may use only
that just-written conversation/host registration. It cannot borrow a host from another task, bypass
the armed-refresh phase, or weaken source-owner validation.

### Pet package manager

[`src-tauri/src/pet.rs`](../../src-tauri/src/pet.rs) owns `PetPackageManager` and the `list_pets`, `load_pet`, `preview_pet_import`, `install_pet`, `remove_pet`, and `open_pets_folder` commands. It confines asset paths to one package directory and validates manifest identity, media type, byte size, sprite version, grid geometry, and render scale before returning a data URL plus cell metadata.

Folder, manifest-file, and ZIP inputs share one prepare/validate interface. Preview performs full validation without activation. Installation copies into same-filesystem staging, validates the copy, and activates it with rename or an atomic macOS directory swap. Installed directory symlinks and ambiguous duplicate archive names are rejected. Removal first moves the package out of discovery and rolls back that move if deletion fails. The WebView decides only when an explicit user action should invoke these operations; native code owns filesystem mutation and conflict enforcement.

The package contract is documented in [Pet packages](../protocol/pet-package.md).

### Native shell

[`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs) builds the Tauri app, registers commands, owns the menu-bar status item, starts observation, performs native dragging, and emits focus-independent pointer hover changes. The status item toggles pet visibility, creates an independent settings window at screen center on demand, and quits the app. Opening settings focuses only that window; a hidden pet remains hidden and its saved geometry is untouched. Closing the detached window destroys it so the next menu-bar request starts from a fresh visible window. The inline and detached settings surfaces are mutually exclusive: the menu-bar entry dismisses inline settings, while the pet entry destroys any detached window before opening locally. [`src-tauri/tauri.conf.json`](../../src-tauri/tauri.conf.json) owns the persistent pet-window definition and bundle configuration, including first-mouse acceptance so an inactive pet can begin dragging on the first press; `show_settings_window` owns detached-window creation and geometry.

The CDP launcher is native-only. Standard **Launch Codex** never closes an existing App. A separate
Settings-confirmed restart can select exactly one same-user official non-CDP App, revalidate that
exact process immediately before one graceful `SIGTERM`, wait for it to exit, then reuse the ordinary
loopback bridge launcher. It rejects zero, multiple, stale, or already-CDP processes, never force
kills, never accepts a PID from the WebView, and never runs as a follow-up control fallback. See the
[CDP channel contract](cdp-follow-up-channel.md) and [ADR 0007](../decisions/0007-user-confirmed-cdp-restart.md).

Window position and size are stored by [`ui/PetWindow.svelte`](../../ui/PetWindow.svelte).
[`ui/lib/window-resize.js`](../../ui/lib/window-resize.js) owns monitor normalization and
selection, restore/reset clamping, minimum dimensions, proportional corner geometry, resize-session
identity, and pointer-update coalescing.

### Presentation and renderer

[`ui/App.svelte`](../../ui/App.svelte) routes each native window label to a distinct root.
[`ui/PetWindow.svelte`](../../ui/PetWindow.svelte) subscribes to `pet-state` and `control-state` and
owns Pixi, bubbles, controls, drag/resize interactions, and the single live reduced-motion query.
[`ui/SettingsWindow.svelte`](../../ui/SettingsWindow.svelte) initializes no Pixi, drag, hover, or
pet-window geometry. Inline and detached hosts both render
[`ui/SettingsPanel.svelte`](../../ui/SettingsPanel.svelte), while their roots retain different
open/close and event-routing behavior.

[`ui/lib/pet-catalog-controller.js`](../../ui/lib/pet-catalog-controller.js) owns catalog snapshots,
selection commits, render invalidation, stale-refresh rejection, and same-ID reload after catalog
mutation. Selection-only events load from the receiver's current catalog without rescanning or
re-persisting; catalog-mutation events request an explicit rescan. Deterministic fallback selection
after a real catalog change remains in [`ui/lib/pet-catalog.js`](../../ui/lib/pet-catalog.js).
Cross-window selection delivery failures produce a bounded transient message; native error text,
paths, and event payloads are not reflected into the WebView.

[`ui/lib/pixi-pet.js`](../../ui/lib/pixi-pet.js) is the Pixi adapter.
[`ui/lib/pet-presentation.js`](../../ui/lib/pet-presentation.js) owns each selected-pet or preview
operation from fetch through decode and commit. One operation generation invalidates cancelled or
superseded work while the Pixi adapter retains cleanup checks after asynchronous decode.
[`ui/lib/pet.js`](../../ui/lib/pet.js) maps normalized states to atlas rows and terminal behavior.
[`ui/lib/motion-preference.js`](../../ui/lib/motion-preference.js) owns the media-query listener and
injects its current value into the Pixi adapter; live changes update both Svelte transitions and
Pixi frame advancement.
Bubble formatting and visible-prefix trimming are isolated in
[`ui/lib/markdown.js`](../../ui/lib/markdown.js),
[`ui/lib/conversation-display.js`](../../ui/lib/conversation-display.js), and
[`ui/lib/bubble-overflow.js`](../../ui/lib/bubble-overflow.js).

## Native/WebView seam

Native-to-WebView events:

| Event | Payload role |
| --- | --- |
| `pet-state` | Selected task lifecycle and bounded conversation context |
| `control-state` | Selected task control availability and compact pending requests |
| `pet-window-hover` | Pointer-inside-window state without focus transfer |
| `refresh-settings` | Targeted native-to-settings catalog refresh after the independent window is created or revealed |
| `close-inline-settings` | Menu-bar-to-pet request to dismiss the inline settings surface before revealing the detached panel |
| `pet-catalog-changed` | Settings-to-pet catalog-mutation/rescan request carrying a preferred package ID and same-ID reload hint |
| `pet-selection-changed` | Bidirectional exact selection update carrying only the selected package ID; never implies a catalog rescan |
| `reset-pet-window` | Settings-to-pet request to run the pet window's existing reset path |

WebView-to-native commands include snapshot reads, explicit control dispatch, stop, steering, pet catalog/load/preview/install/remove/open-folder operations, pointer polling, detached-settings closure, and window operations. Native file and confirmation dialogs are provided through scoped Tauri plugins. [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs), [`capabilities/default.json`](../../src-tauri/capabilities/default.json), and [`capabilities/settings.json`](../../src-tauri/capabilities/settings.json) are the registration and permission sources of truth.

## Privacy boundary

- Raw task/request IDs, owner routes, and exact private IPC payloads stay in native memory.
- Same-UID local processes are inside this trust boundary; CoPets does not isolate Codex from a
  malicious process already running with the current macOS user's privileges.
- IPC initialization requires both a same-user socket path and same-user connected peer.
- Path-based IPC and SQLite access rejects writable or foreign-owned ancestor directories.
- Session, app-log, and thread-index inputs reject symlinks, non-regular files, and foreign owners.
- The renderer receives only opaque action IDs and bounded selected-task previews.
- Hidden reasoning, tool arguments, command output, full answer bodies, background-task content, and unrelated snapshot history do not cross the seam.
- Conversation previews are not persisted by CoPets.
- Import source paths exist only for the active native-dialog/preview operation and are not persisted.
- Incoming control requests to the diagnostic Node IPC observer are rejected.
- Diagnostic IPC output uses method-specific schemas. It emits bounded enums, booleans, integers,
  and hashes of explicitly known identifiers; unknown fields and free-form strings are omitted.
- Diagnostic session and app-log output contains only allowlisted source facts and hashed
  identifiers. Node code does not emit lifecycle or selected-task classifications.

Exact limits belong to constants and tests in the observer/control implementations. Security and legal analysis remains a dated [research snapshot](../research/security-and-legal-boundary.md), not a permanent guarantee.

## Degradation model

| Failure | Expected behavior |
| --- | --- |
| IPC unavailable, untrusted, or owner stale | Withhold actionable controls; retain the selected working/Ready follow-up affordance as a recovery entry and continue best-effort lifecycle from trusted JSONL |
| Activity schema unavailable | Keep last confirmed selected task; reject unknown candidates |
| Thread index missing a projectless conversation | Accept only canonical-UUID active/focused/visible owner-stream evidence; otherwise keep the last selection |
| Session tail unavailable | Keep current state; do not infer completion |
| Imported pet invalid | Keep the installed package unchanged and show the validation error |
| Manually placed pet invalid | Exclude it from selection and list its folder-level diagnostic in settings |
| Saved monitor missing | Clamp restored window to an attached display |
| Reduced motion enabled | Hold a stable state frame and avoid looping animation |

Unknown private protocol data is ignored or surfaced as unavailable. It must never be converted into a fabricated success state.

## Test surfaces

| Module interface | Primary verification |
| --- | --- |
| Node observation adapters and append following | [`test/observer.test.mjs`](../../test/observer.test.mjs), [`test/append-follower.test.mjs`](../../test/append-follower.test.mjs) |
| Lifecycle, selection, control routing | Adapter-local Rust tests under [`observer/`](../../src-tauri/src/observer), cross-module contracts in [`observer/tests.rs`](../../src-tauri/src/observer/tests.rs), and [`control.rs`](../../src-tauri/src/control.rs) |
| Pet package validation, mutation, catalog ordering, and presentation cancellation | Inline Rust tests in [`pet.rs`](../../src-tauri/src/pet.rs), [`test/pet-presentation.test.mjs`](../../test/pet-presentation.test.mjs), [`test/pet-catalog-controller.test.mjs`](../../test/pet-catalog-controller.test.mjs), and [`test/pet-catalog.test.mjs`](../../test/pet-catalog.test.mjs) |
| Animation/presentation | [`test/pet-animation.test.mjs`](../../test/pet-animation.test.mjs) and [`test/conversation-display.test.mjs`](../../test/conversation-display.test.mjs) |
| Window interaction and motion preference | Drag, pointer, resize, motion-preference, and interaction regression tests under [`test/`](../../test) |

Test commands and change-specific gates belong to [Updating and release](../maintenance/updating.md).
