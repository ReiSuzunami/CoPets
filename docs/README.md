# Documentation

> Status: Normative index
> Owns: Documentation taxonomy, navigation, and canonical fact ownership
> Update when: A document is added, removed, renamed, or given a different responsibility
> Last verified: 2026-07-26

This index is the entry point for maintainers. Each durable fact has one canonical owner; other documents summarize and link instead of copying the full contract.

## Document map

| Area | Document | Canonical responsibility |
| --- | --- | --- |
| Product | [README](../README.md) | Project identity, quick start, capability summary |
| Product | [Product context](../PRODUCT.md) | Target users, product purpose, positioning, personality, anti-references, and design principles |
| Legal | [Asset licenses](../ASSET_LICENSES.md) | Asset provenance limits and contributor licensing representation |
| Product | [User guide](user-guide.md) | End-user installation, first run, pet management, controls, troubleshooting, updating, and removal |
| Planning | [Roadmap](roadmap.md) | Forward-looking milestones, dependencies, and exit criteria; not current feature availability |
| Agent workflow | [AGENTS](../AGENTS.md) / [CLAUDE](../CLAUDE.md) | Shared Agent reading order, invariants, and task routing |
| Architecture | [Runtime architecture](architecture/runtime.md) | Module ownership, interfaces, data flow, failure boundaries |
| Architecture | [MVP contract](architecture/mvp.md) | Product boundary and MVP acceptance criteria |
| Architecture | [Multi-session arbitration](architecture/multi-session-state.md) | Per-task lifecycle, selection, and control invariants |
| Architecture | [Multi-active preview and steering](architecture/multi-active-preview-steering.md) | Proposed multi-task projection, targeted steering interface, migration, and acceptance gates |
| Architecture | [CDP follow-up channel](architecture/cdp-follow-up-channel.md) | Experimental opt-in CDP launch, user-confirmed normal-App restart, or explicit local attachment; `Rf` Ready/Steer dispatch and session-field inheritance |
| Architecture | [Pet Preview Studio](architecture/pet-preview-studio.md) | Proposed developer previewer UX, source adapters, diagnostics, isolation, and acceptance gates |
| Features | [Feature catalog](features/catalog.md) | User-visible behavior, implementation location, tests, limits |
| Protocol | [Pet packages](protocol/pet-package.md) | Package discovery, ZIP/import contract, manifest, atlas geometry, and CoPets HD extension |
| Maintenance | [Updating and release](maintenance/updating.md) | Change impact, versioning, verification, release checklist |
| Maintenance | [Architecture repair plan](maintenance/repair-plan.md) | Audited remediation order, dependencies, acceptance gates, and evidence thresholds |
| Maintenance | [M0 clean-profile walkthrough](maintenance/m0-clean-profile-walkthrough.md) | Reproducible M0 acceptance procedure and packaged UI checklist |
| Evidence | [M0 result: 2026-07-20](maintenance/m0-result-2026-07-20.md) | Revision-pinned sanitized execution result for the completed M0 baseline |
| Evidence | [Runtime simplification gates: 2026-07-21](research/runtime-simplification-gates-2026-07-21.md) | Revision-pinned P8 decisions for owner recovery, atlas transport, and current Codex behavior |
| Evidence | [Codex multi-active steering: 2026-07-21](research/codex-multi-active-steering-2026-07-21.md) | Current official activity-tray semantics and installed-App static per-conversation steering evidence |
| Evidence | [Codex Ready follow-up: 2026-07-25](research/codex-ready-follow-up-2026-07-25.md) | Installed-App static start-turn evidence and the pending selected-Ready live gate |
| Evidence | [Codex owner-resume bridge: 2026-07-25](research/codex-owner-resume-bridge-2026-07-25.md) | Historical static owner-resume boundary and retired clone-experiment evidence |
| Evidence | [Codex selection and owner recovery: 2026-07-26](research/codex-selection-and-owner-recovery-static-2026-07-26.md) | Current static foreground-selection and follower-owner recovery evidence |
| Evidence | [Codex CDP electronBridge: 2026-07-26](research/codex-cdp-electron-bridge-2026-07-26.md) | Live CDP renderer target and `electronBridge.sendMessageFromView` presence on a wrapper-launched isolated instance |
| Evidence | [Codex message-from-view static: 2026-07-26](research/codex-message-from-view-static-2026-07-26.md) | Unmodified research-clone static contract for Pets/`send-follow-up-message` envelopes |
| Evidence | [Bridge vs Pets handler: 2026-07-26](research/codex-bridge-vs-pets-handler-2026-07-26.md) | Live+static proof that preload `sendMessageFromView` is not equivalent to Pets/`GTu` follow-up |
| Evidence | [Codex CDP Rf handler live: 2026-07-26](research/codex-cdp-rf-handler-live-2026-07-26.md) | Live Strategy 2 pass: CDP → `Rf` Ready follow-up and `steer-turn-for-host` on real profile |
| Evidence | [Existing local Codex CDP attachment: 2026-07-26](research/codex-existing-cdp-attach-live-2026-07-26.md) | Sanitized live proof for a pre-launched official loopback CDP App, including inherited-listener handling |
| Decisions | [Architecture decisions](decisions/README.md) | ADR lifecycle and index |
| Decisions | [ADR template](decisions/0000-template.md) | Required decision-record structure |
| Decisions | [ADR 0001: Pi extension CoPets bridge](decisions/0001-pi-extension-copets-bridge.md) | Proposed Pi extension/native-adapter topology and trust boundary |
| Decisions | [ADR 0002: Native per-task follow retention](decisions/0002-native-follow-retention.md) | Accepted native follow-state retention and owner-resume boundary |
| Decisions | [ADR 0003: Experimental cloned Codex owner-resume bridge](decisions/0003-experimental-codex-resume-lab.md) | Superseded historical clone-experiment decision |
| Decisions | [ADR 0004: Retire cloned Codex Resume Lab](decisions/0004-retire-codex-resume-lab.md) | Accepted official-App-only owner-recovery boundary |
| Decisions | [ADR 0005: Opt-in CDP `Rf` control channel](decisions/0005-cdp-rf-control-channel.md) | Accepted initial experimental CoPets-managed CDP launch and Pets `Rf` Ready/Steer boundary; extended by ADRs 0006 and 0007 |
| Decisions | [ADR 0006: Explicit existing CDP attachment](decisions/0006-explicit-existing-cdp-attach.md) | Accepted explicit user connection to a verified already-running local Codex CDP endpoint |
| Decisions | [ADR 0007: User-confirmed CDP restart](decisions/0007-user-confirmed-cdp-restart.md) | Accepted Settings-only restart of one normal official App into a verified loopback CDP bridge |
| Contribution | [CONTRIBUTING](../CONTRIBUTING.md) | Contributor workflow and definition of done |
| History | [CHANGELOG](../CHANGELOG.md) | User-visible changes by release |

