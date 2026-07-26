# Feature catalog

> Status: Normative
> Owns: Current user-visible features, implementation map, coverage, and known limitations
> Update when: User-visible behavior, support status, or a feature's implementation/test owner changes
> Last verified: 2026-07-26

## Status vocabulary

- **Implemented:** available in the current desktop build and covered by automated tests.
- **Experimental:** implemented against a private or unstable external interface; reverify after Codex updates.
- **Limited:** deliberately narrower than the apparent full feature.
- **Planned:** documented direction with no current user-facing implementation.

## Codex observation and lifecycle

| Feature | Status | Implementation | Verification / limit |
| --- | --- | --- | --- |
| Attach after Codex starts | Experimental | IPC, session, and selection adapters under [`observer/`](../../src-tauri/src/observer) | Private, unversioned local interfaces; socket path and connected peer must match the effective user |
| Selected-task tracking | Experimental | `RuntimeState` in [`runtime.rs`](../../src-tauri/src/observer/runtime.rs) and `AppLogSelectionAdapter` in [`selection.rs`](../../src-tauri/src/observer/selection.rs) | Focused/visible view activity is authoritative; sidebar owner routes are fallback hints. First-read/reset log tails merge by event time behind a watermark, root-route resets release stale authority, and projectless UUID conversations still require active/focused/visible owner-stream evidence |
| Independent background task state | Implemented | Per-task reducer in [`runtime.rs`](../../src-tauri/src/observer/runtime.rs) | [`multi-session-state.md`](../architecture/multi-session-state.md); one visible pet only |
| Work/review/terminal states | Implemented | `RuntimeEvent`, `ThreadRecord`, and localized source adapters under [`observer/`](../../src-tauri/src/observer) | Exact vocabulary is owned by the [lifecycle census](../architecture/multi-session-state.md); pending controls remain `working`; late JSONL progress cannot reopen a terminal epoch or replace its final display context |
| IPC-disconnect fallback | Limited | JSONL lifecycle continues without control owner through the native incremental file cursor | Evidence files must be same-user, regular, and non-symlink; split UTF-8, truncation, same-size rewrite, and file rotation are covered; missing data never invents completion or selection |
| Standalone observation probe | Implemented | [`src/cli.mjs`](../../src/cli.mjs), three Node adapters, and [`append-follower.mjs`](../../src/append-follower.mjs) | Diagnostic NDJSON source evidence only; session/log probes emit no lifecycle or selection policy, reject unknown threads when the index is unavailable, and redact free-form values |

## Pet and window

