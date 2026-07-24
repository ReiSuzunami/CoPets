# Architecture decisions

> Status: Normative index
> Owns: ADR numbering, lifecycle, and decision index
> Update when: An ADR is added or changes status
> Last verified: 2026-07-24

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

No ADRs are accepted yet. Existing architecture documents describe the current baseline; proposed
records do not change current behavior until accepted and implemented.
