# Pi extension integration

> Status: Research snapshot
> Owns: Dated evidence about Pi coding-agent extension discovery, API, events, controls, and process integration
> Update when: Pi version, extension loader/API, package installation, or RPC/control behavior changes
> Last verified: 2026-07-20 (local `pi` 0.80.3; official repository checked same date)

## Scope and source order

This snapshot records local installed evidence plus official Pi sources. Local executable is `~/.bun/bin/pi`, symlinked to `@earendil-works/pi-coding-agent/dist/cli.js`; `pi --version` returned `0.80.3`. Package metadata declares Node `>=22.19.0`, package version `0.80.3`, repository `https://github.com/earendil-works/pi`, and exports extension types through `dist/index.d.ts` (`package.json`, `engines`, `exports`).

Official sources: [extensions guide](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md), [ExtensionAPI types](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts), [package manager guide](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md). Local counterparts are cited for the exact installed version.

## Observed facts

### Discovery and installation

- Auto-discovery scans `~/.pi/agent/extensions/*.ts`, `~/.pi/agent/extensions/*/index.ts`, project `.pi/extensions/*.ts`, and `.pi/extensions/*/index.ts`; project-local entries load only after project trust. `pi -e/--extension path` is intended for quick tests. Official guide: [Extension Locations](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#extension-locations); local `docs/extensions.md:109-145`.
- `settings.json` accepts `packages` (`npm:` or `git:` specs) and `extensions` (local file/directory paths). Package extensions can declare `pi.extensions` in `package.json`; production installs omit `devDependencies`, so runtime dependencies belong in `dependencies` (official guide, same [section](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#extension-locations)).
- Extensions are TypeScript modules loaded via `jiti`; default export is sync or async factory receiving `ExtensionAPI`. Async factory completes before `session_start`, `resources_discover`, and queued provider registrations flush (`docs/extensions.md:148-220`).
- Official guide warns extensions run with full system permissions and arbitrary code; install only trusted sources ([security note](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#extension-locations)).

### ExtensionAPI and observable events

Installed `dist/core/extensions/types.d.ts:822-870` defines `ExtensionAPI.on()` for:

- lifecycle/session: `project_trust`, `resources_discover`, `session_start`, `session_info_changed`, `session_before_switch`, `session_before_fork`, `session_before_compact`, `session_compact`, `session_shutdown`, `session_before_tree`, `session_tree`;
- agent/turn/message: `before_agent_start`, `agent_start`, `agent_end`, `turn_start`, `turn_end`, `message_start`, `message_update`, `message_end`;
- tool/model/input: `tool_execution_start/update/end`, `tool_call`, `tool_result`, `model_select`, `thinking_level_select`, `user_bash`, `input`, plus `context`, `before_provider_request`, `after_provider_response`.

These names are also enumerated by official `ExtensionAPI` source ([types.ts](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/extensions/types.ts#L748-L870)). Event handlers may return typed results for interception events; e.g. `tool_call` can return `{ block: true, reason }`, and `input` can transform/handle input (guide [Tool events](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#tool-events), [Input events](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#input-events)).

### Registration and outward control

Installed API declares `registerTool`, `registerCommand`, `registerShortcut`, `registerFlag`, `registerMessageRenderer`, `sendMessage`, `sendUserMessage`, `appendEntry`, session naming/labels, `exec`, active-tool management, model/thinking-level operations (`types.d.ts:848-900`; official [API methods](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#extensionapi-methods)).

- Custom tools are callable by LLM; custom commands are slash commands. `sendUserMessage` always triggers a turn; `sendMessage` can choose `steer`, `followUp`, or `nextTurn` delivery (official [methods](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#pi-sendusermessagecontent-options)).
- `pi.exec(command, args, options?)` executes a child process and returns stdout/stderr/code/killed. This is an explicit supported path, but extension code still has host process privileges (local `types.d.ts:888-900`; official [pi.exec](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#piexeccommand-args-options)).
- `ctx.ui` supports prompts, confirm/input/select, notify, status/widget, custom TUI components; availability depends on mode (`ctx.hasUI`). RPC mode is separate and emits JSON over stdin/stdout (official [Custom UI](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#custom-ui)).

### Sockets, HTTP, timers, and shutdown

Node built-ins and npm dependencies are available to extensions (`docs/extensions.md:146-180`), so ordinary Node `net`, `http`, `fetch`, timers, and child-process APIs are technically usable. Official guidance says **do not start background processes, sockets, file watchers, or timers in the factory** because a factory may run without a session; defer startup to `session_start` or the event/command/tool needing it, and register idempotent `session_shutdown` cleanup (`docs/extensions.md:219-223`; official [long-lived resources](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#long-lived-resources-and-shutdown)).

This supports a local sidecar socket/HTTP bridge by design inference, not a Pi-specific transport contract: bind narrowly (localhost/Unix socket), authenticate/authorize requests, bound payloads, handle disconnects, and close on every `session_shutdown`/reload. No official source promises a stable extension-to-external control protocol.

### Unload, reload, and compatibility

- `/reload` hot-reloads auto-discovered extensions. `ctx.reload()` emits `session_shutdown`, tears down runtime, reloads/rebinds extensions, then emits `session_start`; session switch/fork similarly shuts down old instance and starts a new one (official [reload](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#ctxreload), [session replacement](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md#session-replacement-lifecycle-and-footguns)).
- `session_shutdown` cleanup must be idempotent. In-memory state must be re-established at `session_start`; old extension closures must not assume they survive replacement.
- No tested semantic-version compatibility promise found. Local package pins `0.80.3`; extension imports should use exported `ExtensionAPI` types and feature-detect/guard optional behavior. **Not verified:** package install/update CLI exact commands, behavior across Pi versions, or whether a running external process can safely control an already-running Pi instance through extension APIs.

## Design implications for DeskPal

**Observation:** Pi extension hooks expose rich per-session/turn/tool/model/input lifecycle, and `sendUserMessage`/`sendMessage` provide in-process outward control. **Inference:** A Pi adapter could emit bounded snapshots to DeskPal and route explicit user actions back through a trusted extension, while preserving DeskPal invariants (selected task only, opaque IDs, no raw payloads in WebView).

**Boundary:** Extension arbitrary-code/full-permission warning means installation is equivalent to granting host access. DeskPal must treat extension as a privileged local component, require explicit opt-in, avoid accepting unauthenticated remote connections, and use shutdown/reload hooks to prevent stale owners.

## Verification gaps

Not covered by local execution: running a sample extension against a live Pi session; proving every event's runtime payload shape; testing package discovery/trust prompts; testing localhost socket/HTTP lifecycle; validating control delivery under session switch/reload; or proving compatibility beyond `0.80.3`.
