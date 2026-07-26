# Bridge vs Pets handler equivalence: 2026-07-26

> Status: Research snapshot
> Owns: Evidence that `electronBridge.sendMessageFromView` is not equivalent to official Pets `GTu` follow-up handling
> Update when: Preserve this snapshot; add a dated re-verification after App updates or a positive renderer-path live gate
> Last verified: 2026-07-26

## Scope

Decide whether CoPets CDP Channel B can treat

```text
electronBridge.sendMessageFromView({ type: "send-follow-up-message", ... })
```

as the same control path official Pets uses (`Rf` / overlay helper → in-renderer `GTu["send-follow-up-message"]`).

No production profile was used for the live probe. No real user conversation received a follow-up.
Production ChatGPT remained running separately.

## Environment

| Item | Value |
| --- | --- |
| App | Official desktop App build `26.721.41059`; exact local path and PID redacted |
| Research copy asar | matches installed asar (unmodified clone under `artifacts/codex-static-research/`) |
| Live probe profile | Isolated `--user-data-dir` + temp `CODEX_HOME` |
| Probe script | gitignored `tmp/cdp-probe/verify-bridge-vs-gtu.mjs` |

## Static evidence

| Needle | Main (`.vite/build/window-all-closed-*.js`) | Renderer (`app-initial` / Pets overlay) |
| --- | --- | --- |
| `send-follow-up-message` | **0** | present |
| `Cannot send an empty follow-up message` | **0** | present once in `app-initial` |
| `interrupt-conversation` | **0** | present |
| `codex_desktop:message-from-view` | channel constant / export | via preload only |
| `setMessageHandler` / `GTu` | absent | present in `app-initial` |

Official Pets follow-up therefore lives in the **renderer handler map**. The preload bridge invokes
**main** on `codex_desktop:message-from-view`. There is no main-bundle string switch on
`send-follow-up-message`.

`codex-message-from-view` CustomEvent is only **dispatched** from the in-page `postMessage` helper
(after it also calls the bridge). No second listener string ties that event back into `GTu`.

## Live CDP fingerprint

On an isolated CDP-launched instance (`electronBridge` present, `getBuildFlavor() === "prod"`),
three structurally different synthetic bridge requests all **fulfilled** with `undefined` and did
not return the known no-content renderer validation rejection. No production prompt, conversation
identifier, or private target field was sent.

That renderer fingerprint never appeared. Global scan found no reachable function source containing the
fingerprint (expected: `GTu` is module-private).

## Verdict

**Not equivalent.**

`sendMessageFromView({ type: "send-follow-up-message", ... })` is callable over CDP but does **not**
execute the official Pets renderer follow-up handler under this App build. Observed behavior is
consistent with main accepting/dropping an unknown or non-routed view message (fulfill +
`undefined`) rather than running `GTu`.

Therefore CDP Channel B in
[cdp-follow-up-channel.md](../architecture/cdp-follow-up-channel.md) **must not** assume bridge
Ready follow-up equals Pets Continue until a different dispatch strategy is proven—for example
invoking the in-renderer `Rf`/`GTu` path, or discovering a main forwarder that produces the GTu
empty-prompt fingerprint and a selected-Ready live turn on a real profile.

## Implications

1. Keep IPC exact-owner Ready follow-up as the default product path.
2. Treat preload-bridge-only Channel B as **blocked** for Ready send (this snapshot).
3. Do not patch the App to expose `GTu`.

## Re-verification — Strategy 2 live pass (same day)

On a user-authorized foreground test conversation (title, working directory, and identifier
redacted), CDP discovered the `Rf` export from `app-initial` by function-source fingerprint and:

- reproduced the empty-prompt GTu rejection via `Rf` (not via the bridge);
- produced a visible Ready follow-up turn;
- produced an active-turn steer confirmed in the UI.

Full sanitized environment and product implications:
[CDP Rf handler live gate](./codex-cdp-rf-handler-live-2026-07-26.md).

Strategy 1 (bridge-only) remains blocked. Strategy 2 is the evidenced Channel B dispatch path for
that App build.

## Related

- [message-from-view static contract](./codex-message-from-view-static-2026-07-26.md)
- [CDP electronBridge probe](./codex-cdp-electron-bridge-2026-07-26.md)
- [CDP Rf handler live gate](./codex-cdp-rf-handler-live-2026-07-26.md)
- [CDP follow-up channel spec](../architecture/cdp-follow-up-channel.md)
- [ADR 0004](../decisions/0004-retire-codex-resume-lab.md)
