import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const installer = await readFile(
  new URL("../installer/macos/Installer.swift", import.meta.url),
  "utf8",
);
const packager = await readFile(
  new URL("../scripts/package_macos_dmg.sh", import.meta.url),
  "utf8",
);
const behaviorTests = await readFile(
  new URL("../scripts/test_macos_installer.sh", import.meta.url),
  "utf8",
);
const finderLayout = await readFile(
  new URL("../scripts/write_dmg_ds_store.py", import.meta.url),
  "utf8",
);

test("installer pins product identity and validates both payload and installed app", () => {
  assert.match(installer, /bundleIdentifier = "dev\.copets\.sidecar"/);
  assert.match(installer, /executableName = "copets"/);
  assert.match(installer, /codesign/);
  assert.match(installer, /--verify/);
  assert.match(installer, /--deep/);
  assert.match(installer, /--strict/);
  assert.match(installer, /lstat/);
});

test("installer replaces through same-directory staging and restores backup", () => {
  assert.match(installer, /\.CoPets\.installing-/);
  assert.match(installer, /\.CoPets\.backup-/);
  assert.match(installer, /moveItem\(at: target, to: backup\)/);
  assert.match(installer, /moveItem\(at: backup, to: target\)/);
  assert.doesNotMatch(installer, /rm -rf/);
  assert.doesNotMatch(installer, /\/bin\/rm/);
});

test("uninstall is recoverable and never names shared Codex data for deletion", () => {
  assert.match(installer, /trashItem\(at: target/);
  assert.match(installer, /Pet packages in ~\/\.codex\/pets.*preserved/);
  assert.doesNotMatch(installer, /removeItem\(at:[^\n]*\.codex/);
  assert.doesNotMatch(installer, /removeItem\(at:[^\n]*pets/);
});

test("DMG packager embeds only the verified payload and uses copied cleanup handoff", () => {
  assert.match(packager, /Install \$\{app_name\}\.app/);
  assert.match(packager, /Contents\/Helpers/);
  assert.match(packager, /audit-dmg\.sh/);
  assert.match(packager, /write_dmg_ds_store\.py/);
  assert.match(installer, /hdiutil", \["info", "-plist"\]/);
  assert.match(installer, /cleanupPrefix \+ UUID\(\)\.uuidString/);
  assert.match(installer, /hdiutil", \["detach", mountURL\.path\]/);
});

test("Finder layout is deterministic and points at the signed installer", () => {
  assert.match(finderLayout, /backgroundImageAlias/);
  assert.match(finderLayout, /Path\(installer_name\)\.name != installer_name/);
  assert.match(finderLayout, /os\.lstat/);
  assert.match(finderLayout, /store\[installer_name\]\["Iloc"\] = \(360, 288\)/);
  assert.match(finderLayout, /WindowBounds/);
  assert.match(finderLayout, /iconSize/);
});

test("behavior test covers install, upgrade, conflicts, corrupt payload, symlinks, and uninstall", () => {
  assert.match(behaviorTests, /--test-install/);
  assert.match(behaviorTests, /installer-test-upgrade-marker/);
  assert.match(behaviorTests, /preserve-marker/);
  assert.match(behaviorTests, /ln -s/);
  assert.match(behaviorTests, /corrupt payload was accepted/);
  assert.match(behaviorTests, /--test-uninstall/);
});
