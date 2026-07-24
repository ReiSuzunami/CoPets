# Contributing

CoPets integrates with private, version-sensitive local interfaces. Small patches can have cross-task, privacy, or routing consequences; keep changes narrow and prove the affected interface.

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Use the private process in
[SECURITY.md](SECURITY.md) for vulnerabilities and [SUPPORT.md](SUPPORT.md) for support boundaries.

## Setup

Requirements: macOS, Rust, Node.js 20 or newer, and Python 3.13.

```bash
npm install
python3 -m pip install --only-binary=:all: --require-hashes -r requirements-dev.txt
npm run tauri dev
```

Read the [documentation map](docs/README.md), [runtime architecture](docs/architecture/runtime.md), and [feature catalog](docs/features/catalog.md) before changing observation, controls, state, or pet packages.

## Workflow

1. Identify the module and its interface.
2. Add or update a focused test that crosses that interface.
3. Keep private Codex payloads inside the native adapter and preserve selected-task isolation.
4. Update the canonical document named by the [change impact matrix](docs/maintenance/updating.md).
5. Add a changelog entry for user-visible behavior.
6. Run required local checks and relevant macOS integration scenarios.

## Required checks

```bash
npm run check:all
```

Private protocol, window, rendering, control, and release changes also require the macOS integration gate in [Updating and release](docs/maintenance/updating.md).

## Change discipline

- Preserve one selection authority and one per-task lifecycle reducer.
- Route controls only to the exact selected task's live owner.
- Do not emit raw task/request IDs or private snapshots to the WebView.
- Ignore unknown private-schema data instead of guessing.
- Keep official pet fields compatible; use the documented CoPets extension field for high-resolution assets.
- Do not commit prompts, answers, credentials, raw payloads, or stable user identifiers.
- Add an ADR for lasting interface, ownership, protocol, privacy, or dependency decisions.

## Pull request checklist

- [ ] Scope and user-visible outcome are described.
- [ ] Focused tests cover the changed interface.
- [ ] `npm run check:all` passes.
- [ ] Required real-app verification is recorded with sanitized evidence.
- [ ] Feature, architecture, protocol, and maintenance docs are updated where triggered.
- [ ] `CHANGELOG.md` is updated when behavior changed.
- [ ] Private-interface limits and unverified assumptions are explicit.
