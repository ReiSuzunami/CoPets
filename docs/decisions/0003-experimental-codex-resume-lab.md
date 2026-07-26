# ADR 0003: Experimental cloned Codex owner-resume bridge

> Status: Superseded
> Owns: Historical opt-in experimental clone bridge for a selected Ready task whose Codex owner is unavailable
> Update when: Historical status or superseding ADR changes
> Last verified: 2026-07-26

> Superseded by [ADR 0004](0004-retire-codex-resume-lab.md). The experiment is retained only as
> historical static-analysis evidence; its builder, bridge, tests, and clone artifact are removed.

## Context

The installed Codex App has an internal fallback: a follower request that receives
`no-client-found` marks the conversation as needing resume and asks its own conversation manager to
resume. The public sidecar socket does not expose that operation. A non-targeted socket request only
uses client discovery; `thread/resume` is an app-local WebView-to-host request, not a registered
socket handler. See the dated [bridge research](../research/codex-owner-resume-bridge-2026-07-25.md).

The normal CoPets Ready path remains correct: it uses one selected terminal task, an exact owner,
and an explicit follow refresh. It must still fail closed when the App reports the owner unavailable.
The user explicitly requested an experiment, but neither CoPets nor a caller may reconstruct the
model, permissions, workspace, or hidden conversation context needed for an internal resume.

## Decision

Provide an opt-in **CoPets Codex Resume Lab** only as a separately copied, ad-hoc-signed app bundle.
The bridge is disabled by default; CoPets sends it only when its process is launched with
`COPETS_ENABLE_RESUME_LAB=1`. The opt-in is process-scoped and is not persisted. The builder never
changes or launches `/Applications/ChatGPT.app`; it refuses every bundle except the version and asset
hashes documented in the dated research snapshot.

The Lab registers one private, versioned local socket method. CoPets attempts it only after an
explicit Ready follow-up has already failed with a stale-owner result through the normal path. Its
request contains the selected conversation and host identifiers plus an opaque one-time nonce, never
the prompt, model, permissions, workspace roots, or owner identifier. The Lab accepts only an exact
known conversation on its own host whose stream role is currently `follower`, then calls its existing local
`resumeConversationForUnavailableOwner` implementation with `model: null`. It uses Codex's local
conversation state to derive the remaining context.

The Lab acknowledges only that recovery was accepted, echoing its fixed bridge marker, version, and
the exact nonce. CoPets ignores every other non-targeted response until that proof arrives. CoPets
then reannounces the same exact conversation/host follow registration, waits for a new authorized
selected owner, revalidates that selection immediately before dispatch, and sends the original
follow-up only through that owner. A selection, host, lifecycle, role, hash, signature, response
proof, or recovery failure is a bounded failure; it never starts another task or changes a live
steer into a new turn.

## Interface impact

`build_resume_bridge_request`, the process-scoped `RuntimeHandle` Lab gate, and
`RuntimeHandle::request_versioned` are native-only seams. The WebView receives no new payload and
never sees raw task or owner IDs. The bridge method is a Lab implementation detail rather than a
stable Codex protocol. Its Rust sequence is tested through the same request/broadcast boundary used
by normal follow-up dispatch.

Historically, the now-removed `scripts/build-codex-resume-lab.mjs` builder changed only the cloned
app's `app.asar`, changed the clone's outer bundle identity/display name, and ad-hoc signed the
clone. Its ignored `artifacts/` output was disposable. Users had to quit the official Codex App
before opening the Lab because both used the same private local IPC environment.

## Alternatives

- **Keep the manual focus-and-retry flow only.** Safest and remains the default fallback, but does
  not test the installed App's own resume behavior.
- **Send `thread/resume` from CoPets.** Rejected: it is not a registered sidecar request and would
  force CoPets to invent local model, workspace, and permission context.
- **Patch the official app in place.** Rejected: it modifies the user's installed product, breaks
  its signature, and makes rollback/updates unsafe.
- **Use Accessibility, keyboard automation, DevTools, or a hidden composer.** Rejected: those paths
  violate CoPets' no-injection/no-automation production boundary and can affect the wrong task.

## Consequences

The Lab is deliberately brittle and limited to one dated Codex build. A Codex update, changed asset
hash, invalid signature, missing follower role, or timeout disables the experiment rather than
attempting a best-effort patch. It is development-only, not a shipping CoPets dependency, and does
not establish support for the official app's private protocol.

When disabled (the normal path), CoPets does not send a Lab bridge request and returns the normal
owner-unavailable result without the 15-second Lab recovery window. When explicitly enabled, the
experiment adds that bounded window after normal owner refresh has returned stale. It may still fail
if the selected task cannot be resumed by Codex itself. The original official app remains untouched;
removing the ignored clone removes the experiment.

## Verification

- `test/codex-resume-lab.test.mjs` verifies the patch anchors, method version, follower/host guards,
  and absence of follow-up text fields.
- Rust observer tests verify that non-targeted bridge responses are ignored until the Lab marker,
  version, and one-time nonce match, and that recovery retries only the same selected Ready
  conversation after a fresh owner arrives and a final selection check passes.
- The builder verifies the supported version and two source asset hashes, parses patched JavaScript,
  and verifies the clone's ad-hoc signature.
- A real selected-Ready recovery remains a required manual macOS gate. It is not yet proof of
  end-to-end delivery.

## References

- [Ready follow-up snapshot](../research/codex-ready-follow-up-2026-07-25.md)
- [Owner-resume bridge research](../research/codex-owner-resume-bridge-2026-07-25.md)
- [Runtime architecture](../architecture/runtime.md)
- [Multi-session state](../architecture/multi-session-state.md)