| Feature | Status | Implementation | Verification / limit |
| --- | --- | --- | --- |
| Codex V1/V2 packages | Implemented | [`pet.rs`](../../src-tauri/src/pet.rs), [`pixi-pet.js`](../../ui/lib/pixi-pet.js) | Only supported geometry/media passes validation |
| Local pet import and preview | Implemented | `PetPackageManager` in [`pet.rs`](../../src-tauri/src/pet.rs), [`pet-presentation.js`](../../ui/lib/pet-presentation.js), and shared [`SettingsPanel.svelte`](../../ui/SettingsPanel.svelte) | Folder, selected `pet.json`, or ZIP with at most one wrapper; cancelled fetch/decode work cannot restore a removed or superseded pet; a lone spritesheet is not importable |
| Safe replace and removal | Implemented | Staged mutation in [`pet.rs`](../../src-tauri/src/pet.rs), catalog workflow in [`pet-catalog-controller.js`](../../ui/lib/pet-catalog-controller.js) | Replacement is atomic on macOS and forces same-ID texture reload; stale catalog responses cannot overwrite newer state; failed validation keeps the installed package; removal requires confirmation |
| Native Retina atlas | Implemented | Legacy-compatible `sidecarSpritesheetPath` and [`build_native_atlas.py`](../../scripts/build_native_atlas.py) | CoPets supports 2x-4x; source strips cannot be enlarged; selected states can preserve one shared frame transform while their silhouette changes |
| Pet ground shadow | Implemented | [`pixi-pet.js`](../../ui/lib/pixi-pet.js) | A translucent Pixi ellipse is behind the loaded sprite and follows its logical scale and layout; the transparent native pet window still has no system shadow |
| Installable example pet | Implemented | [`examples/pets/sunflower`](../../examples/pets/sunflower) | Ships a V1-compatible 1x fallback plus a source-derived native 2x atlas with no source upscaling |
| Pet Preview Studio | Planned | [`pet-preview-studio.md`](../architecture/pet-preview-studio.md) | Separate development-only window for package/Hatch inspection; no install, selection, source editing, or Codex runtime access |
| State animation | Implemented | [`pet.js`](../../ui/lib/pet.js), [`pixi-pet.js`](../../ui/lib/pixi-pet.js), [`style.css`](../../ui/style.css) | Directional run; terminal animations play once; active status halo stays compact and centered |
| Terminal settle to idle | Implemented | [`conversation-display.js`](../../ui/lib/conversation-display.js) | Observer retains factual terminal state |
| Drag with live run direction | Implemented | [`tauri.conf.json`](../../src-tauri/tauri.conf.json), [`drag-motion.js`](../../ui/lib/drag-motion.js), [`drag-pointer.js`](../../ui/lib/drag-pointer.js) | Inactive window accepts the first press; the first native pointer sample establishes origin and a 5 px threshold filters press jitter before running; horizontal movement selects left/right, and pauses stay running until release |
| Proportional resize grip | Implemented | [`window-resize.js`](../../ui/lib/window-resize.js), [`PetWindow.svelte`](../../ui/PetWindow.svelte) | Manual grip because native corner resize is unavailable on macOS; the geometry module serializes and coalesces pointer updates |
| Position and size restore | Implemented | [`window-resize.js`](../../ui/lib/window-resize.js), [`PetWindow.svelte`](../../ui/PetWindow.svelte) | Best effort; normalizes physical monitor geometry and clamps to an attached display |
| Hover controls while unfocused | Implemented | Native hover monitor in [`lib.rs`](../../src-tauri/src/lib.rs) | Does not activate CoPets or steal focus |
| Menu-bar controls | Implemented | `setup_tray` and `show_settings_window` in [`lib.rs`](../../src-tauri/src/lib.rs), [`SettingsWindow.svelte`](../../ui/SettingsWindow.svelte) | Show/hide, on-demand screen-centered settings, and quit; detached settings initializes no Pixi, drag, or hover runtime and remains available while the pet is hidden |
| Self-cleaning macOS distribution | Implemented | [`Installer.swift`](../../installer/macos/Installer.swift), [`package_macos_dmg.sh`](../../scripts/package_macos_dmg.sh) | Universal private DMG validates and transactionally installs/upgrades or moves a verified app to Trash; eject helper self-removes; shared pet and Codex data are always preserved; development-signed and not notarized |
| Dark mode and reduced motion | Implemented | [`style.css`](../../ui/style.css), [`motion-preference.js`](../../ui/lib/motion-preference.js), [`pixi-pet.js`](../../ui/lib/pixi-pet.js) | One live preference owner updates Svelte presentation and Pixi immediately; reduced motion holds a stable frame |

## Conversation and controls

