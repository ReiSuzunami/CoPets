# Changelog

Notable user-visible changes are recorded here. The project follows Semantic Versioning and the release rules in [`docs/maintenance/updating.md`](docs/maintenance/updating.md).

The current public prerelease is `v0.1.0`. No notarized release has been published.

## Unreleased

- Added **Restart Codex with bridge** to the compact experimental Settings disclosure. It requires a
  warning confirmation, then accepts exactly one same-user normal official Codex App, revalidates it
  before a graceful close request, waits boundedly for that App to exit, and reuses the existing
  loopback CDP launcher. It rejects tracked, already-CDP, stale, zero, and multiple App candidates;
  it never force-kills Codex, chooses among multiple Apps, or silently falls back to a normal launch.
- Added **Connect existing** for a Codex App that was already started with a loopback CDP port, so
  it can become an experimental bridge session without restarting Codex. Automatic connection
  accepts exactly one same-user official App candidate; a custom port is required when ambiguous.
  CoPets verifies the App process, IPv4 loopback listener, renderer, and `Rf` fingerprint before
  Ready and before every send; listener loss returns the bridge to standard IPC controls.
- Fixed existing-CDP attachment discovery and readiness: automatic Connect now identifies the
  same-user official App from its command line rather than its mutable macOS display name; Connect
  and Retry retry short renderer/DevTools probes inside their one hard deadline and correctly accept
  a complete DevTools HTTP response without waiting for its persistent socket to close. External
  listener monitoring begins only after the bridge reaches Ready. A transient startup probe can no
  longer revoke the endpoint mid-verify.
- Cold bridge readiness now prefers the main Codex renderer and fingerprints bounded renderer pages
  concurrently under one hard native deadline, avoiding an avatar-overlay probe delaying a ready page.
- Bounded experimental bridge launch and retry verification across listener, HTTP, WebSocket, and
  renderer checks, so a stalled DevTools request resolves to Unavailable instead of leaving
  Settings on Launching indefinitely.
- Added a safe **Retry verification** action after a delayed experimental bridge readiness check.
  It revalidates only the same live tracked Codex PID and private loopback port; it cannot start
  another App, attach to an arbitrary URL, or send a follow-up.
- Made experimental bridge setup a compact, collapsed settings disclosure. Its status remains
  visible without covering the normal pet and package controls; the port picker and launch action
  appear only when explicitly opened.
- Made inline settings the topmost pet-window layer, above current conversation bubbles and controls.
- Added experimental Codex CDP bridge launch with automatic or custom loopback port. Verified Pets
  `Rf` dispatch can send selected Ready follow-up and active Steer without a fresh IPC owner;
  approval and stop remain IPC-only. CoPets tracks a launched or explicitly verified existing App
  PID and requires it to own the loopback listener before readiness and each send. Runtime ports,
  PIDs, targets, prompts, and private IDs are not persisted.
- Hardened experimental CDP bridge readiness against current Codex bundle drift: guarded ESM-export
  discovery now skips a throwing unrelated export, and the no-content `Rf` probe accepts only its two
  fixed, non-sending validation rejections.

### Added

- Ready completed Codex tasks can now accept an explicit follow-up that starts their next turn
  through the selected exact owner. Active-turn steering remains separate and never falls back to a
  new turn.
- Ready follow-up owner recovery now retains the exact host from its successfully written refresh
  when Codex omits `hostId` from that replacement snapshot; it still rejects another task or host.
- Known selected and background Codex tasks now retain native follower registration state across
  task switches and CoPets IPC reconnects, without persisting transcript data or exposing background
  task content.
- Pets now render a small translucent ground shadow behind the loaded sprite.

### Removed

- Withdrawn the experimental cloned-Codex Resume Lab. CoPets now pairs only with the unmodified
  official Codex App and never sends a private resume request or patches a Codex bundle.

## [0.1.0] - 2026-07-25

### Added

- A universal macOS DMG containing an explicit, development-signed installer instead of a bare app.
- Transactional first install and upgrade with payload/staged-copy signature checks, same-directory
  staging and backup, rollback, running-app shutdown, and fail-closed handling for symlinks,
  non-app targets, unrecognized bundle identities, and corrupt payloads.
- A recoverable uninstall action that moves only a verified CoPets app to Trash and preserves
  `${CODEX_HOME:-~/.codex}/pets` plus all Codex-owned data.
- Post-install cleanup that resolves the exact mounted image, copies a bounded helper to the system
  temporary directory, ejects after the installer exits, optionally moves the verified DMG to
  Trash, and removes the helper.
- An installable Sunflower example pet with smooth rubber-hose animation, exaggerated directional running, a frontal working-state petal-length pulse, an official-compatible 1x atlas, and a native Retina 2x atlas.
- An installable Sunflower Gloves example variant with a brown pot, bold stem-mounted rubber-hose gloves, expressive state poses, an official-compatible 1x atlas, and a native Retina 2x atlas.
- In-app first-run, no-pet, disconnected, and incompatible-update guidance.
- Validated local pet import from a package folder, selected `pet.json`, or ZIP, with live preview and native conflict confirmation.
- Staged installation, atomic macOS replacement, confirmed removal, safe active-pet fallback, Finder reveal, and visible diagnostics for invalid manual packages.
- Rejection of installed directory symlinks and ambiguous duplicate ZIP entry names.
- A complete user guide and reproducible M0 clean-profile walkthrough.
- Documentation ownership, runtime architecture, feature catalog, pet package contract, ADR process, and update/release gates.
- Automated documentation metadata, index coverage, and local-link validation.
- Project-level Agent orientation, task routing, architecture invariants, and verification rules.
- `CLAUDE.md` compatibility entry linked to the canonical `AGENTS.md` guide.

