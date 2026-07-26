# ADR 0006: Explicit attachment to an existing local Codex CDP endpoint

> Status: Accepted
> Owns: User-initiated attachment to an already CDP-enabled official Codex App and its provenance checks
> Update when: Existing-endpoint discovery, process validation, lifecycle monitoring, or Channel B eligibility changes
> Last verified: 2026-07-26

## Context

ADR 0005 introduced the experimental `Rf` control channel only for an App process launched by
CoPets. That protected the initial rollout from arbitrary DevTools attachment, but made a user
restart Codex even when they had already launched the official App with a loopback debugging port.

The existing App can expose more than one process with an inherited listener file descriptor. The
port is also an unauthenticated same-user debugging surface. A generic local Chromium endpoint, a
remote listener, or a helper process must therefore never become a Channel B target merely because
it answers CDP.

## Decision

Keep ADR 0005's CoPets-launched path and add a second **explicit user-initiated** path:

- **Connect existing** either discovers exactly one eligible local Codex CDP process after the user
  clicks it, or validates the user-entered custom port. Automatic discovery refuses zero or multiple
  candidates; it derives candidates from native official-App command lines rather than a mutable
  process display name, and the user must select a custom port in the latter case.
- An eligible endpoint is an exact same-user
  `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` process whose command includes that CDP port,
  owns an IPv4 `127.0.0.1` listener for it, exposes Codex renderer pages, and passes the guarded
  `Rf` source plus fixed no-content sentinel probe.
- Native memory records endpoint provenance (`CoPetsLaunched` or `UserAttached`), PID, port, and
  transport generation. The WebView receives only the existing transport label; it never receives a
  discovered runtime port, PID, page target, or raw control data.
- The external path rechecks the same PID/listener while it is tracked and before every Channel B
  send. Listener loss clears `CdpReady`; any failed discovery, ownership check, page check, or `Rf`
  check remains fail-closed in IPC mode.

The feature remains experimental and is only available through the Settings bridge disclosure.

## Interface impact

`src-tauri/src/cdp/launch.rs` owns official-process and loopback-listener discovery.
`RuntimeHandle` owns one native-only tracked endpoint irrespective of provenance. The same selected
task, target, lifecycle, transport-generation, listener, and `Rf` checks apply before a Ready or
Steer send. `connect_existing_codex_cdp` accepts an optional user preference port; it cannot accept
a host, WebSocket URL, target ID, or raw envelope from the WebView.

The Settings surface keeps automatic/custom next-action choices. In automatic mode, Connect existing
accepts only one discovered official App; in custom mode it connects only to the specified loopback
port. Neither choice persists an observed runtime endpoint.

## Alternatives

- **Require a restart through CoPets.** Retains the original launch ownership proof but unnecessarily
  disrupts a valid user-started CDP session.
- **Attach to any user-entered DevTools URL.** Rejected: it permits non-loopback, non-Codex, or
  helper endpoints and exposes an unbounded endpoint shape to the WebView.
- **Auto-scan and select the first DevTools listener.** Rejected: ambiguity is unsafe. Automatic
  mode accepts only one official same-user candidate; otherwise it asks for a custom port.
- **Persist the discovered port or target for reconnect.** Rejected: ports/PIDs are process-lifetime
  state and can be reused by another process.

## Consequences

Users can attach without restarting Codex when it was already started with a loopback CDP port.
CoPets must parse local process/listener state, monitor external listener loss, and revalidate it
before sends. The endpoint is still not a security boundary against hostile code running as the same
macOS user; it is not advertised as an official OpenAI interface.

This ADR extends ADR 0005 rather than superseding it. The no-patch, no-clone, and no private
owner-resume boundaries from ADR 0004 and ADR 0005 remain unchanged.

## Verification

- Unit tests cover loopback-only listener parsing, same-user official command/port matching,
  renderer target filtering, strict fingerprint result recognition, and concurrent cold-start probes.
- Rust runtime tests cover same tracked endpoint retry and lifecycle generation behavior.
- Frontend tests cover the compact automatic/custom Connect existing action and keep retry separate.
- Product C0e manually verifies automatic and custom attachment to a pre-launched App, rejects a
  wrong/closed/ambiguous port, and proves listener loss prevents a later Channel B send.

## References

- [ADR 0005](0005-cdp-rf-control-channel.md)
- [CDP follow-up channel](../architecture/cdp-follow-up-channel.md)
- [Existing CDP attachment live evidence](../research/codex-existing-cdp-attach-live-2026-07-26.md)
- [CDP `Rf` live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md)
