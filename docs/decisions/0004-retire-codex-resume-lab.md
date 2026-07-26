# ADR 0004: Retire cloned Codex Resume Lab

> Status: Accepted
> Owns: Official-Codex-only follow-up and unavailable-owner boundary
> Update when: The product gains an official, verified owner-recovery interface or this boundary changes
> Last verified: 2026-07-26

## Context

The experimental Resume Lab copied and patched a Codex App bundle to expose an otherwise app-local
resume path. Static analysis established that the copied process cannot attach to an already-open
official window's live conversation: follower role, owner, host, and stream revisions are
window-local memory, not persisted task data. The normal CoPets product is intended to pair with the
unmodified official Codex App, not to ship or rely on a modified clone.

## Decision

Remove the Resume Lab builder, bridge protocol, environment gate, tests, package dependency, and
local clone artifact. CoPets sends only the existing exact-owner follower requests supported by the
official App. It never patches, clones, launches, automates, invokes, or emulates the App's private
resume path.

When an exact owner remains unavailable after the bounded selected-task follow refresh, CoPets keeps
the control stale and instructs the user to open that exact task in the official Codex App. A fresh
owner snapshot remains required before CoPets sends an explicit follow-up.

## Interface impact

`send_follow_up` retains its selected-task, exact-owner, and stale-refresh checks. The public native
surface removes the custom bridge method and process environment switch. No WebView payload or
official Codex bundle is changed.

## Alternatives

- **Keep the patched clone as a recovery dependency.** Rejected: it cannot acquire the official
  window's live execution authority and violates the official-App-only product boundary.
- **Send private `thread/resume` from CoPets.** Rejected: no verified sidecar route exists and the
  request would require fabricated local context.
- **Automate the official UI or forge stream state.** Rejected: either can target the wrong task and
  breaks the explicit-action and fail-closed invariants.

## Consequences

Ready follow-up remains available when Codex exposes a fresh exact owner. An unavailable owner is a
bounded failure rather than an automatically resumed task. Historical Lab research remains in
`docs/research/`; it does not define current product behavior.

## Verification

- Search confirms no product code, package script, or test references the removed bridge.
- `npm run check:all` and the signed macOS build pass.
- Manual selected-Ready verification uses only the unmodified official Codex App.

## References

- [ADR 0003](0003-experimental-codex-resume-lab.md)
- [Owner-resume bridge research](../research/codex-owner-resume-bridge-2026-07-25.md)
- [Runtime architecture](../architecture/runtime.md)
- [Multi-session state](../architecture/multi-session-state.md)