### Changed

- A newly foreground-selected Working or Ready task now gets one bounded owner-discovery window
  before follow-up authorization fails. It accepts only that task's fresh IPC owner snapshot and
  never falls back to a background owner; unavailable-owner guidance now states that the selected
  task has no live owner instead of implying a remote-connection cause.
- CoPets now selects the direct foreground view-activity signal ahead of the sidebar owner-route
  hint. Initial, newly discovered, and reset app-log tails are merged by UTC event time behind a
  retained watermark rather than log-file modification time, preventing a historical file written
  later from pinning the pet to a task that Codex has already switched away from.
- Selected Working tasks now keep their Steer arrow visible while the exact Codex owner reconnects;
  sending remains fail-closed until that owner is current and validated.
- Completed tasks now keep their Continue arrow visible while their exact Codex owner reconnects;
  starting the next turn remains fail-closed until that owner is current and validated.
- Retrying a stale selected follow-up now reannounces only its exact conversation/host follower
  registration before dispatch, instead of immediately returning a reconnecting error.
- The application and installer now use the yellow transparent paper-cloud icon across the bundle,
  Finder, README, and DMG.
- Product identity is now CoPets across the app, package metadata, documentation, signed bundle,
  bundle identifier, persisted storage namespace, signing configuration, and internal staging paths.
  The legacy-named pet manifest extension and Codex IPC client type remain unchanged because they
  are external compatibility tokens rather than product branding.
- The macOS menu-bar item now creates an independent settings window at screen center as well as controlling pet visibility and quitting; hidden pets stay hidden and their saved geometry is unchanged.
- Pet-button settings remain inline while menu-bar settings use the separate centered window. The two surfaces are mutually exclusive, and closing the detached window destroys it so it can be recreated reliably while the pet is hidden.
- The full-width Window reset action now centers its icon and label as one balanced control group.
- The settings panel now provides end-to-end pet management without an Agent or manual package editing, and adapts to an empty catalog.
- Package listing now returns valid pets together with folder-level validation issues; an empty catalog clears the renderer instead of retaining a stale pet.
- Pet selection and import preview now share one presentation operation from fetch through decode
  and commit, so cancelled loading cannot restore a removed or superseded pet.
- Standalone IPC diagnostics now use bounded method-specific sanitizers; unknown and free-form
  private-schema values are omitted instead of being trusted by key name.
- Lifecycle presentation now uses one exact vocabulary: pending questions and approvals remain
  `working`, unproduced waiting/error aliases are removed, and all live controls share the selected
  task's connected non-stale owner predicate.
- Per-task lifecycle, display context, controls, and owner refresh now update as one `ThreadRecord`;
  late session progress cannot replace a terminal task's final bubble context.
- Native session and activity-log followers now preserve split UTF-8 and recover from truncation,
  same-size rewrites, and file rotation; blocking scans and SQLite reads run off the async executor.
- Background selection polling and pre-steering foreground refresh now share one confirmed-selection
  adapter instead of scanning the Codex logs through separate state.
- Projectless Codex conversations can now become the selected task without a thread-index row when
  the App supplies explicit active, focused, visible owner-stream evidence for a canonical UUID;
  weaker unknown activity and unindexed historical routes remain rejected.
- Codex owner-route resets now release stale selection authority, so switching from an indexed task
  to a projectless foreground task no longer leaves CoPets pinned to the previous task.
- Stale-owner follow-up retries now stop when selection or lifecycle changes and require an explicit
  written follow refresh plus a matching conversation/host state snapshot before dispatch. A valid
  refreshed snapshot may retain the same owner only after that recovery barrier.
- A second `no client found` follow-up result now marks the target stale and explains that Codex must
  resume its unavailable owner instead of surfacing the private router error unchanged.
- Targeted control responses now fail closed when the private IPC response omits or changes the
  exact owner identity.
- Native IPC now verifies both the same-user Unix socket path and connected peer before sending its
  initialization frame.
- Session, app-log, and thread-index readers now reject symlinks, non-regular files, and foreign
  owners; append reads open with no-follow semantics.
- Path-based IPC and SQLite reads now reject foreign-owned or group/world-writable ancestor
  directories before connecting or querying.
- Steering request construction no longer retains an unused start-turn fallback payload.
- Runtime Reduce Motion changes now reset the pet renderer to a stable first frame immediately.
- Standalone session and app-log probes now emit only allowlisted, hashed source facts; duplicate
  lifecycle/selection policy and fail-open unverified selection were removed.
- Cross-window pet selection failures now show a self-clearing generic error instead of leaving the
  other settings surface silently stale.
- Inline and menu-bar settings now share one panel while running as distinct window roots; the
  detached settings window no longer initializes hidden Pixi, drag, hover, or pet geometry code.
- Cross-window pet selection now loads directly from the current catalog without rescanning;
  catalog mutations reject stale refreshes and force same-ID replacements to reload their textures.
- Window restore, monitor clamping, proportional resize sessions, and pointer-update coalescing now
  share one geometry policy; runtime Reduce Motion uses one live preference owner for Svelte and Pixi.
- README is now a concise project entry point; implementation detail lives in canonical documents.
- Status breathing halo is centered on the control and kept compact instead of flooding the full button.
- Inactive pet windows accept the first mouse press, allowing immediate click-drag without a focus click.
- Drag running now establishes its origin from the native pointer feed and waits for 5 px of movement, filtering initial pointer jitter and cross-source coordinate offsets instead of defaulting immediately to the right-running animation.
