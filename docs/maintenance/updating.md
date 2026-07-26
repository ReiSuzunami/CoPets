# Updating and release rules

> Status: Normative
> Owns: Change impact, documentation discipline, compatibility verification, versioning, and release gates
> Update when: Development workflow, required checks, version policy, or release process changes
> Last verified: 2026-07-26

## Rule: one fact, one owner

Before editing documentation, find the canonical owner in the [documentation index](../README.md). Update that document first. Secondary pages keep a short summary and link back.

Do not copy private protocol fields, numeric limits, dependency versions, window dimensions, or signing settings across multiple documents. Point to the defining symbol/configuration unless the exact value is part of a user-facing contract.

## Required document metadata

Every Markdown file under `docs/` must declare `Status`, `Owns`, `Update when`, and `Last verified` near its title. Source references should use repository-relative links plus symbol names, not line numbers that drift after edits.

Research rules:

1. Record snapshot date and exact environment/version evidence.
2. Separate observed facts from inference.
3. Redact prompts, answers, credentials, raw request payloads, and stable user identifiers.
4. Reverify unstable external behavior before using it as a current claim.
5. Preserve old snapshots; add a new snapshot or re-verification note instead of rewriting history.

## Change impact matrix

| Changed area | Required documentation | Required verification |
| --- | --- | --- |
| `package.json`, Node dependencies, npm scripts | README quick start, this workflow when gates change | `npm ci`, `npm run check:all` |
| Rust dependencies/features | Architecture if module capability changes | `cargo check`, `cargo test` |
| Observation adapters or lifecycle reducer | Runtime architecture, multi-session rules, feature catalog, research snapshot if external behavior changed | Node observer tests, Rust observer tests, live probes |
| Control models/routing | Runtime architecture, feature catalog, compatibility research when method shapes change | Rust control/observer tests and selected-task live test |
| CoPets CDP launch, user-confirmed restart, or existing local attachment | CDP channel contract, ADR, runtime/multi-session rules, feature catalog, user guide, changelog, and dated local endpoint research | Rust CDP/control/observer tests, frontend build, and C0a/C0r/C0e/C2/C2b product gates on the pinned App |
| Tauri commands/events/window/CSP | Runtime architecture and feature catalog | frontend build, Rust tests, packaged-app smoke test |
| Svelte/Pixi behavior | Feature catalog; architecture only when ownership/interface changes | relevant Node tests, frontend build, visual verification |
| Pet manifest, validation, or atlas rows | Pet package contract and feature catalog | Rust pet tests, renderer tests, atlas QA/provenance check |
| Signing/bundle configuration | README build command and release checklist | signed build plus `codesign` verification |
| macOS installer, upgrade, uninstall, or DMG layout | User guide, feature catalog, changelog, and release checklist | installer behavior matrix, universal/signature/minimum-OS audit, read-only DMG mount, copied-helper eject, checksum |
| Private Codex App update | New/reverified research evidence and compatibility status | IPC/session/log probes and real selected/background task scenarios |
| Security/privacy behavior | Runtime privacy boundary and dated security research | data-flow review and observer/control tests |

## Verification tiers

### Fast module gate

Run the narrow tests for the module being changed. Test the interface used by callers, not private implementation steps.

### Required local gate

```bash
npm run check:all
```

`npm run check:all` is the single source gate. It covers Node tests, the frontend production build,
documentation and public-tree checks, Rust formatting plus locked check/tests, and native-atlas
Python tests. The hash-pinned development tools also generate deterministic Finder DMG metadata.
[`rust-toolchain.toml`](../../rust-toolchain.toml), [`package-lock.json`](../../package-lock.json), and
[`requirements-dev.txt`](../../requirements-dev.txt) pin the toolchain or dependencies used by that
gate. Python verification uses hash-pinned Pillow, `ds-store`, and `mac-alias` distributions.

### Automated source gate

[`ci.yml`](../../.github/workflows/ci.yml) runs the same `npm run check:all` command on a locked macOS
runner setup for pushes, pull requests, and manual dispatch. External actions are pinned to full
commit hashes, dependencies install from committed lock/version files, workflow permissions are
read-only, and checkout credentials are not persisted.

CI validates source only. It does not sign, notarize, publish, access Codex user data, or use release
secrets. Signing and real-App compatibility remain explicit macOS integration/release gates.

### macOS integration gate

Required for private Codex interfaces, selection, controls, focus/hover, window geometry, rendering, and signing changes:

