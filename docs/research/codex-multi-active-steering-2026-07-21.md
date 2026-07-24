# Codex multi-active Pets and per-conversation steering: 2026-07-21

> Status: Research snapshot
> Owns: 2026-07-21 official multi-chat Pets behavior and installed-App static steering evidence; not current DeskPal support
> Update when: Preserve this snapshot; create a new dated snapshot for another App build or live experiment
> Last verified: 2026-07-21

## Scope

This snapshot checks whether the currently installed Codex desktop App has a multi-conversation
runtime and a per-conversation steering path suitable for the proposed
[DeskPal feature](../architecture/multi-active-preview-steering.md).

It combines current official product documentation, installed-bundle static inspection, and a
DeskPal code audit. It does not claim that DeskPal has implemented the feature or that background
steering passed a live end-to-end run.

No prompt, answer, command output, raw task identifier, owner identifier, host identifier, private
payload, or local log is recorded.

## Environment

| Item | Value |
| --- | --- |
| DeskPal revision | `0d91bedd431011c6445bf705c98321c9f02a7a02` |
| App bundle | `/Applications/ChatGPT.app`, `com.openai.codex` |
| App version / build | `26.715.61943` / `5628` |
| Embedded CLI | `codex-cli 0.145.0-alpha.27` |
| macOS | `26.5.2` (`25F84`), arm64 |
| Node.js | `v24.14.0` |

## Official product behavior

The current [official Pets documentation](https://developers.openai.com/codex/pets?surface=app)
states that the desktop pet follows activity across chats, prioritizes needs input, blocked, ready,
then running, and exposes an activity tray for choosing a chat.

The current [App settings documentation](https://developers.openai.com/codex/app/settings) states
that follow-up behavior can steer the current run or wait for the next run.

These sources confirm product semantics. They do not document the private desktop IPC method or
authorize third-party compatibility claims.

## Installed-bundle static evidence

Bounded string inspection of `Contents/Resources/app.asar` found one definition each of:

- `streamingConversations=new Set`;
- `followedConversationIds=new Set`;
- `followerClientIdsByConversationId=new Map`.

The same build contains `streamRoles`, `isConversationStreaming`, per-conversation following state,
and broadcasts carrying `conversationId` and `hostId`. `thread-follower-steer-turn` occurs six times
in the bundle.

A bounded code window around the steering path shows that it sends a follower request containing
the selected `conversationId`, then verifies that conversation's stream role is `owner` and that the
conversation is streaming. A separate bounded code window shows the streaming/following/role
collections are keyed by conversation.

This is strong static evidence that the App runtime can track several conversations and route a
steer request by conversation. It is not a live proof that an independent DeskPal follower can
enumerate every active conversation or successfully steer a background task on this build.

## DeskPal codebase findings

Confirmed from current source:

- [`RuntimeState.threads`](../../src-tauri/src/observer/runtime.rs) already stores independent
  lifecycle, context, control, and refresh state by opaque task key.
- [`ipc.rs`](../../src-tauri/src/observer/ipc.rs) accepts state snapshots for any broadcast
  conversation and stores raw conversation, owner, host, and workspace only in native memory.
- [`RuntimeSnapshot`](../../src-tauri/src/observer/runtime.rs) and `ControlSnapshot` project only
  the selected task.
- [`send_follow_up`](../../src-tauri/src/observer/commands.rs) refreshes Codex foreground selection,
  authorizes only that selected live task, and dispatches `thread-follower-steer-turn` to its exact
  owner.
- Stale-owner recovery already refreshes one conversation/host and requires a different replacement
  owner, but its wait path is coupled to `state.selected`.
- The runtime has no activity timestamp/revision, unread/seen projection, active-list event, or
  thread-record pruning.
- [`PetWindow.svelte`](../../ui/PetWindow.svelte) has one selected runtime/control model and closes
  its current follow-up composer when selected `canReply` becomes false.

Therefore the main gap is not private wire-method discovery. It is the safe projection and
authorization layer between the existing per-task native map and the renderer.

## Confidence table

| Claim | Status | Evidence |
| --- | --- | --- |
| Official desktop Pets exposes multi-chat activity tray and priority | Confirmed current documentation | Official Pets page, accessed 2026-07-21 |
| Installed App stores streaming/following/role state per conversation | Confirmed static | Bounded `app.asar` inspection on build 5628 |
| Installed App has per-conversation follower steering path | Confirmed static | `thread-follower-steer-turn` path plus owner/streaming checks |
| DeskPal reduces background conversations independently | Confirmed source and tests | Runtime reducer and observer tests |
| DeskPal can passively receive every active conversation on build 5628 | Unconfirmed | No current-build multi-task live capture in this snapshot |
| DeskPal can steer a non-selected active conversation on build 5628 | Unconfirmed | Existing command rejects non-selected target; no targeted prototype run |
| Connect-time enumeration/follow-all is safe and stable | Unknown | Static collections exist; callable third-party behavior not probed |
| Remote/cloud conversations share sufficient local owner routing | Unknown | Not probed; current product scope stays local |

## Reproduction commands

Metadata and bounded counts:

```bash
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' /Applications/ChatGPT.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Applications/ChatGPT.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' /Applications/ChatGPT.app/Contents/Info.plist
/Applications/ChatGPT.app/Contents/Resources/codex --version
rg -a -o -F 'thread-follower-steer-turn' /Applications/ChatGPT.app/Contents/Resources/app.asar
rg -a -o -F 'streamingConversations=new Set' /Applications/ChatGPT.app/Contents/Resources/app.asar
rg -a -o -F 'followedConversationIds=new Set' /Applications/ChatGPT.app/Contents/Resources/app.asar
```

The static inspection deliberately prints only known method/field names and bounded code windows.
Do not commit full extracted bundle code or local runtime payloads.

## Required live gate before implementation is called compatible

1. Attach DeskPal diagnostic IPC to App build 5628 or later and capture sanitized hashes for two
   simultaneous local conversations.
2. Confirm both conversations produce independent state snapshots or document which one requires an
   explicit following refresh.
3. Add a temporary targeted test command behind the exact native authorization path.
4. Keep task A selected while steering B; verify only B changes and Codex App does not activate or
   switch.
5. Repeat the mandatory gate with B terminal, IPC disconnect/reconnect, and foreground selection
   changing during send or recovery.
6. If the App build exposes a safe reproducible owner-replacement path, exercise it; otherwise keep
   mocked replacement coverage and record the live limitation rather than fabricating success.
7. Record method/version fingerprint, App/CLI versions, DeskPal revision, results, and uncertainty in
   a new immutable evidence snapshot.

Until that gate passes, the proposed feature remains Experimental and targeted steering must fail
closed when the exact owner is unavailable.
