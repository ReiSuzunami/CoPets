# Codex Ready follow-up compatibility: 2026-07-25

> Status: Research snapshot
> Owns: Installed Codex App 26.721.41059 static Ready-follow-up evidence and selected-Ready live-gate status
> Update when: Preserve this snapshot; create a later dated verification for an App update or selected-Ready send result
> Last verified: 2026-07-25

## Scope

This snapshot examines whether the installed Codex desktop App exposes a private follower path for
an explicit follow-up after a conversation is Ready. It combines bounded installed-bundle static
inspection, a CoPets source audit, and one local active-turn UI attempt. It does not claim that a
Ready follow-up has passed end-to-end on this App build.

No prompt, answer, command output, raw conversation identifier, owner identifier, host identifier,
or private request payload is recorded.

## Environment

| Item | Value |
| --- | --- |
| CoPets revision | `0eb326243fa3` before this change |
| App bundle | `/Applications/ChatGPT.app`, `com.openai.codex` |
| App version / build | `26.721.41059` / `5848` |
| Embedded CLI | `codex-cli 0.146.0-alpha.3.1` |
| macOS | `26.5.2` (`25F84`), arm64 |
| Node.js | `v24.14.0` |

## Installed-bundle static evidence

Bounded string inspection of `Contents/Resources/app.asar` found six occurrences of
`thread-follower-start-turn`, two occurrences of its host-forwarding handler name, and six
occurrences of `thread-follower-steer-turn`.

A bounded code window around the start path shows that the App routes a follower start request to
the host service, binds it to the conversation's host, checks the exact follower owner, and passes
a nested turn-start object to the existing turn-start implementation. This is static evidence that
the current App has a distinct follower start path; it is not a third-party delivery proof.

The same static inspection shows that a duplicate `following: true` refresh for an already-followed
conversation can emit a targeted current state snapshot to the existing follower. That snapshot can
retain the existing owner identity, so owner replacement alone is not a valid freshness test. This
is current-App static evidence, not selected-Ready delivery proof.

The App also keeps per-conversation following registrations, follower-client sets, stream roles, and
stream revisions. When a new owner needs follower status it requests it, and a matching
`following: true` registration causes a state snapshot. This is subscription/liveness state, not a
transcript cache. Complete-history loading is a separate follower operation.

## CoPets implementation decision

`state_from_conversation` maps an idle App runtime to CoPets `completed`. CoPets therefore uses
selected terminal `completed` as its Ready projection rather than introducing an unobserved
lifecycle state.

[`authorize_follow_up`](../../src-tauri/src/observer/runtime.rs) chooses one mode before dispatch:

- selected live `working` task: existing steering request;
- selected terminal `completed` task: follower start-turn request.

Both require connected IPC and an exact non-stale owner. Stale-owner recovery preserves the chosen
mode and rechecks selection, conversation, host, and lifecycle. CoPets marks a target pending,
then accepts a same-owner state snapshot only after its explicit follow refresh was written to the
IPC stream and the snapshot matches the selected conversation and host. A failed steer cannot turn
into a start request, and a failed Ready start cannot route to another task.

The same installed-App bundle shows that a duplicate `following: true` from an already registered
follower sends that follower a current snapshot when the App still has the owner role. Therefore an
explicit user retry from a stale selected target must repeat the exact conversation/host follow
registration before any follower request. It must not treat the stale owner as authorized or invoke
the App's private resume operation.

CoPets also retains each observed task's conversation/host follow registration in native process
memory. It reannounces selected and background task registrations after its own IPC reconnect and
answers an App follower-status request only when the exact remembered conversation and host match.
This does not persist conversation content or raw IDs, change the selected pet, or make a
background task controllable. When the App reports that an owner is unavailable, its own private
runtime has a resume path; CoPets deliberately does not invoke or emulate that private operation.

## Live status

The pre-change packaged CoPets app attached to the running Codex App. Two explicit active-turn test
submissions were safely rejected before delivery because the selected follower owner did not refresh
within the reconnect window. No conversation content was delivered or recorded.

After a cold start of the rebuilt current app, one explicit active-turn follow-up returned success
through the exact selected owner: its CoPets form closed without an action error. That confirms the
current steering dispatch/response path, not that a particular model response was produced.

An observed selected-Ready attempt returned the owner-reconnecting error. Review traced that result
to CoPets requiring a changed owner during stale recovery, even though the current App can validly
reissue a state snapshot with the same owner. The current source now uses the narrower
post-write-refresh barrier described above. It still needs the selected-Ready live gate on the
rebuilt app.

A later selected-Ready attempt returned `no client found` after that refresh. Static analysis shows
this can mean the App's owner role or follower target is unavailable, not merely that CoPets lacks a
new owner ID. CoPets now retains and reannounces follower registrations, but treats a repeated
unavailable-owner result as a bounded failure and asks the user to focus the task so Codex can
resume it. This remains a live-gate limitation, not an end-to-end Ready success claim.

The current source was not yet exercised against a separately selected Ready conversation: the
desktop router does not expose a safe one-shot test-thread creator, and direct automation of the
Codex App is prohibited in this environment. The Ready feature remains Experimental until the
selected-Ready gate below passes on the rebuilt app.

## Reproduction commands

```bash
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' /Applications/ChatGPT.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Applications/ChatGPT.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' /Applications/ChatGPT.app/Contents/Info.plist
/Applications/ChatGPT.app/Contents/Resources/codex --version
rg -a -o -F 'thread-follower-start-turn' /Applications/ChatGPT.app/Contents/Resources/app.asar
rg -a -o -F 'thread-follower-start-turn-for-host' /Applications/ChatGPT.app/Contents/Resources/app.asar
```

The inspection deliberately emits only known method names and counts. Do not commit extracted
bundle code or local runtime payloads.

## Required selected-Ready live gate

1. Build and cold-start the current CoPets app, then run the IPC, session, and log probes.
2. Select a completed local Codex conversation that exposes a fresh owner, open the CoPets
   follow-up control, and send an explicit harmless message.
3. Confirm the App accepts the follower start request, exactly that conversation opens a new
   `working` epoch, and no background conversation or App focus changes.
4. Repeat with a stale owner and reconnect, selected/background switch-away and return, IPC
   disconnect/reconnect with retained follower registrations, selection change during recovery, and
   a failed/interrupted terminal task. The latter two must fail closed.
5. Record only sanitized method/version/result evidence in a later dated snapshot.
