# CDP follow-up channel (dual-channel control)

> Status: Normative
> Owns: Opt-in CDP Launch Services handoff, user-confirmed normal-App restart, or explicit local attachment; Ready follow-up and active-turn steer via in-renderer Pets `Rf`, session-field inheritance rules, trust boundary, and acceptance gates
> Update when: Channel eligibility, launch handoff, endpoint contract, Rf fingerprint, envelopes, field inheritance, or acceptance evidence changes
> Last verified: 2026-07-26

## Decision status

This document is an implementation specification, not a current support claim. Default CoPets
behavior remains the exact-owner IPC follower contract in
[runtime architecture](runtime.md) and
[multi-session arbitration](multi-session-state.md).

The external shapes assumed here are pinned by:

- [CDP electronBridge live probe](../research/codex-cdp-electron-bridge-2026-07-26.md)
- [message-from-view static contract](../research/codex-message-from-view-static-2026-07-26.md)
- [Bridge vs Pets handler](../research/codex-bridge-vs-pets-handler-2026-07-26.md) — Strategy 1 blocked
- [CDP Rf handler live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md) — Strategy 2 live pass
- [Existing local CDP attachment](../research/codex-existing-cdp-attach-live-2026-07-26.md) — same-user endpoint proof
- [Ready follow-up research](../research/codex-ready-follow-up-2026-07-25.md)
- [ADR 0004](../decisions/0004-retire-codex-resume-lab.md) — no patched clone, no private resume emulation

The accepted [ADR 0005](../decisions/0005-cdp-rf-control-channel.md),
[ADR 0006](../decisions/0006-explicit-existing-cdp-attach.md), and
[ADR 0007](../decisions/0007-user-confirmed-cdp-restart.md), and
[ADR 0008](../decisions/0008-launch-services-cdp-handoff.md) authorize this experimental control
transport. It must not weaken exact-target, explicit-action, process-identity, or fail-closed
guarantees.

## Outcome

CoPets keeps IPC as the default observation and control path. When the user explicitly asks CoPets
to request a Launch Services open of Codex **or connects an already loopback-CDP-enabled official
Codex App**,
CoPets may additionally dispatch
**Ready follow-up / Continue** and **active-turn Steer** by CDP-evaluating the in-renderer Pets
`Rf` helper (`_Ze.sendRequest` → `GTu[type]`) so the App-local conversation manager inherits the
live session—without CoPets inventing a follower owner or calling private `thread/resume`.

Approval, answer, and stop remain on the IPC follower path in v1. CDP does not become a second
selection authority. Preload `electronBridge.sendMessageFromView` alone is **not** a product
dispatch path (Strategy 1 blocked).

## Goals

1. Offer an opt-in Launch Services handoff, a user-confirmed restart of one normal local App, and
   an explicit existing-App connection mode for a loopback CDP endpoint on the **same** user Codex
   profile.
2. Gate Channel B Ready/Steer send on a verified **`Rf` fingerprint** (not merely bridge presence)
   in that mode only.
3. Build `Rf` envelopes by **inheriting** session fields from the selected task's native
   `ControlTarget` / IPC snapshot—never inventing conversation, host, model, or tier.
4. Keep raw conversation/host/owner IDs in native memory; WebView still sees opaque task IDs only.
5. Fail closed to the current IPC/stale-owner UX when CDP is absent, `Rf` drifts, or any
   required inherited field is missing.
6. Never patch, clone, or automate the official Codex App bundle.

## Non-goals

- Attaching to a running App without an existing loopback debugging listener, or to an arbitrary
  DevTools URL, browser, remote host, or renderer target.
- Using a cold/isolated Electron profile for product follow-up (that profile has no real sessions).
- Replacing IPC observation, selection, approvals, or stop with CDP.
- Shipping Strategy 1 (`sendMessageFromView` only) as Ready/Steer dispatch.
- Patching `app.asar`, shipping Resume Lab, or emulating `resumeConversationForUnavailableOwner`.
- Scraping `localConversations` / React store as authoritative lifecycle state.
- Persisting runtime CDP ports, page targets, bridge payloads, prompts, or raw IDs. A local
  non-sensitive preference for automatic vs custom next action and custom port may persist.
