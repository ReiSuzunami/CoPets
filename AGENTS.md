# Project agent guide

Use the repository documentation before inferring architecture or behavior from filenames alone. Keep context focused: read the index first, then only the normative documents relevant to the task.

## Required orientation

Before changing code or project documentation:

1. Read [`docs/README.md`](docs/README.md) for document ownership and status.
2. Read [`docs/architecture/runtime.md`](docs/architecture/runtime.md) for module interfaces, data flow, and trust boundaries.
3. Read the task-specific documents from the routing table below.
4. Inspect the exact implementation and tests being changed. Documentation is guidance; source and current runtime evidence remain executable truth.

For a trivial, isolated edit, steps 1 and 3 may be enough. Do not load every research snapshot by default.

## Task routing

| Task area | Read before editing |
| --- | --- |
| Observation, IPC, session/log parsing, lifecycle | [`runtime.md`](docs/architecture/runtime.md), [`multi-session-state.md`](docs/architecture/multi-session-state.md), [`features/catalog.md`](docs/features/catalog.md) |
| Selection, background tasks, approvals, steering, stop | [`multi-session-state.md`](docs/architecture/multi-session-state.md), [`runtime.md`](docs/architecture/runtime.md), [`features/catalog.md`](docs/features/catalog.md) |
| Svelte UI, Pixi animation, bubbles, drag/resize, settings | Presentation section of [`runtime.md`](docs/architecture/runtime.md), [`features/catalog.md`](docs/features/catalog.md) |
| Pet manifest, spritesheet, Retina atlas, Pet Creator compatibility | [`protocol/pet-package.md`](docs/protocol/pet-package.md), [`features/catalog.md`](docs/features/catalog.md) |
| Codex App compatibility or private interface claims | Relevant dated file under [`docs/research`](docs/research), then the compatibility procedure in [`updating.md`](docs/maintenance/updating.md) |
| Build, dependencies, signing, release, versioning | [`updating.md`](docs/maintenance/updating.md), [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Broad refactor, architecture cleanup, overdesign or defensive-code reduction | [`repair-plan.md`](docs/maintenance/repair-plan.md), then the affected architecture and feature documents above |
| Lasting architecture/interface decision | [`decisions/README.md`](docs/decisions/README.md) and [`0000-template.md`](docs/decisions/0000-template.md) |

## Source-of-truth order

When sources disagree:

1. Current implementation, configuration, tests, and live probes.
2. Normative documents under `docs/architecture`, `docs/features`, `docs/protocol`, and `docs/maintenance`.
3. Dated research snapshots, used only as historical evidence.

Do not silently choose one side. Reconcile the implementation and its canonical document in the same change. Private Codex behavior is version-sensitive; reverify it against the installed App before making current compatibility claims.

## Architecture invariants

Preserve these unless an accepted ADR explicitly replaces them:

- One selected task drives the visible pet; background task updates stay isolated.
- Lifecycle and display context are stored per task. Late JSONL progress cannot reopen a terminal epoch.
- Controls target only the exact selected task's live owner. No global fallback owner.
- Steering exists only during a live turn and never starts a new turn or activates Codex App.
- Raw task/request IDs and private IPC payloads remain in native memory.
- WebView payloads contain opaque IDs and bounded selected-task previews only.
- Unknown private-schema data degrades to unavailable/ignored state; never invent success.
- Official pet fields remain compatible. CoPets high-resolution behavior uses the documented legacy-named extension.
- Approval, answer, steering, and stop require explicit user action.

## Change workflow

1. Identify affected module, interface, and canonical document owner.
2. Read focused source and tests; do not infer current behavior from research alone.
3. Add or update a test across the module's public interface.
4. Make the narrow implementation change.
5. Apply the change-impact matrix in [`docs/maintenance/updating.md`](docs/maintenance/updating.md).
6. Update [`docs/features/catalog.md`](docs/features/catalog.md) for every user-visible behavior change.
7. Update [`CHANGELOG.md`](CHANGELOG.md) when behavior changes.
8. Create an ADR only for lasting interface, ownership, protocol, privacy, or major dependency decisions.
9. Run required verification.

## Documentation rules

- One durable fact has one canonical owner. Other files summarize and link.
- Every Markdown file under `docs/` needs `Status`, `Owns`, `Update when`, and `Last verified` metadata.
- Link source paths and name symbols; avoid source line numbers that drift.
- Preserve research history. Add dated re-verification instead of rewriting old evidence as current.
- Never commit prompts, answers, credentials, raw private payloads, or stable user identifiers.

## Verification

Minimum gate:

```bash
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
```

`npm run check` includes Node tests, frontend production build, documentation validation, and `cargo check`.

For Codex private interfaces, controls, selection, window/focus behavior, Retina rendering, or signing, also run the relevant macOS integration gate in [`docs/maintenance/updating.md`](docs/maintenance/updating.md). Report what was verified locally separately from what remains inferred or untested.
