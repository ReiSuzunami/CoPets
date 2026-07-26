# ADR 0007: User-confirmed restart into the CDP bridge

> Status: Accepted
> Owns: Settings-only graceful restart of one normal official Codex App into CoPets-managed loopback CDP mode
> Update when: Restart target selection, confirmation, termination, launch handoff, or recovery semantics change
> Last verified: 2026-07-26

## Context

CoPets can launch a fresh official Codex App with a loopback CDP port, and it can connect to an App
the user already started with such a port. A normal already-open Codex App has neither path without
the user manually quitting it first. That is safe but adds friction when the user wants the verified
Pets `Rf` Ready/Steer channel for the current local profile.

Closing a desktop App is materially more disruptive than a bridge probe. The App may have active
work or unsaved UI state, and macOS can run multiple official App processes. A restart action must
therefore not infer consent, select a vague process, or force a close.

## Decision

Add **Restart Codex with bridge** only inside the existing Settings experimental-bridge disclosure.
It is shown only in standard `IpcOnly` state (a degraded tracked endpoint exposes retry instead) and
requires a confirmation that explicitly warns that it closes and reopens Codex and can interrupt work
or lose unsaved App UI state. Cancelling performs no native process operation.

After confirmation, native code may identify exactly one process that is all of: same UID, exact
official `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` executable, and absent any
`--remote-debugging-port` argument. It rejects no candidate, more than one candidate, any already
CDP App, and a CoPets-tracked endpoint. It revalidates that exact process immediately before one
`SIGTERM`, then waits a bounded interval for the old official process to exit.

No `SIGKILL`, process-group signal, PID fallback, hidden regular-App relaunch, or automatic retry is
permitted. A refusal, stale target, or timeout directs the user to close Codex manually and starts no
replacement. Only after the old App exits and no other official App remains may CoPets use the
ordinary loopback bridge launcher, including its existing production-profile, exact-PID/listener,
and `Rf` readiness requirements.

This is not an owner-recovery or follow-up mechanism. It must never be reached from Continue, Steer,
an IPC/CDP error, startup, or CoPets quit. Quitting CoPets continues to leave the restarted Codex App
running.

## Interface impact

`SettingsPanel.svelte` projects a restart callback; both `PetWindow.svelte` and
`SettingsWindow.svelte` present the confirmation and submit only automatic/custom-port preferences.
The WebView cannot supply an observed PID, endpoint, target, or raw control field.

`restart_codex_with_cdp` in `observer/commands.rs` shares the existing bridge-operation guard and
managed launch/readiness implementation. `cdp/launch.rs` owns same-user exact-process parsing,
restart eligibility, immediate pre-signal revalidation, the sole graceful signal, and exact-old-PID
exit observation. `RuntimeHandle` continues to own only a tracked bridge endpoint after the new
process is launched; no process data crosses into `ControlSnapshot`.

## Alternatives

- **Keep manual quit only.** Safest baseline, retained as the normal Launch Codex path, but adds an
  avoidable multi-step transition for users who explicitly want bridge mode.
- **Restart automatically when a follow-up owner is unavailable.** Rejected: task controls must not
  change App process state or make a destructive consent decision.
- **Restart the first Codex process found.** Rejected: process display names, multiple windows, and
  CDP provenance make ambiguity unsafe.
- **Force-kill after a timeout.** Rejected: it risks data loss and violates the user-controlled
  graceful-close boundary.
- **Use a generic DevTools endpoint or a patched App.** Rejected by ADRs 0004–0006 and the CDP
  channel trust boundary.

## Consequences

Users gain one direct, explicit route from a normal local Codex App to the bridge channel. The action
has a clear cost: it can interrupt work and has no automatic rollback if the replacement fails.
Native code now owns a narrowly scoped process lifecycle operation and must keep its target parsing,
revalidation, error copy, and bounded wait aligned with the architecture contract.

This ADR extends ADRs 0005 and 0006 without rewriting their historical decisions. Standard Launch
Codex remains non-disruptive, existing-CDP Connect remains restart-free, and the no-patch, no-clone,
and no-private-owner-resume limits remain unchanged.

## Verification

- Rust tests cover restart target eligibility and immediate command-line revalidation, including
  already-CDP and ambiguous candidates.
- Frontend contract tests prove the Settings-only callback, confirmation copy, native command wiring,
  and suppression once the bridge is Ready.
- Product Gate C0r manually verifies confirm/cancel behavior, graceful close, one replacement App,
  `CdpReady`, timeout recovery without force kill, and CoPets quit leaving Codex alive on the pinned
  App build. That live gate remains required before a compatibility claim.

## References

- [CDP follow-up channel](../architecture/cdp-follow-up-channel.md)
- [ADR 0005](0005-cdp-rf-control-channel.md)
- [ADR 0006](0006-explicit-existing-cdp-attach.md)
- [Existing CDP attachment evidence](../research/codex-existing-cdp-attach-live-2026-07-26.md)
