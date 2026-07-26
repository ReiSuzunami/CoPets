# Codex CDP in-renderer `Rf` live gate: 2026-07-26

> Status: Research snapshot
> Owns: Live proof that CDP can invoke Pets `Rf` → `GTu` for Ready follow-up and active-turn steer on a real profile without patching the App
> Update when: Preserve this snapshot; add a dated re-verification after App updates or Channel B productization
> Last verified: 2026-07-26

## Scope

Prove a non-bridge CDP dispatch path that reaches the same in-renderer Pets handlers official
Continue / Steer use (`Rf` → `_Ze.sendRequest` → `GTu[type]`), on the user's real Codex profile,
without modifying `app.asar`.

This snapshot supersedes the open Strategy 2 question in
[cdp-follow-up-channel.md](../architecture/cdp-follow-up-channel.md) for App build `26.721.41059`.
It does **not** claim product support or a stable minified export name across updates.

## Environment

| Item | Value |
| --- | --- |
| App | Official desktop App `26.721.41059`; exact local path redacted |
| Launch | User-confirmed local loopback wrapper launch; exact command and port redacted |
| Profile | Production user profile (same sessions as normal use) |
| Research asar | Unmodified extract under `artifacts/codex-static-research/` (matches installed asar) |
| Target conversation | User-authorized foreground test thread; title, working directory, and raw identifier redacted |

## Static wiring (unmodified asar)

From `webview/assets/app-initial-*.js`:

```text
_Ze = { setMessageHandler, sendRequest }
Rf(type, params) => _Ze.sendRequest(type, params)
qTu effect: _Ze.setMessageHandler((type, params) => GTu[type].call(null, context, params))
```

Relevant `GTu` keys:

| Type string | Role |
| --- | --- |
| `send-follow-up-message` | Ready / host follow-up; a controlled empty request reaches a recognized non-sending validation rejection |
| `steer-turn-for-host` | Owner-path active steer → internal `_Tu` → may issue `thread-follower-steer-turn` |
| `thread-follower-steer-turn-for-host` | Follower-shaped steer; asserts follower owner |

Bundler is Rolldown ESM (`import` of `app-initial-*.js`), not webpack. There is no
`webpackChunk*` capture surface.

## Live discovery

1. CDP `Runtime.evaluate` on the Codex page target.
2. `await import('app://-/assets/app-initial-<hash>.js')`.
3. Scan named exports for a function whose `Function.prototype.toString` is exactly:

   ```text
   function Rf(e,t){return _Ze.sendRequest(e,t)}
   ```

4. On this build the export name was `ddt`. **Do not treat `ddt` as stable**; rediscover by source
   fingerprint on each App update.

`GTu` and `_Ze` were not reachable as direct exports. Calling the discovered `Rf` is enough once
`qTu` has installed the message handler (normal App UI load).

## Fingerprint

| Controlled request | Result |
| --- | --- |
| Empty synthetic Ready request through `Rf` | **Rejected** by a recognized non-sending validation gate |
| Equivalent request through `electronBridge.sendMessageFromView` (Strategy 1) | Fulfilled without reaching `GTu` |

That recognized rejection is the positive GTu fingerprint for Channel B readiness.

## Live sends (user-authorized foreground)

| Action | Dispatch class | Observed |
| --- | --- | --- |
| Ready follow-up | `Rf` follow-up dispatch with the native adapter's private parameters | Fulfilled; user confirmed a new turn in the foreground test thread |
| Active steer | `Rf` steer dispatch with the native adapter's private parameters | Fulfilled; user confirmed steer applied |

The steer dispatch used the corresponding native adapter shape. Its private fields remain in native
memory and are intentionally not reproduced in this public snapshot.

Strategy 1 bridge-only send to the same foreground conversation produced no UI turn (user
confirmed absent), consistent with
[bridge vs Pets handler](./codex-bridge-vs-pets-handler-2026-07-26.md).

## Verdict

| Strategy | Ready follow-up | Active steer |
| --- | --- | --- |
| 1 — `sendMessageFromView` only | Blocked (no UI turn) | Not retested; same bridge class |
| 2 — CDP → discover `Rf` → type string | **Live pass** | **Live pass** |
| 3 — IPC exact-owner follower | Unchanged product default | Unchanged product default |

Channel B product work must implement Strategy 2 (or an equivalent that keeps the controlled
no-content fingerprint and live UI gates), not Strategy 1.

## Product implications

1. Fingerprint `Rf` by function source and a recognized controlled no-content probe, never by a
   hard-coded export name alone.
2. Resolve the `app-initial-*.js` URL from the live page (`modulepreload` / script links); the hash
   suffix changes per build.
3. Ready and Steer use distinct native-only parameter shapes; documentation does not reproduce
   private payload fields.
5. Approvals / answer / stop were not exercised on this path.
6. No App patching; ADR 0004 still holds.

## Same-day re-verification: App-side guarded export scan

This is a distinct, non-sending **renderer sub-gate** for C0 on the same installed App version after
the implementation probe encountered current live-module behavior. It does not alter the foreground
conversation and does not replace the user-authorized sends above. The tested App process had already
been reparented to `launchd`, so this run does not claim the separate CoPets-managed-child ownership
gate; product C0 remains pending a launch from the CoPets UI.

| Check | Result |
| --- | --- |
| CDP App process / loopback listener | The current local `ChatGPT` process owned its loopback CDP listener; no PID, port, page target, or conversation ID is retained here |
| CoPets-managed child proof | Not covered: the App process was reparented, so CoPets could not establish its required live child-PID ownership from this run |
| Page readiness | `/json/version` succeeded and two page targets were present |
| Renderer sanity | `window.electronBridge.getBuildFlavor()` was readable |
| `Rf` source | The exact `function Rf(e,t){return _Ze.sendRequest(e,t)}` source was present once per tested page |
| Export discovery | `Object.values(module)` hit an unrelated throwing export; `Object.keys(module)` with a per-export `try` found `Rf` without accepting that export |
| Controlled invocation | Fixed synthetic target plus empty content returned a recognized non-sending validation rejection |

The last result is a validation-order drift, not evidence of a different handler: this live build
checks the synthetic target before empty-content validation. The adapter accepts only its recognized
non-sending validation outcomes after the exact source match. Any fulfilment or other error remains a
fail-closed fingerprint miss. No nonempty prompt, follow-up, steer, or private target field was sent
in this re-verification. The renderer sub-gate passes; the complete product C0 still requires CoPets
to own the freshly launched App child and its loopback listener.

## Related

- [Bridge vs Pets handler](./codex-bridge-vs-pets-handler-2026-07-26.md)
- [message-from-view static contract](./codex-message-from-view-static-2026-07-26.md)
- [CDP electronBridge probe](./codex-cdp-electron-bridge-2026-07-26.md)
- [CDP follow-up channel spec](../architecture/cdp-follow-up-channel.md)
- [ADR 0004](../decisions/0004-retire-codex-resume-lab.md)
