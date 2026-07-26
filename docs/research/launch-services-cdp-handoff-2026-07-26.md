# Launch Services CDP handoff assessment: 2026-07-26

> Status: Research snapshot
> Owns: Dated local assessment of the macOS launch handoff and observed permission-attribution result
> Update when: The official App, macOS launch behavior, handoff implementation, or manual C0a result changes
> Last verified: 2026-07-26

## Scope

This is not a full compatibility pass or a general attribution guarantee. It records the local
facts used to change CoPets from a direct official-executable child spawn to a Launch Services
handoff, the sanitized cold-launch observation made after that change, and the remaining
product-path gates.

## Local observations

Environment inspected on 2026-07-26:

| Item | Observed value |
| --- | --- |
| macOS | 26.5.2 (25F84) |
| Official App version | 26.721.41059 |
| Official App bundle build | 5848 |
| System launcher | `/usr/bin/open` |

The local `open` usage documents:

- `-n` / `--new` opens a new application instance;
- `--args` passes all remaining values to the application main-function argv;
- a bundle path is accepted as the item to open.

CoPets uses that handoff with the official bundle path and only
`--remote-debugging-address=127.0.0.1` plus its selected
`--remote-debugging-port=<port>`. The returned launcher status is intentionally not treated as the
Codex identity. Native code subsequently requires exactly one current-user official executable with
the exact port argument, then requires that PID to own the IPv4 loopback listener before Ready or a
send.

## Live attribution observation

The sanitized local A/B observation used CoPets 0.2.0 and the official App version listed above:

- The older debug build launched the official App as a direct CoPets child. Unified TCC logs showed
  `com.openai.codex` requesting Screen Capture while `dev.copets.sidecar` was the responsible
  process. Separate `SystemPolicyAppData` prompts came from long-running `/usr/bin/find` commands
  that retained the same old debug-build responsibility after their original App session ended.
- The current Launch Services build left both CoPets and the official App with launchd as their
  parent; the official App was not a CoPets child. The selected official process carried the exact
  loopback CDP arguments.
- A prompt still visible after the new launch was traced to those orphaned old-build `find`
  processes, not to the current release process. After terminating only those read-only orphan
  scans, no new `SystemPolicyAppData` request attributed to CoPets appeared in a 45-second check or
  a later two-minute check.
- The installed CoPets executable matched the tested release bundle SHA-256
  `99edeb6ce5a3e7536a9a677c2286f7781c833d5769ec68304ecd1ca395d77e96`.

This is evidence for this App build, permission class, and machine state only. It does not promise
that every future macOS permission dialog will name Codex, CoPets, or another responsible process.

## Inference and limit

Launch Services is the macOS app-opening boundary behind the system launcher, so this change removes
CoPets' direct child-process relationship to the official App. It is reasonable to expect this to
be closer to a normal Finder/Dock application launch.

The live observation confirms the intended launch structure and distinguishes the current process
from stale old-build responsibility. macOS still decides permission attribution from factors beyond
a stable parent PID, including the permission class, App version, system state, and the event that
caused the request.

## Remaining manual C0a/C0r verification

The attribution slice is recorded above. A full product gate still requires a deliberately prepared
test situation:

1. Quit all official Codex/ChatGPT App instances. Confirm the standard Launch path is eligible.
2. Start the current CoPets build, select an automatic unused bridge port, and choose **Launch
   Codex**. Do not use Connect existing for this gate.
3. Verify Settings reaches Ready only after native inspection proves one exact same-user official
   process owns the selected IPv4 loopback listener. Confirm the system launcher/helper PID was not
   accepted.
4. Verify a renderer page and the existing `Rf` fingerprint gate, then quit CoPets and confirm the
   launched official App remains running.
5. Exercise the separately confirmed C0r restart path and prove the old PID exits before one
   replacement reaches Ready.
6. For another A/B comparison, run an older direct-launch build only in a separate deliberately
   prepared test session. Do not recreate a direct launcher in the current product solely to
   provoke a prompt.

Until the remaining steps have a sanitized dated result, documentation may state the observed
launch and attribution result above, but must not claim full C0a/C0r compatibility or universal
permission-dialog behavior.

## References

- [ADR 0008](../decisions/0008-launch-services-cdp-handoff.md)
- [CDP follow-up channel](../architecture/cdp-follow-up-channel.md)
- [Apple Launch Services documentation](https://developer.apple.com/documentation/coreservices/launch_services)
- [Apple `NSWorkspace.OpenConfiguration` documentation](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration)
