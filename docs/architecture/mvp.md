# CoPets MVP

> Status: Normative
> Owns: Product boundary, stable pet state, MVP compatibility, and acceptance criteria
> Update when: MVP scope, exposed state vocabulary, or acceptance criteria changes
> Last verified: 2026-07-24

Multi-session selection, lifecycle reduction, and control routing are specified in
[`multi-session-state.md`](./multi-session-state.md).

## Product boundary

The app is an interactive macOS desktop companion. It attaches after Codex is already running and never proxies normal model traffic. It submits approval, input, follow-up, or stop requests only after an explicit user action. It reads only the newest 2 MB of active session logs and keeps bounded, in-memory previews of the selected task's user-visible progress. User-question context is best-effort when it remains inside that tail; the app never backscans or stores full prompt/output content.

## Runtime

```text
Codex / ChatGPT App
  ├─ user-private IPC snapshots <─┐
  │  explicit follower controls ─>│
  ├─ session JSONL append stream ─┼─> Rust observer + state reducer
  ├─ activity logs ───────────────┤        │
  └─ state_5.sqlite thread index ─┘        ├─> Tauri event channel
                                           └─> Svelte + PixiJS pet stage
```

The Rust backend owns filesystem, socket access, owner routing, raw request IDs, and permission payloads. The WebView receives a stable pet state, opaque action/question IDs, compact request summaries, connection health, and validated pet assets.

## Stable pet state

`idle | working | reviewing | completed | failed | interrupted | disconnected`

Source-specific events are reduced per thread. The GUI activity signal selects which thread drives the visible pet. Unknown private-schema events do not escape to the renderer.

## Pet compatibility

- Codex V1: `1536x1872`, 8x9, 192x208 cells, missing `spriteVersionNumber`.
- Codex V2: `1536x2288`, 8x11, 192x208 cells, `spriteVersionNumber: 2`.
- CoPets extension: integer-scaled atlases with the same 192:208 cell aspect and 8-column row contract are accepted for high-resolution rendering. The legacy `sidecarSpritesheetPath` field name remains stable for package compatibility. Exporting back to Codex/Pet Creator remains fixed-size V1/V2.
- Package discovery follows `${CODEX_HOME:-~/.codex}/pets/<id>/pet.json` with a relative `spritesheetPath` confined to that pet folder.

## Window behavior

- Transparent, borderless, always-on-top, all workspaces, hidden from Dock/taskbar.
- PixiJS renders at `devicePixelRatio` with `autoDensity`.
- The tray controls show/hide and quit. The always-on-top pet remains interactive and never enters click-through mode.
- Dragging the pet body calls the native window drag API. The inactive window accepts the first mouse press, so click-drag works without a separate focus click. A round grip appears beside the pet's right foot on hover and performs proportional manual resizing because native corner resize is unavailable on macOS.
- Physical position and size persist locally; restore clamps the complete window rectangle inside the best matching attached monitor.
- Completion, failure, and interruption animations play once, hold briefly, then the presentation returns to idle and clears its speech bubbles. The observer retains the factual terminal state.
- Reduced-motion holds the first state frame and avoids state-loop animation.

## MVP acceptance

1. Launches as a Tauri 2 macOS app without restarting Codex.
2. Imports a compatible folder, selected `pet.json`, or ZIP through the settings GUI and renders it sharply on Retina displays.
3. Tracks current visible Codex thread and changes animation for work, review, completion, failure, interruption, and disconnect. Pending questions and approvals remain work plus explicit control cards.
4. Explicit controls cover local command/network/file/permission approvals, questions, tool confirmations, follow-up, and stop.
5. The WebView receives only bounded previews for the selected task: user-visible `agent_message` progress plus best-effort question context when present in the newest 2 MB. Full prompt/answer bodies, hidden reasoning, tool arguments, command output, unrelated snapshot history, and raw thread/request IDs never reach it.
6. Unknown Codex versions degrade to JSONL or disconnected state instead of inventing completion.
