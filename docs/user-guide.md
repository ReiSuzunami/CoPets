# User guide

> Status: Normative
> Owns: End-user installation, first run, pet management, controls, troubleshooting, updating, and removal
> Update when: A user-facing workflow, requirement, control, error state, or installation path changes
> Last verified: 2026-07-26

[简体中文](user-guide.zh-CN.md) · English

CoPets is an independent macOS companion for an already-running Codex App. Once the `.app` is installed, selecting and managing pets does not require an Agent or manual file editing.

## Requirements and installation

The public `v0.2.0` prerelease requires:

- macOS 11 or newer.
- Codex App for task observation. CoPets may launch before or after Codex.

Download the universal DMG and its checksum from the
[`v0.2.0` release](https://github.com/ReiSuzunami/CoPets/releases/tag/v0.2.0). Verify the download:

```bash
shasum -a 256 -c CoPets-v0.2.0-macos-universal.dmg.sha256
```

Open the DMG and double-click **Install CoPets**. Confirm **Install** or **Upgrade**. The installer
validates both the embedded payload and staged copy, quits a running recognized CoPets instance,
and replaces an existing version through a same-directory backup that can be restored if placement
fails. It refuses symbolic links, foreign items named `CoPets.app`, changed bundle identities,
unexpected executable layouts, and invalid signatures.

After success, choose whether to eject and keep the DMG or eject and move the verified DMG to
Trash. The temporary eject helper deletes itself. On a Mac where `/Applications` is not writable,
the installer uses the current user's `~/Applications` directory.

The prerelease has a local development signature and is not notarized. Gatekeeper may require the
standard Finder **Open** confirmation. This is a public testing artifact, not a notarized distribution
build.

### Build from source

Source builds additionally require Node.js 20 or newer and a Rust toolchain:

```bash
npm install
npm run codesign:setup
npm run build:macos:signed -- --bundles app
```

The local signing setup creates a development-only signing identity in the current user's login keychain. The built application is:

```text
src-tauri/target/release/bundle/macos/CoPets.app
```

Open that application from Finder. The pet runs as a menu-bar accessory: it stays out of the Dock,
remains above ordinary windows, and exposes Open Settings, Show/Hide, and Quit from its round
menu-bar item. Developer ID signing, notarization, and public downloads remain M5 work in the
[roadmap](roadmap.md).

## First run

CoPets opens its compact settings panel on the first run or when no valid pet is installed. The shortest path is:

1. Open Codex App and select a local task.
2. Import a pet from the settings panel.
3. Close settings. The pet now follows the task selected in Codex.

The first-run explanation is dismissible. Disconnected guidance remains available whenever settings is open and Codex cannot be observed. A Codex update can change the private local interfaces CoPets observes; in that case update CoPets or consult the project's newest compatibility evidence rather than granting Accessibility access.

## Open settings

Move the pointer over the pet window. The round status control appears beside the pet's left foot without requiring CoPets to become the foreground app. Click it, then click the settings icon in the small status menu.

Alternatively, click the round CoPets item in the macOS menu bar and choose **Open Settings…**. An independent window appears at the center of the screen. It remains available while the pet is hidden and does not reveal, move, or resize the pet window.

Only one settings surface is shown at a time. Opening the menu-bar window closes the pet's inline panel, and opening inline settings closes the independent window.

Settings is also where you can reset the window's size and position if a saved placement no longer suits the current displays.

## Import and manage pets

CoPets supports the following Pet Creator-compatible inputs:

| Input | How to choose it | Result |
| --- | --- | --- |
| Package folder | Click **Folder** and choose the folder containing `pet.json` | The complete folder is validated and copied |
| Manifest file | Click **ZIP / pet.json** and choose `pet.json` | Its containing folder is treated as the package |
| ZIP archive | Click **ZIP / pet.json** and choose a `.zip` | A root package or one wrapping folder is extracted and validated |

A spritesheet image alone cannot be imported because it does not contain the manifest identity and version needed to interpret the atlas. See the [pet package contract](protocol/pet-package.md) for supported fields, geometry, limits, and archive safety rules.

After choosing a source, CoPets validates it and temporarily previews the candidate in the pet window. The preview panel shows its name, ID, sprite version, atlas dimensions, and native scale. Choose:

- **Install** to add a new package.
- **Replace** to replace an installed package with the same ID. A native confirmation appears first.
- **Cancel** to restore the currently selected pet without changing files.

The source is never moved or edited. Installation uses a validated staged copy. If validation or activation fails, the installed package remains unchanged and settings shows the reason.

Use the Pet selector to switch packages. **Rescan** detects valid packages added outside the app. **Show in Finder** opens `${CODEX_HOME:-~/.codex}/pets`. Invalid manually placed folders are excluded from selection and listed under **Needs attention**.

**Remove** permanently deletes the selected package from the pets directory after native confirmation. If other pets remain, CoPets selects the current valid pet or the first remaining one; if none remain, it clears the renderer and restores the import guidance. Removal does not delete the original folder or ZIP from which the pet was imported.

## Read task state

The status control and pet animation reflect the one task currently selected in Codex. Background tasks retain independent state and cannot take over the visible pet.

Typical states include ready, working, review, needs input or approval, complete, failed, interrupted, and waiting for Codex. Active states loop their working animation. Terminal actions play once, hold briefly, then the visible presentation returns to idle and clears its bubbles.

The top of the window may show up to two compact messages: a best-effort current question and the newest bounded Codex progress. Markdown is rendered safely. Long content keeps its visible prefix and ends with an ellipsis; CoPets is not a transcript viewer.

## Use controls

Approval and stop require the selected task's current, validated native owner. Follow-up uses that
same default path unless you explicitly enable the experimental CoPets bridge:

- Approval and question cards let you explicitly allow, deny, or answer the exact pending request.
- The reply control appears only during a live turn. It steers that turn; it does not create a new task or revive a completed one.
- A completed task keeps a **Continue** arrow visible. In standard mode it starts the next turn only
  after CoPets has a current validated owner. If that owner is reconnecting, an explicit retry first
  refreshes only that task's native follow registration. If it remains unavailable, open and focus
  that exact task in the official Codex App, wait for its owner to recover, then retry.
- **Experimental bridge:** In Settings, open the compact **Experimental bridge** disclosure. To
  start a new bridge session, choose automatic or an unused custom local port, quit Codex first, and
  click **Launch Codex**. To use a Codex App you already started with a loopback CDP port, click
  **Connect existing** instead—no Codex restart is needed. Automatic Connect accepts exactly one
  same-user official Codex candidate; if you run more than one, choose **Custom port**, enter the
  port you started Codex with, and Connect.

  If a normal Codex App is already open and you want CoPets to make it bridge-capable, use
  **Restart Codex with bridge**. It asks for confirmation because it closes that App; active work can
  be interrupted and unsaved App UI state can be lost. CoPets accepts exactly one same-user normal
  official App, rechecks it, asks it to close gracefully, and waits briefly before launching the
  bridge replacement. It never force-closes Codex. If more than one App is open, it does not choose
  one for you—close the extras yourself. This action appears only while standard IPC is active; a
  degraded tracked bridge instead offers Retry verification. It only appears in Settings and never
  runs from a Continue/Steer error.

  When the bridge says ready, Continue and active Steer can use the selected task's existing
  in-window session even while its IPC owner is stale. This uses a private, version-sensitive local
  debugging interface and is not an official OpenAI interface. CoPets accepts only an official
  same-user App process with an IPv4 loopback listener, matching CDP port, Codex renderer, and
  verified Pets handler; it does not accept a host, DevTools URL, ordinary browser, or arbitrary
  port. Use it only in a trusted local macOS user session.

  If verification says unavailable, keep that Codex App open and choose **Retry verification**.
  Retry only rechecks the same tracked process and never sends a follow-up or starts another App.
  If the listener closed or the process changed, connect, restart, or launch again. Launch, Restart,
  Connect, and Retry all finish with Ready or Unavailable after a bounded local check; they do not
  stay on Launching indefinitely.
- Stop targets only the selected task's current live owner.

If a control cannot send, switch back to the intended task in Codex and confirm that the turn is
still live or ready. CoPets deliberately does not fall back to a background task or activate Codex
App to manufacture availability.

## Move, resize, and restore

- Drag the pet body to move the window. Running starts only after real pointer movement, follows left/right direction, and continues through pauses until release.
- Hover near the pet's right foot and drag the round diagonal grip to resize proportionally.
- Position and size are restored at the next launch and clamped to an attached display.
- Use **Reset size & position** in settings to return to the default size and centered placement.

CoPets follows macOS light/dark appearance. Reduce Motion holds stable frames instead of looping motion.

## Privacy model

CoPets observes local, same-user Codex signals. Before IPC initialization it verifies both the
socket path owner and the connected peer. Session, app-log, and thread-index reads reject symlinks,
non-regular files, and files owned by another user. Path-based IPC and thread-index access also
rejects writable or foreign-owned ancestor directories. It does not proxy model traffic, inject
into the Codex WebView, or use Accessibility for message delivery.

It reads only the newest bounded portion of active session logs and keeps selected-task previews in memory. It does not persist conversation previews or display full prompts, full answers, hidden reasoning, tool arguments, or command output. Raw task/request IDs and private control payloads remain in native memory. Import source paths exist only during the current picker/preview operation and are not saved.

See [runtime architecture](architecture/runtime.md) for the complete trust boundary.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| **Waiting for Codex** | Open Codex App, select a local task, and leave CoPets running. No Accessibility permission is required. |
| State stays disconnected after a Codex update | Quit and reopen both apps, then check the newest dated compatibility research. Private local interfaces can change. |
| No pets found | Open settings and import a package folder, its `pet.json`, or a ZIP. A lone PNG/WebP is insufficient. |
| Package appears under **Needs attention** | Open the pets folder and fix or remove the named manual package. The displayed diagnostic identifies the first failed check. |
| Import fails | Confirm there is exactly one package manifest, no nested wrapper beyond one folder, and that the official or CoPets atlas geometry is valid. |
| Pet is blurry | Confirm the package has a validated `sidecarSpritesheetPath` pointing to a native 2x–4x atlas; changing only the manifest cannot add resolution. |
| Reply/approval/stop is hidden | The selected task has no live compatible owner or the action is not currently valid. In a verified experimental bridge, only Ready/Steer can bypass fresh IPC owner proof; approval and stop cannot. CoPets never starts a new turn as a fallback. |
| Window is misplaced or too small | Open settings and choose **Reset size & position**. |
| An error covers a control | Errors clear automatically after five seconds and can also be dismissed immediately. In settings they stay inside the panel. |

For development diagnostics, use the independent probes documented in [updating and release rules](maintenance/updating.md). Never attach raw conversation logs or private payloads to a public issue.

## Update and remove

For prerelease updates, download the newer DMG and run **Install CoPets** again. The installer quits
the recognized running copy, validates the replacement, stages it beside the destination, and
restores the previous app if final placement fails. Recheck Codex compatibility after either app
changes.

To remove CoPets, reopen its release DMG, double-click **Install CoPets**, and choose
**Uninstall Existing…**. After confirmation the verified app moves to Trash. The uninstaller does
not delete `${CODEX_HOME:-~/.codex}/pets`, Codex sessions, logs, databases, sockets, or imported
source folders because those are shared or externally owned. Remove individual packages through
CoPets settings only when you intend to delete those installed copies.
