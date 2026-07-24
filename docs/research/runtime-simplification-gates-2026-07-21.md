# Runtime simplification gates: 2026-07-21

> Status: Evidence snapshot
> Owns: Revision-pinned P8 evidence for stale-owner recovery, high-resolution atlas transport, and current private Codex behavior
> Update when: Preserve this result; create a new dated snapshot for another app build, product revision, or measurement method
> Last verified: 2026-07-21

This snapshot closes the evidence gate in the [architecture repair plan](../maintenance/repair-plan.md).
It records decisions, including decisions to retain existing defenses. It is not a compatibility
promise for future Codex App builds.

## Tested environment

| Item | Value |
| --- | --- |
| DeskPal source | `c7d86439e45a9ee8d75a332f6a3f3a0e4f3717e7` |
| Codex App bundle | `/Applications/ChatGPT.app`, `com.openai.codex` |
| App version / build | `26.715.52143` / `5591` |
| Embedded CLI | `codex-cli 0.145.0-alpha.27` |
| macOS | `26.5.2` (`25F84`), arm64 |
| Node.js | `v24.14.0` |

The atlas timing build added a temporary measurement-only hook around the existing
`load_pet -> PixiPet.load -> texture commit` path. The hook did not change validation, transport,
decode, texture construction, cleanup, selection, or preview behavior and was removed before the
evidence commit. No raw prompt, answer, task identifier, private payload, or local probe log is
committed.

## Decision 1: retain stale-owner recovery

The focused contract gate passed four tests:

- stale router errors are recognized;
- a refresh snapshot rejects the stale owner and wrong host;
- recovery broadcasts a following refresh and retries only the replacement owner; and
- a foreground-selection change cancels the retry.

The real owner-replacement experiment was not run. The production recovery path emits a
`thread-stream-following-changed` broadcast and then sends steering when recovery succeeds. That
changes live Codex conversation state and requires an explicit user control action. Passive
observation cannot exercise the same path.

Decision: keep stale-owner classification, refresh broadcast, bounded polling, same conversation
and host checks, different-owner check, exact selected-task revalidation, post-await identity
checks, and the follow-up inflight guard. Mock tests prove the control contract, not current macOS
router recovery. This result does not authorize any simplification.

Focused command:

```bash
cargo test --manifest-path src-tauri/Cargo.toml stale_owner -- --nocapture
```

## Decision 2: keep data URLs and keep 4x opt-in

### Targets fixed before the run

- Data-URL payload: at most 20 MiB.
- Initial or reload end-to-end p95: at most 1,000 ms.
- Native-process peak RSS: at most 256 MiB.
- WebContent peak RSS: at most 256 MiB.
- Preview, ten reloads, and restoration: all operations must commit successfully.

The transport targets decide whether an asset protocol is justified. The renderer-memory target
detects whether the atlas itself is practical; crossing it does not by itself attribute memory to
base64 transport.

### Method

Each source atlas was copied to an isolated `mktemp` `CODEX_HOME`, and only the copied manifest's
legacy-named `sidecarSpritesheetPath` was changed. One ad-hoc-signed DeskPal process was launched
per scale. `performance.now()` measured native invocation and WebView decode/texture commit. Ten
reloads, one `preview_pet_import`, and one restoration load ran in sequence. `ps -o rss` sampled
the exact DeskPal PID and newly created `com.apple.WebKit.WebContent` PID every 50 ms.

| Atlas | Payload | Initial | Reload p50 / p95 / max | Preview / restore | Native peak RSS | WebContent peak / final RSS | Result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1x, 1536x1872 | 1.77 MiB | 40 ms | 7 / 8 / 8 ms | 7 / 15 ms | 121.0 MiB | 122.5 / 122.5 MiB | 13/13 committed |
| 2x, 3072x3744 | 5.22 MiB | 125 ms | 21 / 23 / 23 ms | 21 / 21 ms | 125.0 MiB | 293.5 / 293.5 MiB | 13/13 committed |
| 4x, 6144x7488 | 15.46 MiB | 448 ms | 60 / 485 / 485 ms | 54 / 58 ms | 164.1 MiB | 1007.5 / 769.4 MiB | 13/13 committed |

The 4x WebP is 12,154,928 bytes. Its exact returned pet payload was 16,206,916 bytes, and one
decoded RGBA image is 184,025,088 bytes (175.5 MiB) before GPU texture accounting. The 4x payload
and timing remained below their transport targets. WebContent memory crossed the renderer target
by a wide margin, while native RSS remained below its target.

Decision: do not add an asset protocol. The measurement does not attribute the dominant memory to
the 15.46 MiB data URL; the decoded atlas and texture are much larger, and replacing the URL cannot
remove them. Keep the current 2x native atlas as the package default and retain 4x protocol support
as opt-in compatibility. A future attempt to make 4x the default must first test a renderer-level
strategy such as bounded/tiled textures, not merely another byte transport.

Representative commands:

```bash
sips -g pixelWidth -g pixelHeight spritesheet-4x.webp
stat -f %z spritesheet-4x.webp
npm run tauri -- build --bundles app
ps -o rss= -p <exact-test-pid>
```

### Measurement limits

- RSS is process-resident memory, not an exact split of JavaScript heap, decoded image memory, and
  GPU allocation.
- This is one machine and one run per scale; the ten reloads characterize the path but do not form
  a cross-machine benchmark.
- Startup samples are not a stable pre-load baseline, so the final RSS values do not prove a leak.
- No asset-protocol comparison was run, so transport-specific memory savings remain unknown.

## Decision 3: retain review mode; no JSONL selection fallback exists

Read-only probes against the installed App produced:

- a successful IPC initialize and a live following broadcast;
- a session watcher over 28 recent files plus new allowlisted session events; and
- app-log activity facts filtered through the known-thread index.

During the same run, IPC and the newest foreground app-log fact agreed on one opaque conversation,
while live JSONL activity arrived for a different opaque thread. This corroborates the separation
between foreground selection and background task updates without retaining raw identifiers.

The current `app.asar` still contains `entered_review_mode` and `exited_review_mode` handlers. A
sanitized scan of parsed session events found no current-build runtime sample: the newest actual
enter/exit pair was from 2026-06-06, before this App build. Therefore current review-mode runtime
behavior remains inconclusive and the `reviewing` lifecycle mapping stays.

The source audit disproved the second P8 hypothesis. [`SessionAdapter`](../../src-tauri/src/observer/session.rs)
emits per-thread lifecycle/context events but never selects a thread. Only
[`AppLogSelectionAdapter`](../../src-tauri/src/observer/selection.rs), after known-thread index
validation, emits `RuntimeEvent::Select`. There is no JSONL fallback-selection path left to remove
or preserve. If foreground app-log selection is unavailable, selection fails closed instead of
guessing from the most recently active JSONL file.

Probe commands:

```bash
npm run probe:ipc
npm run probe:sessions
npm run probe:logs
/Applications/ChatGPT.app/Contents/Resources/codex --version
mdls -name kMDItemVersion -name kMDItemCFBundleIdentifier /Applications/ChatGPT.app
```

### Remaining uncertainty

- Review-mode enter/exit was not triggered on App `26.715.52143`.
- The passive probe did not exercise an IPC disconnect/reconnect or a terminal transition.
- Private app-log, IPC, and JSONL formats remain version-sensitive and must fail closed on drift.

## Gate result

P8 is complete as an evidence and decision gate. It removes no live safety defense and adds no
speculative transport. Re-run this snapshot for a future Codex App build, a renderer transport
change, a proposed 4x default, or a user-authorized stale-owner recovery experiment.