## Document classes

### Normative

Architecture, feature, protocol, maintenance, and decision documents describe the current intended system. They carry four metadata fields near the title:

```text
Status: Normative, Proposed, Accepted, Superseded, or Deprecated
Owns: Facts for which this file is canonical
Update when: Concrete change triggers
Last verified: YYYY-MM-DD
```

### Research snapshot

Files under [`research/`](research) preserve dated evidence about external/private systems. They may become stale. Do not silently rewrite an old experiment to look current; add a new snapshot or an explicit re-verification section.

Snapshots created before the product rename may use the former development name DeskPal. Those
references remain unchanged so revision-pinned evidence keeps its original meaning.

- [Hook capability](research/codex-hook-capability.md)
- [Non-hook interfaces](research/codex-non-hook-api-capability.md)
- [Running-app attachment](research/codex-app-parasitic-attachment.md)
- [Official Pets control parity](research/official-pets-control-parity.md)
- [DevTools/CDP investigation](research/codex-devtools-hook.md)
- [Codex CDP electronBridge: 2026-07-26](research/codex-cdp-electron-bridge-2026-07-26.md)
- [Codex message-from-view static contract: 2026-07-26](research/codex-message-from-view-static-2026-07-26.md)
- [Bridge vs Pets handler: 2026-07-26](research/codex-bridge-vs-pets-handler-2026-07-26.md)
- [Codex CDP Rf handler live: 2026-07-26](research/codex-cdp-rf-handler-live-2026-07-26.md)
- [Existing local Codex CDP attachment: 2026-07-26](research/codex-existing-cdp-attach-live-2026-07-26.md)
- [Pi extension integration](research/pi-extension-integration.md)
- [Security and legal boundary](research/security-and-legal-boundary.md)
- [Runtime simplification gates: 2026-07-21](research/runtime-simplification-gates-2026-07-21.md)
- [Codex multi-active steering: 2026-07-21](research/codex-multi-active-steering-2026-07-21.md)
- [Codex Ready follow-up: 2026-07-25](research/codex-ready-follow-up-2026-07-25.md)
- [Codex owner-resume bridge: 2026-07-25](research/codex-owner-resume-bridge-2026-07-25.md)
- [Codex selection and owner recovery: 2026-07-26](research/codex-selection-and-owner-recovery-static-2026-07-26.md)
- [Retired Accessibility bridge artifact](research/retired-app-bridge.rs)

