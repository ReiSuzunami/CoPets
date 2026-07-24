# Security and legal boundary

> Status: Research snapshot
> Owns: 2026-07-19 security and legal risk analysis; not legal advice or current product contract
> Update when: Preserve this snapshot; add a dated re-verification section or a new snapshot
> Last verified: 2026-07-19

Snapshot: 2026-07-19. This is risk guidance, not legal advice. Terms, product behavior, and local law can change; obtain counsel before distribution or commercial use.

## Scope and facts

This project is a local sidecar. Its observation path connects to the current-user Unix socket (`~/.codex/ipc/ipc.sock`) and tails append-only session JSONL; its control path submits only explicit user-triggered approval, input, follow-up, or stop requests. It does not proxy model traffic, expose a network listener, or persist message text. The selected task may expose bounded in-memory previews of the user's latest question and user-visible progress to the pet WebView ([README](../../README.md); [codex-app-parasitic-attachment.md](./codex-app-parasitic-attachment.md)).

DevTools/CDP is a separate experimental path. Current production Codex App does not expose a remote-debugging endpoint; a launch-time wrapper can start an explicitly instrumented development instance on a random loopback port. CDP can execute JavaScript and alter DOM/page state, but renderer bundles, React/store names, and IPC details are private and unstable ([codex-devtools-hook.md](./codex-devtools-hook.md)).

## Primary contract constraints