| Feature | Status | Implementation | Verification / limit |
| --- | --- | --- | --- |
| Selected-task bubbles | Limited | [`conversation-display.js`](../../ui/lib/conversation-display.js) | At most two; current question is best effort |
| Safe GFM Markdown | Implemented | [`markdown.js`](../../ui/lib/markdown.js) | Unsafe links/raw HTML are not rendered as active content |
| Streaming progress reveal | Implemented | [`PetWindow.svelte`](../../ui/PetWindow.svelte) | Streams the bounded in-memory preview, not raw model tokens |
| Visible-prefix ellipsis | Implemented | [`bubble-overflow.js`](../../ui/lib/bubble-overflow.js) | No internal scrollbar; recalculates after resize |
| Approval and answer cards | Experimental | [`control.rs`](../../src-tauri/src/control.rs), [`PetWindow.svelte`](../../ui/PetWindow.svelte) | Exact selected task and live request revalidated before send |
| Experimental CDP bridge launch / restart / connect | Experimental | [`cdp/`](../../src-tauri/src/cdp), `launch_codex_with_cdp`, `restart_codex_with_cdp`, `connect_existing_codex_cdp`, `retry_cdp_bridge`, and shared [`SettingsPanel.svelte`](../../ui/SettingsPanel.svelte) | Standard Launch asks macOS Launch Services to open the unmodified official App after the user quits existing instances, then CoPets rediscovers exactly one same-user official PID carrying the selected loopback port; the system handoff helper is never accepted as Codex. Users may instead explicitly restart exactly one normal App after a warning confirmation, or connect an already loopback-CDP-enabled App without restarting it. Restart rejects zero, multiple, stale, tracked, or already-CDP App candidates; it revalidates the exact same-user official PID before one graceful `SIGTERM`, waits boundedly, and never force-kills or falls back to another App. Automatic Connect accepts one same-user official App candidate; custom Connect validates only the selected loopback port. CoPets tracks the exact PID/command/listener, rejects helper-only or arbitrary endpoints, and rechecks before every send. Command liveness starts when a PID is tracked; listener liveness starts after Ready. One native hard deadline bounds launch rediscovery and listener/DevTools readiness; existing-endpoint Connect/Retry use short bounded attempts within it and accept bounded HTTP `Content-Length` replies without waiting for a socket close. Main-window-preferred concurrent probes avoid cold overlay head-of-line blocking. Retry rechecks only the same native endpoint; runtime port/PID/target/payload stay native-memory only. Launch Services does not promise a macOS permission-dialog label; that requires product-path observation |
| Active-turn steering | Experimental | `send_follow_up` in [`commands.rs`](../../src-tauri/src/observer/commands.rs) | Default/degraded mode retains exact-owner IPC recovery. In a verified launched or explicitly attached local `CdpReady` session, a selected `working` task can dispatch `steer-turn-for-host` through Pets `Rf` with exact retained conversation/host/workspace, bridge-generation, and tracked-listener checks; it never starts a new turn or falls back after CDP failure |
| Ready-task follow-up | Experimental | `ControlSnapshot`, `PetWindow.svelte`, `send_follow_up`, `authorize_follow_up`, [`cdp/`](../../src-tauri/src/cdp), and `build_cdp_ready_params` | Default/degraded mode requires a fresh exact IPC owner and bounded selected-task recovery. In a verified `CdpReady` session, selected `completed` Continue uses `send-follow-up-message` through Pets `Rf` without requiring a fresh owner, but still requires the exact native conversation/host/workspace target and managed-listener proof. Reverify product-path C0/C2/C2b after every Codex App update |
| Retained task follows | Experimental | [`ipc.rs`](../../src-tauri/src/observer/ipc.rs) | Native-memory conversation/host registration is retained for selected and background tasks, reannounced after CoPets reconnects, and never projects background content to the pet or persists transcript data. An unavailable owner still must be resumed by Codex App |
| Stop active turn | Experimental | `stop_current_task` in [`commands.rs`](../../src-tauri/src/observer/commands.rs) | Targets selected task's live owner only |
| Self-clearing transient errors | Implemented | [`transient-message.js`](../../ui/lib/transient-message.js) | Prevents stale errors covering normal controls |
| First-run and recovery guidance | Implemented | [`SettingsPanel.svelte`](../../ui/SettingsPanel.svelte) | Optional first-run card plus actionable no-pet, disconnected, and invalid-package states |
| Settings surface | Implemented | Shared [`SettingsPanel.svelte`](../../ui/SettingsPanel.svelte), distinct [`PetWindow.svelte`](../../ui/PetWindow.svelte) and [`SettingsWindow.svelte`](../../ui/SettingsWindow.svelte) hosts, native lifecycle in [`lib.rs`](../../src-tauri/src/lib.rs) | The pet button opens settings inline at the pet as the topmost pet-window layer; the menu bar creates a separate centered window. Experimental bridge setup is a collapsed disclosure that keeps its compact status visible until the user chooses to configure it. It offers Launch Codex, Connect existing, and—while standard IPC is active—Restart Codex with bridge. Restart shows a warning confirmation before the native command can gracefully close one exact normal App; a degraded tracked endpoint instead exposes retry without exposing its endpoint. It stores only automatic/custom preference and optional custom port; runtime CDP state remains native-memory only. Pure selection sync does not rescan the catalog; package mutation does |

## Multi-session behavior

CoPets caches multiple tasks but presents only the task selected in Codex. Background events cannot replace the visible animation, bubble, or controls. There is no conversation switcher or background attention badge in the current build. Those are planned extensions and must reuse the existing per-task map rather than create a second selection authority.

## Deliberate limits

- macOS desktop only.
- No transcript viewer, full prompt/answer capture, hidden reasoning, tool argument, or command-output display.
- No arbitrary WebView/DevTools attachment or preload-bridge message dispatch. The sole experimental
  CDP paths are a CoPets-managed loopback launch and an explicit, verified same-user official local
  CDP connection; both call the verified in-renderer Pets `Rf` handler.
- No modification, clone, patch, or private owner-resume automation of the official Codex App.
- No Accessibility-based message delivery.
- No automatic new-turn creation from a failed steer; only an explicit follow-up on a selected
  terminal `completed` task can start its next turn.
- No promise of version-independent Codex App compatibility.
- Official Pet Creator export remains fixed-size; high-resolution sheets are a CoPets extension using the legacy `sidecarSpritesheetPath` key.

## Feature update rule

Every user-visible change must update one row here in the same change. Record new behavior, status, implementation owner, verification, and any reduced scope. Detailed algorithms stay in architecture/protocol documents or source tests; this catalog stays scannable.

Forward-looking work belongs in the [roadmap](../roadmap.md); roadmap entries are not current feature support.
