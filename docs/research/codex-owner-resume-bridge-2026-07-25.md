# Codex owner-resume bridge research: 2026-07-25

> Status: Research snapshot
> Owns: Static evidence for the App-local owner-resume boundary and a retired clone experiment
> Update when: Official App static evidence or historical-retirement status changes
> Last verified: 2026-07-25

## Scope

This is a bounded static inspection of the installed Codex App. It records method names, current
version/build, asset paths, and cryptographic hashes only. It records no prompt, response,
conversation ID, owner ID, host ID, private payload, credential, or live task content.

## Retirement note: 2026-07-26

The clone-patch design recorded below is historical static-analysis evidence only. [ADR
0004](../decisions/0004-retire-codex-resume-lab.md) retires the builder, bridge, tests, and clone
artifact because a copied process cannot attach to the official window's live execution state. The
current product pairs only with the unmodified official Codex App.

## Environment

| Item | Value |
| --- | --- |
| App bundle | `/Applications/ChatGPT.app`, `com.openai.codex` |
| App version / build | `26.721.41059` / `5848` |
| App archive | `Contents/Resources/app.asar` |
| Native socket asset | `.vite/build/src-BPbHdvxe.js` |
| Native socket asset SHA-256 | `efcbdf277ce7c7b78db991bef1b05fd2fa78635f2c69c0d204cb3dcbd8e49a38` |
| WebView asset | `webview/assets/app-initial-BHB6SClA.js` |
| WebView asset SHA-256 | `09909b1444003ea23a48d5fa973bedf48b638c6d6ef3059fb48a9f262e73513e` |

## Observed internal flow

The WebView asset contains `cpn`, a helper used by follower start, steer, compact, settings, and
history actions. When its local stream role is `follower` and the delegated request returns a
`no-client-found` class error, it marks the conversation as needing resume and calls
`resumeConversationForUnavailableOwner` with `model: null`; the local conversation manager derives
the rest of the resume request context.

That manager eventually sends `thread/resume` through an app-local WebView `mcp-request` bridge.
The bridge binds the request to a `hostId` and routes it to an app-server manager. It is not the
same transport as the sidecar Unix socket.

## Socket boundary

The sidecar socket router uses client discovery for a request without `targetClientId`; it does not
send such a request directly to the app-server manager. Current registered socket handlers cover
the `thread-follower-*` actions and approvals/answers but do not cover `thread/resume`.

Therefore an external sidecar cannot safely call the App's normal resume method. Sending a bare
`thread/resume` request has no verified route and would require invented context. The normal CoPets
implementation must keep failing closed on an unavailable owner.

## Lab patch design

The Lab builder creates a separately named, ad-hoc-signed clone only after the exact version and two
asset hashes above match. Its patch adds one private method across the existing socket handler,
WebView request dispatch, and host-manager request map. The method accepts `conversationId`,
`hostId`, and an opaque one-time `bridgeNonce`; the Lab checks the nonce shape, host equality, an
existing known conversation, a `follower` role, and a string owner before calling the existing local
resume method. Its response echoes a fixed Lab marker, protocol version, and that nonce, so CoPets
can ignore competing non-targeted socket responses. It never receives the user prompt or tries to
form a `thread/resume` payload in CoPets.

This is an inference from static code, not a delivery result. The generated clone is not launched by
the builder and the official app is not modified. A live selected-Ready result must be recorded in a
new dated snapshot.

## Historical reproduction (retired)

The following commands reproduce the discarded clone experiment as it existed on 2026-07-25. The
builder and artifact were removed by ADR 0004; do not run or recreate this patch for CoPets.

```bash
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Applications/ChatGPT.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' /Applications/ChatGPT.app/Contents/Info.plist
npm run build:codex-resume-lab
codesign --verify --deep --strict 'artifacts/codex-resume-lab/CoPets Codex Resume Lab.app'
```

Do not run the Lab alongside the official Codex App. Build output is ignored and disposable.
