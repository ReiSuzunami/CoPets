# ADR 0002: Native per-task follow retention

> Status: Accepted
> Owns: Retained Codex follower registrations across task switches and IPC reconnects
> Update when: Follow retention scope, transport, privacy boundary, or owner-recovery policy changes
> Last verified: 2026-07-25

## Context

CoPets already reduces lifecycle and controls per `ThreadRecord`, but the private Codex follower
registration was only refreshed reactively after a selected follow-up failed. Switching away from a
task or reconnecting CoPets could therefore leave its remembered owner state without an active
follower registration, making a later Ready follow-up needlessly fragile.

The installed Codex App keeps its own per-conversation following registration, follower-client set,
stream role, and revision state. Its follower status request is a registration signal, not permission
for a sidecar to recreate an owner. When an owner is unavailable, the App's own private runtime can
resume it; CoPets has no safe public path to invoke that operation.

## Decision

Keep each observed task's existing native `ThreadRecord` control target as an in-memory follow
record. On every successful CoPets IPC initialization, reannounce `following: true` for every known
control target with an exact conversation and host. When the App asks for following status, answer
only if the remembered native target matches that exact conversation and host.

This retention is transport state, not transcript memory:

1. It is process-memory only. CoPets does not write conversation IDs, owners, hosts, snapshots, or
   message bodies to disk for this purpose.
2. Raw identifiers remain native. The WebView continues to receive only the selected task's opaque,
   bounded projection.
3. Background follow state may update its own `ThreadRecord`, but never changes the visible pet,
   bubbles, controls, or selection authority.
4. Every user action still targets only the revalidated selected task and its exact current owner.
5. CoPets never sends private `thread/resume`, invents an owner, or retries indefinitely. A repeated
   unavailable-owner result is surfaced as an instruction to focus the task in Codex so the App can
   resume it.

## Interface impact

[`ipc.rs`](../../src-tauri/src/observer/ipc.rs) owns retained re-registration and exact follower
status replies. [`runtime.rs`](../../src-tauri/src/observer/runtime.rs) continues to own per-task
native control records, selection, and authorization. [`commands.rs`](../../src-tauri/src/observer/commands.rs)
keeps stale-owner recovery bounded and maps a second unavailable-owner rejection to actionable UI
text.

No WebView interface, pet manifest, persisted setting, or public IPC protocol is added.

## Alternatives

### Retain only the selected task

This preserves less state but drops useful registration whenever the user views another task. It
does not meet the switching behavior this decision addresses.

### Persist task history or raw identities to disk

Persistent history would widen privacy exposure and is unnecessary for follower registration. The
App already owns conversation history and native state needs only the current process lifetime.

### Invoke the App's private resume operation from CoPets

The private resume path requires App-local stream-role context. Calling it from a sidecar would
invent unsupported ownership and could act on the wrong conversation. Rejected.

## Consequences

- Switching back to a known background task reuses its retained native follow state instead of
  relying on a new foreground-only discovery event.
- CoPets resubscribes all known targets after its own IPC reconnect and responds to the App's exact
  status request, while selected-only rendering and action authorization remain unchanged.
- The App may still report an unavailable owner. CoPets can retain and reannounce follow state, but
  only Codex App can resume its owner.
- More known tasks mean more local IPC follow broadcasts immediately after reconnect. They carry
  only the existing conversation and host identifiers and stay on the same-user local socket.

## Verification

1. Unit-test retained selected and background targets, including host omission and exact status
   matching.
2. Unit-test that unknown or wrong-host status requests receive no follow response.
3. Run selected/background task, switch-away/switch-back, IPC reconnect, stale owner, and Ready
   follow-up integration gates with sanitized evidence.
4. Confirm no raw IDs or background task content reaches `RuntimeSnapshot`, `ControlSnapshot`, or
   logs.
5. Run `npm run check:all`, signed macOS build, strict code-sign verification, and the private IPC
   probes.

## References

- [Runtime architecture](../architecture/runtime.md)
- [Multi-session arbitration](../architecture/multi-session-state.md)
- [Ready follow-up research](../research/codex-ready-follow-up-2026-07-25.md)
- [Updating and release](../maintenance/updating.md)
