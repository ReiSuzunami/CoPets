# Codex CDP electronBridge probe: 2026-07-26

> Status: Research snapshot
> Owns: 2026-07-26 live CDP renderer/`electronBridge` evidence; not current product contract
> Update when: Preserve this snapshot; add a dated re-verification or new snapshot after App updates
> Last verified: 2026-07-26

## Scope

Live, read-only probe of whether a wrapper-launched ChatGPT/Codex App exposes a CDP
renderer target and whether `window.electronBridge` is reachable from
`Runtime.evaluate`. No follow-up, interrupt, or approval messages were sent. No
production profile was reused. The already-running production App was left running.

## Environment

| Item | Value |
| --- | --- |
| Installed App | `/Applications/ChatGPT.app` (`26.721.41059` / `5848`) |
| Chromium (CDP) | `Chrome/150.0.7871.128`, Protocol `1.3` |
| Probe profile | Isolated `--user-data-dir` + temporary `CODEX_HOME` under `tmp/cdp-probe/` |
| Production App | Remained running (PID observed at start); no TCP CDP listener on that instance |

## Results

### A. `BUILD_FLAVOR=dev` + isolated profile (second instance)

- Browser CDP endpoint came up (`/json/version` OK).
- Launch log reported `allowDevtools=true allowInspectElement=true buildFlavor=dev`.
- Main app then failed: `Desktop bootstrap failed to start the main app phase=bootstrap-import-main`.
- `/json/list` stayed at **0 page targets**. Verdict: **endpoint-only**.

### B. Default prod flavor + isolated profile (second instance)

- Browser CDP endpoint came up without setting `BUILD_FLAVOR`.
- Within ~1.3 s, `/json/list` showed one `type=page` target at `app://-/index.html` (title "Codex").
- `Runtime.evaluate` saw:
  - `globalThis.electronBridge` present (`typeof object`)
  - `globalThis.codexWindowType` present
  - `electronBridge.sendMessageFromView` is a function (`[native code]`, arity `0`)
  - `electronBridge.getBuildFlavor()` returned `"prod"`
  - `electronBridge.windowType` / `codexWindowType` reported `"electron"`
- `getInitialSidebarBootstrap()` returned an object whose **shape** (keys/types only) included
  `catalogSnapshot`, `globalStateEntries`, `workspaceRootOptions`, `projectlessWorkspaceRoot`.
  No conversation bodies, prompts, or raw IDs were persisted.
- Direct property-name search on `globalThis` / `electronBridge` found **no**
  `localConversations`, `remoteTasks`, `controlTarget`, or `send-follow-up*` symbols.
- Only one page target was listed; no separate Pets-overlay target appeared in this cold profile.

Verdict: **renderer target + preload bridge readable**. This does **not** prove that
`sendMessageFromView('send-follow-up-message', …)` works, that host resume is reachable, or
that Pets overlay state is addressable.

## Interpretation for dual-channel follow-up

What this proves:

1. **CDP launch path works without modifying the `.app` / `app.asar`**, when CoPets (or a wrapper)
   starts a separate instance with `--remote-debugging-address=127.0.0.1`,
   `--remote-debugging-port=<port>`, and an explicit `--user-data-dir`.
2. The main Codex WebView exposes `electronBridge.sendMessageFromView` to CDP-evaluated scripts.
3. Ordinary already-running Prod App still has **no** CDP listener; dual-channel requires a
   **CDP-enabled launch**, not hot attach.

What this does **not** prove:

1. Message method names / payload shapes for follow-up, interrupt, or approval.
2. Access to in-bundle `localConversations` / `remoteTasks` (likely module-private, not globals).
3. That a cold isolated profile can operate on the user's real conversations (it cannot).
4. That Pets overlay UI is a distinct CDP target or that host `resumeConversationForUnavailableOwner`
   is callable from the renderer bridge.
5. Stability across Codex updates (bridge method set is private and version-sensitive).

Product implication if pursued:

```text
default launch  -> IPC follower only (current); Ready follow-up stays exact-owner gated
CDP wrapper launch -> optional experimental channel; gate follow-up UI on verified CDP bridge
```

Any CDP channel must: bind loopback only, use a random high port, never persist Runtime/Network
payloads, fail closed when the bridge fingerprint drifts, and stay out of the default production
path until a live send/receive fixture exists on a disposable profile.

## Reproduction

Probe scripts (gitignored under `tmp/cdp-probe/`):

1. `probe-cdp.mjs` — endpoint + page discovery + shallow bridge presence
2. `probe-bridge-deeper.mjs` — bootstrap shape + bridge metadata only

Do not point `--user-data-dir` at the production Application Support directory for exploratory
sends. Prefer a disposable profile, then graduate to an explicit opt-in wrapper over the real
profile only after message fixtures exist.

## Live injection smoke: 2026-07-26

Isolated second instance (disposable `--user-data-dir`, production App left running). Scripts under
gitignored `tmp/cdp-probe/`.

| Step | Result |
| --- | --- |
| DOM inject via `Runtime.evaluate` | **Success.** Marker element persisted (`data-copets-cdp`). |
| Wrap `electronBridge.sendMessageFromView` | **Success.** |
| IPC reachability | Calls hit Electron remote method `codex_desktop:message-from-view`. |
| `(channel:string, payload)` / most envelopes | Host throws `TypeError: Cannot read properties of undefined (reading 'startsWith')`. |
| Single object `{ type: "send-follow-up-message", conversationId, text }` | **No throw**; return `undefined`. Not proof the host executed a follow-up (fake UUID, cold profile). Later static work shows Pets uses **`prompt` + `serviceTier`**, not `text` — see [message-from-view static](./codex-message-from-view-static-2026-07-26.md). |
| Loaded page script sample | `app://-/assets/index-HpxHYSUy.js` |

Interpretation:

1. **CDP script injection into the Codex WebView works** without modifying the `.app` bundle.
2. The preload bridge is callable; the likely first-arg shape is a **single object with `type`**, not a bare channel string.
3. Semantic follow-up success (host routes to a real conversation / resume path) remains **unproven**. Next step needs either bounded static extraction of the `message-from-view` handler or a disposable profile with a real local conversation plus main-process log correlation.

## Related

- [DevTools/CDP investigation (2026-07-19)](./codex-devtools-hook.md)
- [Security and legal boundary](./security-and-legal-boundary.md)
- [Owner recovery static evidence](./codex-selection-and-owner-recovery-static-2026-07-26.md)
- [ADR 0004: Retire cloned Codex Resume Lab](../decisions/0004-retire-codex-resume-lab.md)
