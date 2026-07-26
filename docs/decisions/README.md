# Architecture decisions

> Status: Normative index
> Owns: ADR numbering, lifecycle, and decision index
> Update when: An ADR is added or changes status
> Last verified: 2026-07-26

Architecture Decision Records preserve decisions whose reasoning cannot be recovered safely from code alone.

## Process

1. Copy [`0000-template.md`](0000-template.md) to the next four-digit number plus a short slug.
2. Start with `Status: Proposed`.
3. Describe context, decision, rejected options, consequences, and verification.
4. Change to `Accepted` when implementation begins.
5. Never rewrite an accepted decision to describe a new design. Add another ADR and mark the old one `Superseded`.
6. Add every ADR to the index below.

## Index

| ADR | Status | Decision |
| --- | --- | --- |
| [0001 — Pi extension CoPets bridge](0001-pi-extension-copets-bridge.md) | Proposed | Integrate Pi through an opt-in extension that connects outward to a native CoPets adapter |
| [0002 — Native per-task follow retention](0002-native-follow-retention.md) | Accepted | Retain native follower registrations across task switches and IPC reconnects |
| [0003 — Experimental cloned Codex owner-resume bridge](0003-experimental-codex-resume-lab.md) | Superseded | Historical clone experiment; superseded by ADR 0004 |
| [0004 — Retire cloned Codex Resume Lab](0004-retire-codex-resume-lab.md) | Accepted | Pair CoPets only with the unmodified official Codex App |
| [0005 — Opt-in CDP `Rf` control channel](0005-cdp-rf-control-channel.md) | Superseded in part | Historical direct-launch Channel B decision; launch handoff superseded by ADR 0008 |
| [0006 — Explicit existing CDP attachment](0006-explicit-existing-cdp-attach.md) | Accepted | Attach only to a user-requested, same-user official Codex loopback CDP endpoint that passes process, listener, and `Rf` checks |
| [0007 — User-confirmed CDP restart](0007-user-confirmed-cdp-restart.md) | Superseded in part | Historical restart decision; replacement launch handoff superseded by ADR 0008 |
| [0008 — Launch Services CDP handoff](0008-launch-services-cdp-handoff.md) | Accepted | Open the official App through Launch Services, then rediscover and verify exactly one official PID |

Existing architecture documents describe the current baseline; proposed records do not change current
behavior until accepted and implemented.