OpenAI individual Terms prohibit modifying, copying, leasing, selling, or distributing Services; attempting to reverse engineer, decompile, or discover source code/underlying components; automatic/programmatic extraction of data or Output; and interference, rate-limit circumvention, or bypassing protective measures ([OpenAI Terms of Use](https://openai.com/policies/terms-of-use/)). The business/developer Services Agreement likewise prohibits reverse engineering, extraction other than through the Services, and bypassing limits or safety measures; it defines reverse engineering broadly to include discovering source or underlying components ([OpenAI Services Agreement](https://openai.com/policies/services-agreement/), §§3.3; definitions).

These clauses do not automatically make every local observer unlawful. They do make private protocol discovery, renderer instrumentation, and redistribution of modified app code legally sensitive. A user’s authorization to inspect their own running process is not a license to redistribute OpenAI software or defeat technical restrictions. “Read-only” is a security property, not a contractual safe harbor.

The narrow-scope position is that the sidecar observes same-user local lifecycle signals and extracts only bounded previews needed for the selected-task hint: the latest user question and user-visible `agent_message` progress. It does not expose hidden reasoning, tool arguments, command output, full answer bodies, cookies, or credentials, and it does not persist the previews. That remains materially narrower than transcript scraping or DOM/network interception. The unresolved point is whether OpenAI would treat programmatic collection of lifecycle metadata or these bounded previews, or discovery of private IPC framing, as prohibited extraction or reverse engineering. Only OpenAI authorization or jurisdiction-specific legal advice can settle that point.

The local threat model treats processes already running under the same macOS UID as trusted. Source
identity checks prevent cross-user and path-confusion inputs; they do not sandbox Codex from a
malicious process that already has the user's own filesystem and process privileges.

Public third-party projects do not establish authorization. Codex Dream Skin describes loopback CDP DOM/CSS injection without modifying the signed app; Codex++ describes patching `app.asar`, re-signing, and loading runtime tweaks. Both identify themselves as unofficial or use-at-your-own-risk projects. The default observer here is technically less invasive than either approach, but their availability is not evidence of OpenAI approval.

## Risk matrix

| Technique | Default classification | Boundary / required controls |
|---|---|---|
| Current-user Unix IPC observer, initialization only, no control methods | **Lower risk (still private API)** | Restrict socket path and peer ownership to same UID; reject non-user socket; cap frame/message size; fail open; never log prompt/output; no network relay. Treat protocol as unversioned and require compatibility fixtures. |
| Current-user session JSONL tail with field allow-list | **Lower risk for personal use; privacy-sensitive** | Read only files owned by user; avoid copying transcript/content; redact paths, tokens, personal data; provide disable/delete controls; document retention. Do not ship another user’s logs or cloud-sync them by default. |
| Codex command hooks that emit local state then exit | **Needs caution** | Hooks run with user privileges and can affect Codex if blocking. Use absolute executable path, no shell interpolation, short timeout, fail-open, and no network. Configure only with explicit user consent; never use hook to alter prompts, approvals, or safety decisions. |
| Launch-time CDP on random loopback port for a wrapper-started dev/test instance | **Needs caution / experimental** | Bind loopback only; random high port; authenticate or firewall endpoint; lifecycle-close on exit; disclose that CDP permits script execution and page mutation. Keep out of default production mode; do not infer private DOM/store as authoritative data. |
| Attaching to already-running production App without endpoint | **Avoid** | Current build has no external CDP endpoint. Do not inject dylibs, patch bundle, attach debugger, bypass code-signing, or defeat DevTools gates. Such actions resemble modification/reverse engineering and create credential/data-exfiltration risk. |
| Redistributing modified Codex App, patched Electron bundle, or extracted internal protocol/schema | **Avoid** | Conflicts with no-modification/no-distribution and reverse-engineering restrictions; may violate copyright, license, signing/notarization, and third-party rights. |
| Network-facing IPC bridge, remote control, or multi-user relay | **Avoid by default** | Expands attack surface and can expose prompts, tokens, or control actions. Require separate threat model, authentication, authorization, encryption, and counsel review. |

## Platform security requirements

Apple describes App Sandbox as limiting app access to files, network, hardware, and other processes; App Store distribution requires the sandbox entitlement ([App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)). Outside the App Store, Apple recommends signing and notarizing Mac apps so Gatekeeper can assess code integrity ([Apple Security](https://developer.apple.com/security/)). A sidecar should therefore be separately signed/notarized, request least privilege, and avoid entitlements that grant broad file/process access. It must not weaken Codex’s signature, quarantine, SIP, TCC, or sandbox boundaries.

Electron’s security guidance recommends disabling `nodeIntegration` for remote content and enabling `contextIsolation`; preload bridges should expose the smallest API possible ([Electron security checklist](https://www.electronjs.org/docs/latest/tutorial/security)). The sidecar should keep CDP/debug endpoints out of ordinary production runs, never expose them beyond loopback, and avoid evaluating untrusted page or network content.

## Release posture

1. **Personal, local-only sidecar:** acceptable target. State clearly that it uses private, unversioned surfaces; bound selected-task previews, keep them in memory, and ship opt-in and a kill switch.
2. **Public sidecar distribution:** needs security review, privacy notice, data-retention policy, signed/notarized binaries, protocol regression tests, and legal review of the applicable OpenAI account/product terms.
3. **App modification/injection product:** do not ship absent explicit authorization or a documented supported extension API from OpenAI. Prefer official hooks/app-server/API paths where they cover the use case.

No conclusion here determines infringement, authorization, or enforceability in a particular jurisdiction. Consult qualified counsel for those questions.

## Re-verification: 2026-07-20 diagnostic sanitizer

The standalone Node IPC probe previously used a recursive key-name allowlist. An isolated
adversarial fixture showed that arbitrary strings under keys such as `reason` or `status` could
reach diagnostic output. No evidence showed that the current Codex App placed private content in
those fields or that this diagnostic-only path reached the DeskPal WebView; the defect was a local
sanitizer policy gap.

The probe now uses method-specific schemas, bounds method/version/key metadata, hashes only known
identifier fields, and accepts only declared enums, booleans, and non-negative safe integers.
Unknown methods and nested payloads retain key-shape evidence but omit values. Tests cover
free-form text under formerly allowed keys, oversized IDs/methods, excessive key counts, hostile
prototype names, unknown schemas, and the one declared nested change discriminator.

## Re-verification: 2026-07-24 local source identity

The production Rust IPC adapter now validates the path with non-following metadata, requires a Unix
socket owned by the effective user, connects, then checks the connected peer credentials before
sending the initialization frame. Reconnect attempts repeat both checks. The diagnostic Node
adapter shares the same path-type, owner, and protected-ancestor policy; the production native
adapter is the component that additionally verifies connected peer credentials.

Native session and app-log reads now discover only same-user non-symlink regular files and reopen
them with no-follow semantics before validating the opened descriptor. The thread index requires a
root- or same-user-owned, non-writable ancestor chain before it is revalidated and queried read-only;
both the lexical and resolved ancestor chains are checked so root-owned system symlinks do not hide
an unsafe target. This prevents another local user from replacing the pathname between checks. Node diagnostics apply
the same file-type, owner, and ancestor policy and use no-follow file handles for appended reads.
Unit tests cover non-socket paths, foreign-owner policy, same-user peers, writable ancestors,
symlink rejection, and ordinary same-user files. This closes the implementation gap in the
snapshot's same-user requirement; it does not turn the private interfaces into supported APIs.

### Live smoke result

On 2026-07-24, macOS 26.5.2 arm64 ran ChatGPT App 26.721.31836 (build 5828) with embedded
`codex-cli 0.146.0-alpha.3.1`. The sanitized standalone probes:

- attached to the trusted IPC socket and observed a bounded following-change fact;
- watched nine trusted session files and emitted only allowlisted diagnostic record kinds;
- watched three trusted app-log files and recovered active/inactive view facts.

This confirms that all three read-only diagnostic inputs remained available in that installed
version after the source-identity hardening. It does not verify native reconnect behavior, control
dispatch, signing, notarization, or the full selected/background-task integration matrix.
