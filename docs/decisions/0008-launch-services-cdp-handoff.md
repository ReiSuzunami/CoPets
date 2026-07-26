# ADR 0008: Launch Services handoff for the CDP bridge

> Status: Accepted
> Owns: Official Codex App launch handoff, native PID rediscovery, and liveness boundary for a CoPets-launched CDP bridge
> Update when: Launch mechanism, process identity proof, liveness handling, or permission-attribution evidence changes
> Last verified: 2026-07-26

## Context

The original Channel B launcher directly spawned the official
`/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` executable and retained its child handle. That
gave CoPets an unnecessary direct-parent relationship to Codex. A user reported that a Codex App
started through that path could present macOS permission requests under the CoPets name.

CoPets must keep the same launch safety properties: explicit user action, no existing-App takeover,
loopback-only CDP arguments, same-user exact-official-App identity, bounded readiness, and
fail-closed control delivery. It must not present an unverified helper PID, a generic local port, or
a changed permission-dialog label as proof that the intended App launched.

## Decision

For **Launch Codex** and the replacement side of **Restart Codex with bridge**, CoPets asks macOS
Launch Services to open the official bundle through `/usr/bin/open -n
/Applications/ChatGPT.app --args ...`. It supplies only
`--remote-debugging-address=127.0.0.1` and the selected
`--remote-debugging-port=<port>` arguments.

The Launch Services handoff process is not a Codex identity and no `Child` handle is retained.
After the handoff returns, native code must rediscover exactly one process that is all of:

- owned by the current macOS user;
- the exact official ChatGPT/Codex executable;
- carrying the exact selected CDP port in its command line.

Only that rediscovered PID becomes the native `CoPetsLaunched` endpoint. Before `CdpReady` and
before every CDP send, it must also own the selected IPv4 loopback listener and pass the existing
renderer/`Rf` readiness checks. Process-command liveness begins once the PID is tracked; listener
liveness begins after Ready. Any ambiguity, mismatch, disappearance, closed listener, or deadline
expiry fails closed and never falls back to a different PID, URL, port, or transport.

The handoff is intended to make the operating-system launch boundary look like a normal app launch.
It does **not** guarantee how every macOS/TCC permission prompt will be attributed or labelled.
That result is version-, permission-, and system-state-dependent and requires a cold-launch A/B
observation before making a product claim.

## Interface impact

`cdp/launch.rs` owns the fixed Launch Services invocation, official-process rediscovery, and
same-user command/listener proof. `observer/commands.rs` owns the bounded rediscovery loop and
the two native monitors. `RuntimeHandle` retains only the rediscovered official PID and port; the
WebView receives neither the handoff helper identity nor runtime endpoint data.

This extends ADR 0005 by replacing its direct-child launch and child-exit assumptions. It extends
ADR 0007 because a user-confirmed restart now reaches the same handoff after the old App exits.
Existing-CDP attachment in ADR 0006 remains a separate user-initiated path.

## Alternatives

- **Keep direct executable spawning.** Rejected: it preserves the direct CoPets parent/launcher
  relationship that prompted this change.
- **Treat `open`'s process as Codex.** Rejected: the system launcher is not the official App PID
  and would weaken the native trust proof.
- **Use a generic DevTools URL after handoff.** Rejected: it would bypass exact App, UID, PID, and
  listener verification.
- **Use `launchd`, `setsid`, or a wrapper helper.** Rejected: those add another process-owner
  relationship without the normal Launch Services app-opening semantics.
- **Promise that permission dialogs will name Codex.** Rejected: CoPets has no authoritative
  contract for TCC attribution and must report observed results instead.

## Consequences

CoPets no longer owns a direct Codex child process, so it cannot use child exit as its liveness
signal. It instead pays a bounded native rediscovery step and continuously revalidates the exact
official command before relying on the tracked endpoint. This preserves the existing fail-closed
CDP boundary while making launch behavior more aligned with macOS app opening.

The change remains Experimental until the product C0a/C0r gates include a cold launch on the pinned
Codex App. The pending dated assessment records the required manual attribution test; it is not
compatibility evidence.

## Verification

- Rust unit tests cover loopback-only handoff arguments and rejection of zero or multiple
  rediscovered exact-port candidates.
- The full source gate covers formatting, Rust tests, frontend build, and documentation validation.
- Product C0a/C0r must cold-launch through Launch Services, prove the rediscovered PID/listener
  boundary, and record any permission-dialog attribution without including task content.

## References

- [CDP follow-up channel](../architecture/cdp-follow-up-channel.md)
- [ADR 0005](0005-cdp-rf-control-channel.md)
- [ADR 0006](0006-explicit-existing-cdp-attach.md)
- [ADR 0007](0007-user-confirmed-cdp-restart.md)
- [Launch Services handoff assessment](../research/launch-services-cdp-handoff-2026-07-26.md)
- [Apple Launch Services documentation](https://developer.apple.com/documentation/coreservices/launch_services)
- [Apple `NSWorkspace.OpenConfiguration` documentation](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration)
