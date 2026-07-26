# Codex `message-from-view` / Pets follow-up static contract: 2026-07-26

> Status: Research snapshot
> Owns: Read-only static contract for WebView→desktop follow-up messaging; not product behavior
> Update when: Preserve this snapshot; add a dated re-verification after App updates
> Last verified: 2026-07-26

## Scope

Bounded static inspection of an **unmodified** local research copy of the installed ChatGPT/Codex
App. No `app.asar` patch, no resume bridge, no product code changes. Raw extracts stay under
gitignored `artifacts/codex-static-research/` and are not committed.

This snapshot records channel names, message `type` strings, and parameter field names only. It
records no prompts, conversation IDs, credentials, or live payloads.

## Research clone

| Item | Value |
| --- | --- |
| Source | `/Applications/ChatGPT.app` |
| Research copy | `artifacts/codex-static-research/ChatGPT-unmodified-research.app` |
| Version / build | `26.721.41059` / `5848` |
| `app.asar` SHA-256 | matches source at copy time (see clone `README.md`) |
| Policy | read-only; do not patch |

## Preload bridge contract

Source: `.vite/build/preload.js`

- Channel: `codex_desktop:message-from-view` (invoke)
- API: `window.electronBridge.sendMessageFromView(message)`
- **Arity / shape:** one argument. It must be an **object** with a string `type` field.
- Special case: `type === "shared-object-set"` also updates the preload shared-object snapshot.
- Implementation sketch: `ipcRenderer.invoke("codex_desktop:message-from-view", message)`.

This matches the earlier CDP smoke result: string-first calls throw in the host
(`…startsWith` on `undefined` because `message.type` is missing); a single object with `type`
does not throw.

Outbound desktop→view uses `codex_desktop:message-for-view` (and optional chunked transfer ACKs).

## Official Pets / renderer dispatch path

Pets overlay assets call a thin request helper (minified `be` / `T`, same family as `Rf`) as:

```text
helper("<type>", { ...fields })
```

That helper is wired to an in-renderer handler map (minified `GTu`) via:

```text
_Ze.setMessageHandler((type, params) => GTu[type].call(null, context, params))
Rf(type, params) -> _Ze.sendRequest(type, params)
```

Important separation:

| Path | Where it runs | Used by |
| --- | --- | --- |
| `Rf` / Pets helpers → lookup `GTu` by type string | Renderer (app-initial / overlay) | Official Pets controls |
| `electronBridge.sendMessageFromView` with `{type,…}` | Preload → **main** IPC | Generic desktop bridge |

CDP can call the preload bridge. That is **not automatically** the same as invoking `GTu` inside
the renderer. Semantic follow-up success still needs a live fixture.

## Follow-up message contract (Pets + app-initial)

### Type

`send-follow-up-message`

### Pets overlay fields (minimal observed)

From `avatar-overlay-page-*.js` / `avatar-overlay-native-page-*.js`:

| Field | Role |
| --- | --- |
| `conversationId` | Target local conversation |
| `prompt` | User text (**not** `text`) |
| `serviceTier` | Resolved tier (`local` fallback observed in call sites) |

Gate before send: `controlTarget.type === "app-server-conversation"`, then
`controlTarget.conversationId`.

### Full app-initial fields (richer call site)

| Field | Role |
| --- | --- |
| `hostId` | Host binding |
| `conversationId` | Target conversation |
| `prompt` | User text (trimmed; empty rejected) |
| `serviceTier` | Tier |
| `model` | Optional |
| `reasoningEffort` | Optional |
| `messageMetadata` | Optional metadata |

Handler behavior (shape only): rejects empty `prompt`; tracks analytics as a `steer`-class message
event; updates next-turn settings when model/effort provided; starts/continues the turn through
the local conversation manager (`NS` / turn-start helpers). **No Unix-socket sidecar resume** in
this path—it is app-local manager logic.

### Related Pets control types (same helper)

Observed in overlay call sites / handler map keys (non-exhaustive):

- `interrupt-conversation` — `{ conversationId, initiatedBy: "user" }`
- `reply-with-command-execution-approval-decision`
- `reply-with-file-change-approval-decision`
- `reply-with-permissions-request-approval-response`
- `reply-with-mcp-server-elicitation-response`
- `reply-with-user-input-response`
- `update-thread-settings-for-next-turn`
- `remove-plan-implementation-request`

## Implications for CoPets CDP dual-channel

1. **DOM / preload injection works** on a CDP-launched instance (prior live smoke).
2. Correct bridge envelope is `{ type, ...fields }`, with follow-up using **`prompt` + `serviceTier`**,
   not `text`.
3. Official Pets does **not** need a follower fresh-owner IPC snapshot because it stays inside the
   App's conversation manager via `GTu` / host APIs.
4. A later live fingerprint showed preload `sendMessageFromView` is **not** equivalent to
   Pets/`GTu` (empty `prompt` does not raise the GTu error). See
   [bridge vs Pets handler](./codex-bridge-vs-pets-handler-2026-07-26.md).
5. Do **not** patch the research clone to “make it work.” Keep product recovery on official-App
   exact-owner IPC unless a verified in-renderer (or proven-equivalent) CDP path is demonstrated.

## Files inspected (research extract only)

- `.vite/build/preload.js`
- `.vite/build/window-all-closed-*.js` (channel constants / exports)
- `webview/assets/app-initial-*.js` (`Rf`, `GTu`, follow-up handler)
- `webview/assets/avatar-overlay-page-*.js`
- `webview/assets/avatar-overlay-native-page-*.js`
- `webview/assets/use-avatar-overlay-selection-*.js`

## Related

- [CDP electronBridge live probe](./codex-cdp-electron-bridge-2026-07-26.md)
- [Owner-recovery static evidence](./codex-selection-and-owner-recovery-static-2026-07-26.md)
- [ADR 0004: Retire cloned Codex Resume Lab](../decisions/0004-retire-codex-resume-lab.md)
