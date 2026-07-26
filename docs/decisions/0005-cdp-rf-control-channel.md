# ADR 0005: Opt-in CDP `Rf` control channel

> Status: Accepted
> Owns: Opt-in CoPets-managed CDP launch, Channel B Ready/Steer dispatch, and its transport/security boundary
> Update when: CDP eligibility, launch ownership, `Rf` fingerprint, or Channel B scope changes
> Last verified: 2026-07-26

## Context

The exact-owner IPC follower can observe and control an active Codex task only while the App exposes
a current follower owner. It cannot resume a Ready task after that owner disappears. Research on the
unmodified official App establishes a separate, App-local path: a wrapper-launched loopback CDP page
can discover Pets `Rf` by source fingerprint and call its `GTu` handlers. On App `26.721.41059`, this
path passed the empty-prompt fingerprint plus user-authorized Ready follow-up and active Steer probes.

The old Resume Lab cannot acquire a live official window's owner state and remains retired. CDP is not
an owner-resume API; it only reaches the same official App window's own conversation manager.

## Decision

CoPets keeps IPC as its default observation/control transport. It may offer an **experimental**
Channel B only after an explicit user click directly spawns the official
`/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` executable with a loopback CDP port. The launcher
uses either an automatic random dynamic port or a user-entered unused local port, retains the spawned
PID, and requires that PID to own the chosen listener before it declares `CdpReady` or sends a
follow-up. The selected mode/custom-port preference may persist in the settings WebView; the resolved
runtime port, process, page target, raw IDs, envelopes, and prompts remain native process memory only.

Channel B accepts only a CoPets-managed `CdpReady` session whose current page passes the exact `Rf`
function-source and controlled no-content rejection fingerprint. Discovery guards each named ESM
export read so an unrelated throwing export cannot manufacture a fingerprint miss. The fixed
non-existent sentinel plus empty prompt may reject at either prompt validation or AppServerManager
lookup; only those exact expected errors pass. It rebuilds the discovery expression for each
preflight/send and never caches `Rf` on `globalThis`. Ready and working Steer use only
`send-follow-up-message` and `steer-turn-for-host`; preload `sendMessageFromView` is not a fallback.

When `CdpReady`, Ready/Steer may use the selected task's retained native conversation, host, and
workspace target without a fresh IPC owner. They still require explicit user input, the matching
selected lifecycle, exact target revalidation, and a frozen bridge generation. Approval, answer, and
stop remain exact-owner IPC operations. A CDP send failure fails closed; it does not retry through
another target, Strategy 1, or IPC start-turn.

## Interface impact

`RuntimeState` owns `ControlTransport`, an in-memory CDP port and managed child PID, and a transport
generation. The WebView receives only the transport label in `ControlSnapshot`; it never receives
ports, PIDs, or raw conversation/host identifiers. `send_follow_up` freezes the selected target plus
transport generation before dispatch; IPC checks that guard again immediately before queueing and
again in its writer before emitting a frame. `control.rs` builds the two source-proven parameter
shapes from native state.

The launcher never hot-attaches to a user-supplied existing endpoint. It requires Codex to be quit
first, binds only `127.0.0.1`, verifies the tracked child process still owns the listener, and starts
the unmodified production-profile App. CDP itself remains an unauthenticated same-user debugging
surface, so the feature is not a defense against hostile code already running as the same macOS user.
CoPets never patches, clones, injects into, or calls the private owner-resume path.

## Alternatives

- **Keep IPC-only follow-up.** Retained as the default and degraded path, but it cannot cover owner
  loss that the live `Rf` route handles.
- **Attach to any manually supplied CDP endpoint.** Rejected: it weakens ownership and endpoint trust
  and cannot prove the endpoint belongs to the user-selected App instance.
- **Use preload `sendMessageFromView`.** Rejected: live evidence proves it is not equivalent to Pets
  `GTu` for Ready follow-up.
- **Revive Resume Lab or emulate `thread/resume`.** Rejected: it cannot recover the official window's
  live session and violates ADR 0004's private-resume boundary.

## Consequences

The feature requires the user to quit Codex, then launch it through CoPets; it exposes a same-user
loopback debugging port for that App session and can drift with private Codex bundles. It remains
Experimental until product-path manual gates pass on each supported App build. Users can continue to
use passive IPC mode without launching CDP.

This ADR narrowly supersedes ADR 0004 only where that record's broad wording could be read to forbid
launching the official App at all. Its prohibition on patched clones and private owner-resume remains
unchanged.

## Verification

- Unit tests cover `Rf` script construction, loopback target validation, managed-PID readiness,
  parameter shapes, transport authorization, IPC dispatch guard rechecks, stale-IPC bypass, and
  in-flight token replacement safety.
- Frontend tests cover automatic/custom port preference validation without exposing runtime endpoints.
- Product C0/C2/C2b live gates remain required and are tracked by the CDP architecture document.

## References

- [CDP Channel B contract](../architecture/cdp-follow-up-channel.md)
- [CDP `Rf` live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md)
- [Bridge vs Pets handler](../research/codex-bridge-vs-pets-handler-2026-07-26.md)
- [ADR 0004](0004-retire-codex-resume-lab.md)