- Cloud / remote-host follow-up via CDP.
- Making CDP the default launch path.
- Automatically closing, restarting, or force-killing Codex.

## Dual-channel model

```text
CoPets native host
  RuntimeState / selection / ControlTarget (native memory only)
           |                              |
           v                              v
  Channel A (default)              Channel B (opt-in)
  IPC follower                     CDP Runtime.evaluate
  observe, approve, stop           discover Rf → GTu[type]
  steer when Cdp not ready         Ready + active Steer
           |                              |
           v                              v
  ~/.codex ipc.sock                Codex page (Rolldown ESM)
  exact owner snapshot             import(app-initial).Rf(...)
                                          |
                                          v
                                   App conversation manager
                                   (in-process session inheritance)
```

| Concern | Channel A — IPC | Channel B — CDP `Rf` |
| --- | --- | --- |
| Endpoint | Attach to already-running App | CoPets Launch Services handoff plus rediscovery, or explicit connect to one verified local CDP App |
| Selection / lifecycle | App-log + JSONL + IPC | Same native reducers (unchanged) |
| Approvals / answer / stop | Exact-owner IPC | Not in v1 scope |
| Active-turn steer | Exact-owner IPC when Channel B unavailable | `Rf('steer-turn-for-host', …)` when `CdpReady` |
| Ready follow-up | Exact-owner `thread-follower-start-turn` when owner fresh | `Rf('send-follow-up-message', …)` when `CdpReady` |
| Unavailable owner | Follow refresh → fail closed → user opens task | Prefer Channel B if session is live in that App window; else fail closed |

Channel B does not remove Channel A. Strategy 2 live evidence for Ready + Steer on App
`26.721.41059` is recorded in
[CDP Rf handler live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md). Product enablement
still requires the ADR + acceptance gates below on each pinned App build.

## Endpoint contract

### CoPets-launched endpoint requirements

The CDP launcher must:

1. Keep **Launch Codex** non-disruptive: when a non-CDP Codex/ChatGPT instance is open, ask the
   user to quit it; this path never closes it. **Restart Codex with bridge** is a separate,
   explicitly confirmed action governed below.
2. Ask macOS Launch Services to open the official
   `/Applications/ChatGPT.app` bundle (or configured official bundle) through
   `/usr/bin/open -n ... --args` with:
   - `--remote-debugging-address=127.0.0.1`
   - `--remote-debugging-port=<ephemeral>`
   The handoff helper is not the App identity. Rediscover exactly one same-user
   `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` process whose command line carries that exact
   port, then retain only that native PID.
3. Reuse the **production** Electron user-data directory and the user's `CODEX_HOME` so
   `localConversations`, thread index, and session JSONL are the same sessions the user already has.
4. Bind CDP to loopback only. Default to a random dynamic high port; an explicit custom port is
   allowed only after native availability validation. A free-port probe selects a candidate only—it
   is never ownership proof. Never persist the resolved runtime port as a stable API.
5. Before `CdpReady`, prove that the exact rediscovered PID owns the chosen loopback listening
   socket; repeat that native ownership check before every CDP send. A collision or ownership
   mismatch degrades Channel B rather than attaching to the endpoint. Recheck the exact official
   command while it is tracked; process-command liveness starts before Ready and listener liveness
   starts after Ready.
