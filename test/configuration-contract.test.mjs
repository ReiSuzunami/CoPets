import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const readJson = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), "utf8"));
const tauri = await readJson("../src-tauri/tauri.conf.json");
const petCapability = await readJson("../src-tauri/capabilities/default.json");
const settingsCapability = await readJson("../src-tauri/capabilities/settings.json");
const packageConfig = await readJson("../package.json");

test("Tauri config declares one persistent interactive pet window", () => {
  assert.equal(tauri.$schema, "https://schema.tauri.app/config/2");
  assert.equal(tauri.app.macOSPrivateApi, true);
  assert.equal(tauri.app.windows.length, 1);
  assert.deepEqual(tauri.app.windows[0], {
    label: "pet",
    title: "CoPets",
    width: 360,
    height: 480,
    minWidth: 280,
    minHeight: 320,
    resizable: true,
    decorations: false,
    transparent: true,
    alwaysOnTop: true,
    acceptFirstMouse: true,
    visibleOnAllWorkspaces: true,
    skipTaskbar: true,
    shadow: false,
    center: true,
  });
  assert.equal(tauri.app.windows.some(({ label }) => label === "settings"), false);
});

test("window capabilities are scoped to pet and detached settings surfaces", () => {
  assert.deepEqual(petCapability.windows, ["pet"]);
  assert.deepEqual(new Set(petCapability.permissions), new Set([
    "core:default",
    "core:window:allow-start-dragging",
    "core:window:allow-start-resize-dragging",
    "core:window:allow-set-position",
    "core:window:allow-set-size",
    "core:window:allow-outer-position",
    "core:window:allow-outer-size",
    "core:window:allow-available-monitors",
    "core:window:allow-primary-monitor",
    "dialog:allow-open",
    "dialog:allow-message",
  ]));
  assert.deepEqual(settingsCapability.windows, ["settings"]);
  assert.deepEqual(new Set(settingsCapability.permissions), new Set([
    "core:default",
    "core:window:allow-close",
    "dialog:allow-open",
    "dialog:allow-message",
  ]));
});

test("build commands and release targets use the supported pipeline", () => {
  assert.deepEqual(tauri.build, {
    beforeDevCommand: "npm run frontend:dev",
    devUrl: "http://127.0.0.1:1420",
    beforeBuildCommand: "npm run frontend:build",
    frontendDist: "../dist",
  });
  assert.deepEqual(tauri.bundle.targets, ["app"]);
  assert.deepEqual(tauri.bundle.icon, [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico",
  ]);
  assert.equal(tauri.bundle.macOS.signingIdentity, "-");
  assert.equal(tauri.bundle.macOS.minimumSystemVersion, "11.0");
  assert.equal(
    packageConfig.scripts.check,
    "npm test && npm run docs:check && npm run audit:public && cargo check --manifest-path src-tauri/Cargo.toml --locked",
  );
  assert.equal(
    packageConfig.scripts.test,
    "node --test test/*.test.mjs && npm run frontend:build",
  );
  assert.equal(
    packageConfig.scripts["check:all"],
    "npm run check && cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check && npm run test:rust && npm run test:atlas",
  );
  assert.equal(packageConfig.scripts["build:macos:signed"], "bash scripts/build_macos_signed.sh");
  assert.equal(packageConfig.scripts["package:macos:dmg"], "bash scripts/package_macos_dmg.sh");
  assert.equal(packageConfig.scripts["test:macos-installer"], "bash scripts/test_macos_installer.sh");
});