```bash
npm run probe:ipc
npm run probe:sessions
npm run probe:logs
npm run build:macos:signed -- --bundles app
codesign --verify --deep --strict "src-tauri/target/release/bundle/macos/CoPets.app"
```

Use a running Codex App and cover at least: selected working task, background working task, switch away and back to a retained task, terminal transition, IPC disconnect/reconnect, stale-owner follow refresh (including a same-owner reissued state snapshot when supported), and an explicit control when the change touches those paths. Record sanitized evidence; never commit live conversation content.

For a CDP launch or restart-handoff change, also run the CDP contract's C0a/C0r cold-launch gate:
prove the rediscovered official PID/listener boundary and, if a relevant macOS permission prompt
appears, record its displayed app name as an observation. Do not infer that label from process ancestry
or claim a result when no prompt was exercised.

For installer, bundle, icon, or DMG changes, also run:

```bash
npm run package:macos:dmg
```

The packager builds a universal app and installer, runs first-install/upgrade/conflict/symlink/
corrupt-payload/uninstall behavior tests in isolated temporary roots, audits the read-only DMG,
tests exact image resolution and copied-helper eject, and writes a SHA-256 sidecar under
`artifacts/release/v<version>/`. The default local identity is development-only; set
`COPETS_SIGNING_IDENTITY` to an existing development identity when required. Never use
`COPETS_PACKAGE_SKIP_APP_BUILD=1` for a release artifact.

## Compatibility update procedure

Private Codex surfaces have no compatibility promise. After a Codex App update:

1. Record app version/build, embedded CLI version, date, and platform in a new or reverified research snapshot.
2. Run IPC, session, and log probes independently.
3. Verify framing/initialization, selected-task ownership, one lifecycle transition, terminal sealing, and control availability.
4. Test a background task to detect selection regressions.
5. Update feature status to Experimental/Limited if any source degrades.
6. Prefer a sanitized fixture for new schema shapes when the test harness supports it.
7. Never weaken validation merely to accept an unknown payload.

## Versioning and changelog

The project uses Semantic Versioning. Before 1.0, incompatible private-adapter changes may ship in a minor version, but user-visible behavior changes still require clear changelog entries.

- Patch: compatible fixes, presentation polish, internal hardening.
- Minor: new user-visible capability, new adapter, new pet protocol extension, or incompatible pre-1.0 behavior.
- Major: post-1.0 incompatible public package/configuration contract.
- Documentation/research-only changes: no version bump unless they correct a published contract.

Keep versions synchronized in [`package.json`](../../package.json), [`src-tauri/Cargo.toml`](../../src-tauri/Cargo.toml), and [`src-tauri/tauri.conf.json`](../../src-tauri/tauri.conf.json). Add user-visible changes under `Unreleased` in [`CHANGELOG.md`](../../CHANGELOG.md), then move them into a dated version section at release.

## Architecture decisions

Create an ADR from the [template](../decisions/0000-template.md) when a change:

- adds or removes an observation/control seam;
- changes which module owns state or validation;
- introduces a persistent data format or public package field;
- weakens a privacy/security invariant;
- replaces a major renderer/runtime dependency;
- has two viable designs with lasting tradeoffs.

Small bug fixes and reversible UI polish do not need ADRs.

## Release checklist

1. Freeze scope and update `CHANGELOG.md`.
2. Synchronize versions when releasing a new version.
3. Run required local and macOS integration gates.
4. Build and audit the self-cleaning DMG with `npm run package:macos:dmg`.
5. Verify both app and installer architectures, minimum macOS, identifiers, nested/outer signatures,
   Finder layout, install, upgrade, rollback boundary, conflict and symlink refusal, uninstall data
   preservation, copied-helper eject, and SHA-256.
6. Use a Developer ID identity, notarization, and stapling for public distribution. A local identity
   is development-only and must be labeled as such in a private prerelease.
7. Inspect icons, pet discovery, task selection, terminal settle, controls, drag/resize restore, and
   Retina rendering.
8. Publish only after compatibility evidence names the tested Codex App version.
9. Download the published assets again, verify the checksum, mount read-only, and repeat signature,
   identity, architecture, and launch checks.
10. Keep release notes honest about private-interface and platform limits.

## Definition of done

A change is done only when implementation, focused tests, user-visible documentation, changelog entry when applicable, and required verification agree. `npm run docs:check` must pass with no orphaned document or broken local link.