6. Wait for `/json/version` and at least one `type=page` target before marking Channel B ready.
7. Verify `window.electronBridge` is present and `getBuildFlavor()` is readable (sanity only).
8. **Fingerprint `Rf`:** resolve the live `app-initial-*.js` URL from the page, `import()` it, then
   scan named exports with `Object.keys()` and a per-export guarded read; a throwing export is not a
   candidate and must not abort discovery. Find the export whose `toString` matches
   `function Rf(e,t){return _Ze.sendRequest(e,t)}`. Then invoke it only with an empty `prompt` and
   the fixed non-existent sentinel conversation ID. Accept exactly one of two rejection modes:
   `Cannot send an empty follow-up message.` when prompt validation runs first, or
   `No AppServerManager registered for conversationId: 00000000-0000-4000-8000-000000000000` when
   manager lookup runs first. Fulfilment or any other rejection is a fingerprint failure. Re-run that
   discovery before every send; do not cache a binding on the page or hard-code minified names such
   as `ddt`.
9. If an initial readiness timeout can be explained by delayed renderer initialization (for example,
   an OS permission prompt), an explicit user retry may re-run only the ownership proof and `Rf`
   fingerprint against the same still-live tracked PID and native-only endpoint. The retry accepts
   no port or PID from the WebView and clears that endpoint when its process/listener disappears.
10. Every launch, Connect, or retry readiness pass has one hard native deadline. Launch first uses
    that budget to rediscover the official App; if no unique exact-process candidate appears, it
    returns without tracking an endpoint. Once a PID is tracked, listener, HTTP, WebSocket, and
    renderer evaluation consume the remaining budget rather than renewing a timeout per frame or
    response chunk. Connect and retry divide that one budget into bounded probe attempts so a
    transient page/socket miss can be retried without extending the deadline. The loopback HTTP
    reader accepts a bounded `Content-Length` response without waiting for the peer to close an
    HTTP/1.1 connection. Deadline expiry returns the command, leaves a tracked endpoint degraded,
    and must never leave Settings indefinitely in a launching state.
11. On CoPets quit (optional policy): leave Codex running; do not kill the user's App by default.

Do **not** set `BUILD_FLAVOR=dev` for product launches. Live probes showed `dev` can fail at
`bootstrap-import-main` while default prod flavor + remote debugging yields a page target.

### User-confirmed restart requirements

**Restart Codex with bridge** is Settings-only and must never run from a follow-up, owner-recovery,
or automatic startup path. It may proceed only after an explicit confirmation states that it closes
the current App, can interrupt active work, and can discard unsaved App UI state. The native command
must:

1. Refuse when CoPets already tracks any CDP endpoint. Otherwise identify exactly one same-user
   official `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` process with **no**
   `--remote-debugging-port` argument. Zero candidates, more than one candidate, or an already-CDP
   App fail closed with recovery guidance; the WebView never supplies a PID or process identity.
2. Revalidate that exact PID, UID, executable, and absence of a remote-debugging argument immediately
   before sending only `SIGTERM`. It never signals a process group, sends `SIGKILL`, or falls back to
   another process when the snapshot changes.
3. Wait a bounded interval for that exact official-process PID to disappear. A close refusal or
   timeout starts no replacement App and directs the user to close Codex manually. CoPets never
   force-closes Codex.
4. Only after the old process exits and no other official App remains, reuse the ordinary
   loopback-only CoPets Launch Services handoff and all of its rediscovered-PID/listener/`Rf`
   readiness checks. A later launch failure does not silently reopen a normal non-CDP App; recovery
   remains an explicit user choice.
5. Keep the target PID, command line, resolved port, and exit observation native-only. Quitting
   CoPets still leaves the user App running.

### Existing local endpoint requirements

**Connect existing** is a separate explicit user action. It may discover one candidate in automatic
mode or validate the user-entered custom port. It must:

1. Accept only one same-user official App main process at
   `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT`, with a matching
   `--remote-debugging-port=<port>` command argument. Automatic discovery derives those candidates
   from native process command lines, never from a mutable process display name.
2. Require that exact process to own an IPv4 `127.0.0.1:<port>` listener. A child helper that merely
   inherits the descriptor is not an eligible process.
3. In automatic mode, reject zero or multiple eligible processes; custom mode is then the only way
   to choose a port. Never scan or accept a host, WebSocket URL, target ID, or non-loopback listener
   from the WebView.