### Evidence snapshot

Dated execution results use `Status: Evidence snapshot`. They pin the tested revision and
environment, remain immutable, and support a normative procedure without becoming a current
compatibility claim. Add a new dated result instead of revising an older run.

### Generated evidence

Screenshots, atlas provenance, probe output, and build artifacts support verification but do not define behavior. Keep secrets, raw prompts, answers, request payloads, and stable user identifiers out of committed evidence.

Local generation runs and exploratory brand studies live under ignored `artifacts/` and `design/`
directories. Curated production identity assets live under [`src-tauri/icons/`](../src-tauri/icons),
and committed test evidence must be minimal, sanitized, and linked from its canonical document.
Tauri schemas under [`src-tauri/gen/`](../src-tauri/gen) remain committed because capability files
reference them and clean checkouts do not currently regenerate them before validation.

## Canonical ownership

| Fact | Canonical owner |
| --- | --- |
| npm commands, Node requirement, JS dependency versions | [`package.json`](../package.json) |
| Rust dependency versions and features | [`src-tauri/Cargo.toml`](../src-tauri/Cargo.toml) |
| Window, CSP, bundle identity and targets | [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) |
| Observation, lifecycle, selection, preview limits | [`src-tauri/src/observer/`](../src-tauri/src/observer) |
| Control validation and compact request summaries | [`src-tauri/src/control.rs`](../src-tauri/src/control.rs) |
| Pet package validation | [`src-tauri/src/pet.rs`](../src-tauri/src/pet.rs) |
| Renderer behavior and UI interaction | [`ui/PetWindow.svelte`](../ui/PetWindow.svelte), [`ui/SettingsWindow.svelte`](../ui/SettingsWindow.svelte), [`ui/SettingsPanel.svelte`](../ui/SettingsPanel.svelte), and [`ui/lib`](../ui/lib) |
| Native atlas generation and provenance | [`scripts/build_native_atlas.py`](../scripts/build_native_atlas.py) |
| Current user-visible coverage and limitations | [Feature catalog](features/catalog.md) |
| Current audited repair sequencing and acceptance | [Architecture repair plan](maintenance/repair-plan.md) |
| Proposed multi-active task projection and targeted steering contract | [Multi-active preview and steering](architecture/multi-active-preview-steering.md) |
| Experimental CDP Ready/Steer channel and session-field inheritance | [CDP follow-up channel](architecture/cdp-follow-up-channel.md) |
| Historical claims about Codex private surfaces | Dated files under [`research/`](research) |

Source code remains the executable truth. Normative docs explain why modules exist, what their interfaces guarantee, and where behavior must be updated.

## Validation

Run the local documentation guard:

```bash
npm run docs:check
```

It checks local Markdown links, required metadata, and whether every document under `docs/` is reachable from this index. It does not validate external URLs or prove that private Codex behavior is still current.
