# Codex selection and owner-recovery static verification: 2026-07-26

> Status: Research snapshot
> Owns: Version-pinned static evidence for foreground selection, follower-owner recovery, and a retired clone comparison
> Update when: Codex App build/assets, current selection evidence, or historical-retirement status changes
> Last verified: 2026-07-26

## Scope

This snapshot compares the installed Codex App with the separately named, ad-hoc-signed CoPets
Codex Resume Lab. It is static inspection only: neither app was launched for this inspection. It
records no prompt, answer, raw conversation identifier, owner identifier, host identifier, request
payload, or credentials.

## Retirement note: 2026-07-26

The Lab comparison below records a discarded static experiment. [ADR
0004](../decisions/0004-retire-codex-resume-lab.md) retires that clone and its private bridge; it is
not a current CoPets capability. Current production behavior pairs only with the unmodified official
Codex App and fails closed when that App has not exposed a fresh exact owner.

## Verified artifacts

| Item | Value |
| --- | --- |
| Installed App | `/Applications/ChatGPT.app` (`com.openai.codex`) |
| App version / build | `26.721.41059` / `5848` |
| Lab bundle | `artifacts/codex-resume-lab/CoPets Codex Resume Lab.app` |
| Lab bundle identifier | `com.openai.codex.copets-resume-lab` |
| Lab signature | `codesign --verify --deep --strict` passed |
| Official native asset SHA-256 | `efcbdf277ce7c7b78db991bef1b05fd2fa78635f2c69c0d204cb3dcbd8e49a38` |
| Official WebView asset SHA-256 | `09909b1444003ea23a48d5fa973bedf48b638c6d6ef3059fb48a9f262e73513e` |

The Lab's patched asset hashes differ as expected because the version- and hash-gated builder adds
its narrowly scoped bridge. Their static method anchors remain compatible with the installed App
build.

## Observed foreground-selection chain

The current WebView's stream-view `setActive` path emits
`thread_stream_view_activity_changed`. The native log also carries the renderer focus and visibility
fields used by CoPets. This is the direct view-activity signal for the foreground conversation.

The separate `browser-sidebar-owner-sync` path emits an `ownerRoutePath` derived from router state.
It is a sidebar routing broadcast, not a stream-role or view-activity assertion. Static structure
therefore supports treating it as a compatibility hint, not as stronger authority than accepted
focused/visible foreground activity.

The two paths are independent enough that a startup scan cannot use log-file modification time as a
global ordering signal. CoPets now merges initial selection records by their fixed UTC event
timestamps and lets a confirmed foreground activity win over a conflicting route hint.

## Observed follow-up and recovery chain

The normal follower-start path sends `thread-follower-start-turn` using the stream role's exact
owner client target. The host handler then validates the conversation's follower/owner relationship
before invoking the existing turn-start implementation. A selected conversation and a currently
available exact owner are separate requirements.

The App raises the `no-client-found` class when the follower's owner disconnects, changes, or no
longer matches the stream revision. Its internal follower helper recognizes that class only for a
follower role, marks the conversation as needing resume, then calls
`resumeConversationForUnavailableOwner` through the App-local manager. This is not a generic
"conversation not found" result and it is not a sidecar transport failure.

The historical Resume Lab bridge was deliberately narrower: it accepted only an existing conversation bound
to the requested host, a follower stream role with a string owner, and a shape-checked one-time
nonce. It calls the installed App's own resume helper and requires CoPets to observe a fresh exact
owner snapshot before it can retry the original follow-up. The official app has no equivalent
sidecar `thread/resume` handler, so ordinary CoPets continues to fail closed rather than inventing a
resume payload.

## Consequence

The remaining normal-App fix belongs at the selection boundary, not SSH or generic IPC discovery:

```text
focused/visible foreground activity -> exact selected conversation
exact selected conversation + fresh IPC owner snapshot -> follower start/steer target
follower owner unavailable -> fail closed
```

This is static compatibility evidence, not an end-to-end selected-Ready delivery result. The live
gate in [Codex Ready follow-up compatibility: 2026-07-25](codex-ready-follow-up-2026-07-25.md)
still applies.

## Historical reproduction (retired)

The Lab command below records the discarded comparison artifact. ADR 0004 removed that artifact and
its builder; current CoPets verification uses the unmodified official Codex App only.

```bash
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Applications/ChatGPT.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' /Applications/ChatGPT.app/Contents/Info.plist
codesign --verify --deep --strict 'artifacts/codex-resume-lab/CoPets Codex Resume Lab.app'
```

Use bounded asset extraction or token inspection only. Do not commit extracted app code or live
runtime payloads.
