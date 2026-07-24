# M0 clean-profile walkthrough

> Status: Normative
> Owns: Reproducible M0 end-to-end acceptance procedure and packaged UI checklist
> Update when: M0 onboarding, package-management flow, installation path, or acceptance procedure changes
> Last verified: 2026-07-24

This walkthrough verifies that ordinary use after installation does not require an Agent or manual
package editing. It uses a temporary `CODEX_HOME`, so it cannot see or mutate the operator's
installed pets. Conversation content is not recorded. Dated executions are immutable evidence
snapshots; the latest committed result is [2026-07-20](m0-result-2026-07-20.md).

## Procedure

1. Run the required local gates:

   ```bash
   npm run check
   cargo test --manifest-path src-tauri/Cargo.toml
   ```

2. Build and verify the locally signed application:

   ```bash
   npm run build:macos:signed -- --bundles app
   codesign --verify --deep --strict "src-tauri/target/release/bundle/macos/CoPets.app"
   ```

3. Create an empty temporary directory and launch the bundled executable with that directory as `CODEX_HOME`. Do not reuse the normal `~/.codex` path.
4. Confirm the empty catalog opens actionable settings, including connection help and all three supported selections: folder, `pet.json`, and ZIP.
5. Import a known-valid package ZIP, inspect its live preview metadata, and install it.
6. Import the corresponding folder or `pet.json` again, confirm the replacement warning, and replace it.
7. Quit and relaunch with the same temporary `CODEX_HOME`; confirm the installed pet is discovered without an Agent.
8. Remove the pet through settings, confirm the catalog becomes empty, and confirm the renderer clears.
9. Import an invalid or partial package and confirm the active valid package is not replaced. Place an invalid folder under `pets/`, rescan, and confirm it appears under **Needs attention** but not in the selector.
10. Run the UI integration checklist below.
11. Quit the smoke instance and delete only the temporary test directory.

The package-manager unit suite covers ZIP traversal, staged validation, replacement preservation, catalog diagnostics, fallback selection, and cleanup independently of this visual walkthrough.

## UI integration checklist

Use the signed bundle, not the source tree, so this checks the actual Tauri/WebView boundary:

- With the pet hidden, open Settings from the menu bar and confirm the detached window appears near
  screen center without revealing or moving the pet. Open inline settings from the pet and confirm
  the detached window closes.
- Change the selected pet from each settings surface and confirm only the opposite window receives
  the catalog/selection update. A failed load or invalid replacement must retain the last rendered
  pet.
- On a live task, confirm question/progress bubbles preserve at most two messages, stream Codex
  progress, trim visual overflow with an ellipsis, and clear after terminal settle.
- Enable Reduce Motion in macOS and relaunch. State sprites must hold a stable frame; bubble and
  status animations must not loop. Disable it and confirm normal motion returns.
- Hover the unfocused pet and confirm controls appear without focus activation. Drag from rest:
  running begins only after actual movement, follows horizontal direction, and continues through a
  held pause. Resize from the foot grip and confirm aspect ratio, minimum size, and monitor bounds.
- Check light and dark appearances at the minimum and default window sizes: controls, menus,
  bubbles, tails, shadows, status halo, and transient errors must remain visible and non-overlapping.

## Recording results

Add a new dated evidence document for every complete execution. Record the commit SHA, package and
platform versions, commands, sanitized results, signing/notarization state, and remaining
uncertainty. Never rewrite an older result to make it describe a newer build.
