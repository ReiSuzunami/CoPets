<p align="center">
  <img src="docs/assets/brand/copets-cloud.png" width="128" alt="CoPets yellow cloud">
</p>

<h1 align="center">CoPets</h1>

<p align="center">
  <strong>A desktop pet for your running Codex tasks.</strong>
</p>

<p align="center">
  English · <a href="README.zh-CN.md">简体中文</a>
</p>

CoPets is an independent, open-source macOS companion for Codex App. It turns local lifecycle
signals from the task selected in Codex into pet animations, compact context bubbles, and
context-aware controls.

CoPets runs locally. It does not proxy model traffic, modify the Codex UI, or require Accessibility
permission. It is not an official OpenAI product. Codex integration relies on private, unversioned
local interfaces and may need an update when Codex App changes.

The optional experimental bridge is explicitly configured in Settings and only uses a verified local
loopback endpoint owned by the official Codex App. See the [user guide](docs/user-guide.md) before
enabling it.

## Install

Requirements:

- macOS 11 or newer
- Codex App

Download the universal DMG and its checksum from
[GitHub Releases](https://github.com/ReiSuzunami/CoPets/releases). Keep both files in the same
folder, then verify the current release:

```bash
shasum -a 256 -c CoPets-v0.1.0-macos-universal.dmg.sha256
```

Open the DMG and double-click **Install CoPets**. The same installer can safely upgrade CoPets or
move an existing installation to Trash. Removing the app does not delete pet packages under
`${CODEX_HOME:-~/.codex}/pets`.

To build from source:

```bash
npm ci
npm run codesign:setup
npm run build:macos:signed -- --bundles app
```

The app is written to `src-tauri/target/release/bundle/macos/CoPets.app`.

## macOS notice

CoPets releases are intentionally not notarized by Apple. macOS may block **Install CoPets** or
CoPets on first launch.

If that happens:

1. Try to open the blocked app once.
2. Open **System Settings → Privacy & Security**.
3. Click **Open Anyway**, authenticate, and confirm **Open**.
4. Repeat for CoPets itself if macOS asks again after installation.

Only bypass Gatekeeper for a file downloaded from this repository's Releases page after its
checksum matches. The source and CI workflow are public, and you can build the app locally instead.

## Use

1. Open Codex App and select a local task.
2. Open CoPets. Settings appears automatically when no valid pet is installed.
3. Import a Pet Creator-compatible folder, `pet.json`, or ZIP. No pet is bundled or auto-installed;
   source checkouts include importable [example pets](examples/pets/).
4. Close Settings. The pet now follows the task selected in Codex.

Hover the pet to open its status menu, or use the round CoPets menu-bar item to open Settings,
show or hide the pet, and quit. Approval, reply, stop, and other task controls appear only when
CoPets can verify that they target the selected live task.

For pet management and troubleshooting, see the [user guide](docs/user-guide.md).

[MIT](LICENSE) © 2026 CoPets contributors.

Asset provenance and contribution requirements: [ASSET_LICENSES.md](ASSET_LICENSES.md).