4. Reuse the same `/json/version`, renderer-page restriction, guarded `Rf` source fingerprint, and
   fixed no-content rejection as a CoPets-launched endpoint.
5. Keep provenance, PID, and runtime port native-only. Start external-listener monitoring only
   after the initial ownership and `Rf` verification reaches `CdpReady`; while attached, recheck
   the exact PID/listener and clear `CdpReady` on listener loss. Repeat that check before every
   send.

### Profile inheritance (hard requirement)

| Resource | Must match normal Codex | Why |
| --- | --- | --- |
| Electron `--user-data-dir` | Default ChatGPT/Codex Application Support | In-window conversation manager / bootstrap |
| `CODEX_HOME` | User's normal Codex home | Session JSONL, thread index, IPC socket path |
| macOS user | Same UID | Local trust boundary unchanged |
| Bundle | Unmodified official App | ADR 0004 |

A disposable isolated profile is allowed only for engineering probes, never for shipping
Ready follow-up. The custom-port preference is not a profile or persisted runtime endpoint.

### Mode detection

Native state owns an explicit enum:

- `ControlTransport::IpcOnly` — default
- `ControlTransport::CdpReady` — a CoPets-launched or user-attached tracked official App PID still
  owns the loopback listener, and the **`Rf` fingerprint** passed
- `ControlTransport::CdpDegraded` — endpoint tracking attempted but listener/page/`Rf` fingerprint
  is missing; Channel B withheld. A same-process verification retry is permitted only while native
  state still retains the live tracked endpoint. A bounded readiness timeout also resolves to this
  state.

The Settings UI may show the transport state, but never the runtime port or page target; it must not
imply CDP is safer or official.

## Session-field inheritance

All Channel B fields are assembled in **native** code from the selected task's already-trusted native
records. The WebView supplies only the user `prompt` and the opaque selected task id.

### Identity fields

| Field | Inherit from | Rule |
| --- | --- | --- |
| `conversationId` | Selected task `ControlTarget.conversation_id` | Exact selected task only. No “last controllable conversation” fallback. |
| Opaque task id | `RuntimeState.selected` | Revalidated immediately before send; must still map to the same native conversation. |
| `hostId` | Selected task exact IPC `host_id` | Required for both v1 Ready and Steer. If absent, fail closed rather than borrow or let the App guess a host. |

### Turn / workspace fields

| Field | Inherit from | Rule |
| --- | --- | --- |
| `prompt` | Explicit user input in CoPets | Trim; reject empty / oversize (same limits as IPC follow-up). Never reuse a previous prompt. |
| `cwd` / workspace | `ControlTarget.cwd` | Native must know cwd before offering CDP Ready send, matching IPC `prepare_follow_up`. The App handler also reads conversation cwd; CoPets still fails closed if native cwd is missing so it does not send a half-bound envelope. |
| `serviceTier` | `null` | Current native IPC snapshot does not retain a trustworthy tier. `null` is the live-proven local-manager shape; never guess a cloud tier. |
| `model` | — | Omit in v1. The current native snapshot does not retain a trustworthy model field. |
| `reasoningEffort` | — | Omit in v1. The current native snapshot does not retain a trustworthy effort field. |
| `messageMetadata` | — | Omit for v1 Continue unless a future App fixture requires a documented empty object. Do not fabricate metadata. |

### Lifecycle gates before inheritance

**Ready (Continue)** — all must hold:

1. Transport is `CdpReady`.
2. Selected lifecycle is terminal `completed` (Ready projection).
3. Native selection still points at that conversation after a final revalidation.
4. `conversationId`, `hostId`, and `cwd` are present in native memory for that task.
5. User explicitly submitted Continue / Ready follow-up.

**Steer** — all must hold:

1. Transport is `CdpReady`.
2. Selected lifecycle is non-terminal `working` with an active turn (same presentation as today's Steer).
3. Native selection revalidated; `conversationId`, `hostId`, and `cwd` present.
4. User explicitly submitted Steer.

Channel B Ready/Steer does **not** require a fresh IPC follower owner. That is the point of
Channel B. It still requires the conversation to exist in the verified CDP App's session world
(same profile). If the App returns a structured failure (unknown conversation, empty prompt, no
active turn for steer, host mismatch), CoPets surfaces a transient error and does not retry against
another task or silently fall back to Strategy 1.

### Ready envelope (v1) — `send-follow-up-message`

Params object passed as the second argument to `Rf`:

```text
{
  conversationId: <native selected>,
  prompt: <user text>,
  serviceTier: null,
  hostId: <native selected host>
}
```

Dispatch (Strategy 2 only):

```text
await Rf('send-follow-up-message', params)
```

Do **not** productize `electronBridge.sendMessageFromView({ type: 'send-follow-up-message', ... })`.

### Steer envelope (v1) — `steer-turn-for-host`

Owner-path steer (live-proven). Params align with Pets composer / `_Tu` and reuse the same
`restoreMessage` / `input` shapes as [`build_follow_up`](../../src-tauri/src/control.rs):

```text
{
  hostId: <native selected host>,
  conversationId: <native selected>,
  input: [{ type: 'text', text: <user text>, text_elements: [] }],
  serviceTier: null,
  attachments: [],
  restoreMessage: {
    id: <fresh uuid>,
    text: <user text>,
    context: {
      prompt: <user text>,
      addedFiles: [],
      fileAttachments: [],
      ideContext: null,
      imageAttachments: [],
      workspaceRoots: [<cwd>]
    },
    cwd: <native cwd>,
    createdAt: <epoch ms>
  }
}
```

Dispatch:

```text
await Rf('steer-turn-for-host', params)  // returns { turnId } on success
```

Prefer `steer-turn-for-host` over `thread-follower-steer-turn-for-host` for Channel B: the latter
asserts follower ownership and is the IPC-shaped path, not the in-window owner path proven live.

### Field source matrix (normative)

| Envelope key | Required | Source | Fail closed when |
| --- | --- | --- | --- |
| `Rf` type string | yes | `send-follow-up-message` or `steer-turn-for-host` | Unknown op |
| `conversationId` | yes | Selected `ControlTarget` | Missing / selection changed |
| `prompt` (Ready) / `input[].text` (Steer) | yes | User input | Empty / too long |
| `serviceTier` | yes (nullable) | `null` | — |
| `hostId` | yes | Selected exact host | Missing / never borrow from another task |
| `cwd` / `restoreMessage.cwd` | Steer: yes; Ready: native gate | `ControlTarget.cwd` | Missing cwd |
| `model` | no | — | Omit in v1 |
| `reasoningEffort` | no | — | Omit in v1 |
| `messageMetadata` | no | — | Omit in v1 |
| `attachments` | Steer: yes | `[]` in v1 | — |

## Dispatch strategies

### Strategy 1 — Preload bridge only — **blocked for Ready send**

`Runtime.evaluate` calling only:

```text
window.electronBridge.sendMessageFromView(envelope)
```

Live fingerprint on App `26.721.41059` shows this path is **not** equivalent to Pets/`GTu`: empty
`prompt` fulfills with `undefined` instead of rejecting with
`Cannot send an empty follow-up message`. Main bundles also contain **zero**
`send-follow-up-message` strings. See
[bridge vs Pets handler](../research/codex-bridge-vs-pets-handler-2026-07-26.md).

Do not ship Ready follow-up on Strategy 1 alone.

### Strategy 2 — In-renderer Pets handler (`Rf` / `GTu`) — **required Channel B path**

Dispatch must call the discovered `Rf` export so `_Ze` forwards into `GTu[type]`.

Live on App `26.721.41059` (production profile, user-authorized foreground):

- empty-prompt fingerprint via `Rf` ✓
- Ready follow-up UI turn ✓
- active steer via `steer-turn-for-host` → `{ turnId }` ✓

Evidence: [CDP Rf handler live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md).

Cost: minified export names and `app-initial-*.js` hashes drift per App build; rediscover by guarded
function-source fingerprint + exact controlled no-content rejection on each update. Do not hard-code
`ddt`.

### Strategy 3 — IPC-only (default / current product)

Unchanged exact-owner follower start-turn / steer. Used when CDP mode is off or degraded. A Channel B
send failure fails closed and does not automatically retry through IPC.

## Trust boundary

- CDP endpoint: process-lifetime only. It is either CoPets-launched or a user-clicked explicit
  existing local connection. A CoPets launch is a macOS Launch Services handoff followed by exactly
  one same-user official-process rediscovery; the handoff helper is never the endpoint identity.
  Automatic existing-App discovery accepts exactly one same-user official App process; custom mode
  accepts only its matching IPv4 loopback listener. A stored preference never exposes or restores a
  runtime endpoint. CoPets tracks the exact App PID and requires it to own the listener at readiness
  and before every send, so a port race, helper-only FD, or PID/listener mismatch fails closed rather
  than becoming an arbitrary attach. A retry reuses only the native retained endpoint and it is
  discarded on command-liveness loss or external listener loss.
- Chromium's loopback DevTools protocol has no per-client authentication. It remains an opt-in,
  same-user debugging surface, not a boundary against hostile code already running as that macOS
  user. Use it only in a trusted local user session; CoPets never advertises it as official or safe
  for multi-user exposure.
- No Network/Fetch domain subscription in product mode.
- Envelope construction stays in Rust; the pet WebView never receives raw `conversationId` /
  `hostId` / bridge envelopes.
- Diagnostic logs may record type string, success/failure class, and hashed conversation id—never
  prompt text or full envelopes.
- `Rf` fingerprint: guarded function-source match + one recognized controlled no-content rejection
  before readiness and before each send. The probe uses only the fixed non-existent sentinel ID and
  an empty prompt; it accepts neither fulfilment nor an unknown error. Eligible Codex renderer pages
  are bounded, main-window-preferred, and probed concurrently. Existing-endpoint readiness retries
  short probe attempts inside one hard deadline so a transient page/socket miss cannot consume the
  full budget; a cold avatar overlay cannot indefinitely delay the ready page. Fingerprint miss →
  `CdpDegraded`; a later send failure is fail-closed and never falls back to another transport.
- Do not enable CDP for notarization/signing claims; document Gatekeeper / debugging risk in the
  user-facing opt-in copy.

## UI / product behavior

1. Settings offers **Launch Codex**, **Restart Codex with bridge**, and **Connect existing** inside
   one compact experimental disclosure. Restart appears only in standard `IpcOnly` state and opens a
   destructive-action confirmation before it can call native code; a degraded tracked endpoint keeps
   restart hidden and exposes retry instead. Automatic Connect accepts one discovered same-user
   official loopback-CDP App; custom Connect validates only the user-entered port. It never accepts
   an arbitrary DevTools URL or host.
2. While `CdpReady`, Continue on a selected Ready task remains visible through IPC owner reconnect
   (presentation), and **Send** uses Channel B (`Rf` Ready).
3. While `CdpReady` and selected lifecycle is `working`, **Steer Send** may use Channel B
   (`steer-turn-for-host`) without requiring a fresh IPC follower owner.
4. While `IpcOnly` or `CdpDegraded`, Continue / Steer keep today's IPC exact-owner authorization.
5. Stop / Approvals never switch to CDP in v1.
6. Copy must state: standard launch requires the user to quit Codex first and asks macOS to open the
   official App through Launch Services; restart closes and reopens one normal Codex only after
   confirmation; connecting an already CDP-enabled App does not restart it. All paths use a local
   debugging port and are not an official OpenAI interface. Copy must not promise how a macOS
   permission prompt will be labelled.
7. When an initial readiness check degrades, Settings may offer **Retry verification**. It rechecks
   only the same native tracked endpoint, never sends a follow-up, starts no process, and accepts no
   runtime endpoint from the WebView.
8. A launch, restart, connect, or retry command must settle after its bounded native check. The UI
   must not depend on an unbounded DevTools request to clear its in-progress state.

## Module impact

| Module | Change |
| --- | --- |
| Native launcher / `lib.rs` | Launch Services handoff, explicit official-process rediscovery, user-confirmed exact-PID graceful restart, loopback port bookkeeping, and PID/listener proof |
| `observer/runtime.rs` | `ControlTransport`, native-only tracked endpoint provenance/PID, transport generation; no second selection authority |
| `observer/commands.rs` | `dispatch_ready_follow_up` / `dispatch_steering` may select Channel B when `CdpReady`; launch, confirmed restart, connect, and retry are ownership-checked before `Rf` verification |
| `control.rs` | `build_cdp_ready_follow_up` + `build_cdp_steer` params from `ControlTarget` |
| New `cdp` adapter (Rust CDP client) | Attach, resolve `app-initial`, fingerprint `Rf`, evaluate send, map errors |
| `PetWindow.svelte` / follow-up visibility | Gate Send on transport + existing Ready/Steer predicates |
| Tests | Envelope inheritance fixtures; fingerprint fail-closed; no live prompt fixtures in CI |

## Acceptance gates

### Gate C0a — CoPets launch + `Rf` fingerprint

- Launch Services opens the unmodified official App on loopback CDP with production-profile paths;
  CoPets independently rediscovers one exact same-user official PID. The helper PID is never
  accepted, and the rediscovered PID owns the listener before readiness and before a send.
- Page target appears; `electronBridge` readable; **guarded `Rf` source fingerprint + recognized
  controlled no-content reject**.
- Fingerprint or PID/listener proof failure leaves transport degraded; IPC path unaffected.
- A stalled CDP request is bounded by the readiness deadline, returns a degraded result, and cannot
  leave the launch affordance pending indefinitely.
- A post-timeout retry succeeds only for the same native retained endpoint and never becomes a
  generic CDP attach.
- For a launch-handoff change, perform a cold manual A/B observation of any relevant macOS
  permission prompt. Record its actual label as observed evidence, or record that no prompt
  appeared; neither result can be inferred from process ancestry alone.

### Gate C0r — User-confirmed normal-App restart

- The Settings action shows a clear cancellation-capable confirmation before native restart begins;
  cancelling leaves the App untouched.
- Native selection accepts exactly one same-user official non-CDP process, revalidates it before
  `SIGTERM`, and rejects zero, multiple, stale, or already-CDP candidates without signaling any App.
- The old PID must exit within the bounded wait before the ordinary C0a launcher begins. A timeout or
  launch failure sends no force-kill and no hidden normal-App fallback.
- Product-path manual verification on the pinned App must prove the old App is replaced once, the new
  App reaches `CdpReady`, and quitting CoPets afterwards leaves that new App running.

### Gate C0e — Existing local endpoint attach

- With Codex already launched using a loopback CDP port, automatic Connect succeeds only when one
  same-user official App candidate exists; custom Connect succeeds only for its exact port.
- A transient renderer or DevTools miss can be retried only within Connect's one bounded native
  deadline; external liveness monitoring starts only after the endpoint is `CdpReady`.
- A helper-inherited listener, wrong executable/UID, wildcard/IPv6 listener, zero/multiple automatic
  candidates, closed port, or `Rf` mismatch leaves Channel B degraded or off.
- Listener loss after Ready clears Channel B, and a later Ready/Steer is withheld rather than sent to
  a reused port.

### Gate C1 — Envelope inheritance (automated)

Fixtures prove:

- selected conversation id is copied exactly into Ready and Steer params;
- foreign host id cannot appear;
- missing cwd blocks Ready and Steer;
- empty prompt blocked before CDP evaluate;
- Ready uses `prompt`; Steer uses `input` + `restoreMessage` (no cross-shape mix-up);
- `serviceTier` null vs inherited value both serialize correctly;
- WebView command args cannot supply raw conversation/host ids.

### Gate C2 — Selected-Ready live send (manual, real profile)

Research probe already passed once on `26.721.41059`
([Rf live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md)). Product C2 still required
through the CoPets UI path:

1. Complete a turn to Ready / CoPets `completed`.
2. With Channel B up, submit Continue from CoPets.
3. Codex shows the new user turn on **that** conversation only.
4. Repeat with IPC owner intentionally stale; Channel B still targets the same conversation.
5. Record a dated evidence snapshot (hashes, App version, pass/fail)—no prompts or raw ids.

### Gate C2b — Selected working live steer (manual, real profile)

Research probe passed once on the same App build. Product C2b through CoPets UI:

1. Select a `working` task with an active turn in the verified CDP App.
2. Submit Steer from CoPets while IPC owner is stale or fresh.
3. Codex applies steer to **that** conversation only; response reflects the steer text.
4. Record dated evidence (no prompts or raw ids).

### Gate C3 — Negative cases

- Cold isolated profile: Channel B send must refuse or no-op with clear error.
- Wrong/opaque task id: no send.
- `Rf` / `app-initial` renamed after App update: degrade to IPC-only Continue/Steer rules.
- Strategy 1 bridge call must never be used as a silent fallback after `Rf` failure.

Shipping Channel B Ready + Steer as non-experimental requires C0+C1+C2+C2b on a pinned App build.

## Rollout

1. Land ADR + this spec.
2. Implement CDP attach, `Rf` fingerprint, and C1 fixtures without enabling UI.
3. Behind an explicit settings flag, enable Continue → Channel B, then Steer → Channel B.
4. Keep feature catalog status **Experimental** until C2+C2b pass on the current App via CoPets UI.
5. On App update: re-run C0 fingerprint, C2, and C2b; refresh research snapshots rather than silently
   trusting old export names.

## Open questions

1. ~~Does `sendMessageFromView` equal Pets/`GTu`?~~ **Answered 2026-07-26: no.** See
   [bridge vs Pets handler](../research/codex-bridge-vs-pets-handler-2026-07-26.md).
2. ~~Can CDP invoke in-renderer `Rf`/`GTu` without patching?~~ **Answered 2026-07-26: yes** for
   Ready + Steer on App `26.721.41059`. See
   [CDP Rf handler live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md).
3. ~~Should CoPets offer “restart Codex with bridge” from the stale-owner Continue error, or only
   from Settings?~~ **Answered 2026-07-26: Settings-only.** It is an explicit, confirmed process
   lifecycle action and never a send-error fallback.
4. ~~Should active-turn steer gain a CDP path?~~ **In scope for v1 Channel B** after C2b; IPC remains
   fallback when not `CdpReady`.

## References

- [Runtime architecture](runtime.md)
- [Multi-session arbitration](multi-session-state.md)
- [Feature catalog](../features/catalog.md)
- [CDP electronBridge probe](../research/codex-cdp-electron-bridge-2026-07-26.md)
- [message-from-view static contract](../research/codex-message-from-view-static-2026-07-26.md)
- [Bridge vs Pets handler](../research/codex-bridge-vs-pets-handler-2026-07-26.md)
- [CDP Rf handler live gate](../research/codex-cdp-rf-handler-live-2026-07-26.md)
- [Existing local CDP attachment](../research/codex-existing-cdp-attach-live-2026-07-26.md)
- [Launch Services handoff assessment](../research/launch-services-cdp-handoff-2026-07-26.md)
- [ADR 0004 — Retire Resume Lab](../decisions/0004-retire-codex-resume-lab.md)
- [ADR 0006 — Explicit existing CDP attachment](../decisions/0006-explicit-existing-cdp-attach.md)
- [ADR 0007 — User-confirmed CDP restart](../decisions/0007-user-confirmed-cdp-restart.md)
- [ADR 0008 — Launch Services CDP handoff](../decisions/0008-launch-services-cdp-handoff.md)
