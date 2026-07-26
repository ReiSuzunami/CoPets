import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  normalizeControlAnswers,
  prepareFollowUp,
  visibleAnswer,
} from "../ui/lib/control-input.js";
import { shouldShowFollowUp } from "../ui/lib/follow-up-visibility.js";
import {
  PET_SELECTION_SYNC_ERROR,
  petSelectionSync,
  synchronizePetSelection,
} from "../ui/lib/pet-catalog.js";
import {
  CDP_CUSTOM_PORT_KEY,
  CDP_PORT_MODE_KEY,
  ONBOARDING_KEY,
  SELECTED_PET_KEY,
  WINDOW_STATE_KEY,
} from "../ui/lib/storage-keys.js";
import {
  CDP_PORT_MODE_AUTOMATIC,
  CDP_PORT_MODE_CUSTOM,
  bridgeNeedsVerificationRetry,
  bridgeSummaryLabel,
  bridgeStatusLabel,
  normalizeCdpPortMode,
  parseCustomCdpPort,
} from "../ui/lib/cdp-bridge-settings.js";
import {
  frameForMotionPreference,
  shouldAdvanceAnimation,
} from "../ui/lib/pet.js";
import {
  monitorIntersectionArea,
  selectBestMonitor,
} from "../ui/lib/window-resize.js";

if (!globalThis.navigator) {
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { userAgent: "node.js" },
  });
}

const { PixiPet } = await import("../ui/lib/pixi-pet.js");

test("control answers contain only trimmed non-empty strings", () => {
  assert.deepEqual(normalizeControlAnswers({
    first: " yes ",
    blank: "  ",
    second: "custom",
    invalid: null,
  }), {
    first: ["yes"],
    second: ["custom"],
  });
});

test("follow-up preparation rejects whitespace and trims valid input", () => {
  assert.equal(prepareFollowUp("   "), null);
  assert.equal(prepareFollowUp(" continue here "), "continue here");
  assert.equal(prepareFollowUp(null), null);
});

test("working and completed tasks keep the follow-up affordance visible while owner recovery is pending", () => {
  assert.equal(shouldShowFollowUp({ canReply: true }), true);
  assert.equal(shouldShowFollowUp({ canStartFollowUp: true }), true);
  assert.equal(shouldShowFollowUp({ showWorkingFollowUp: true }), true);
  assert.equal(shouldShowFollowUp({ showReadyFollowUp: true }), true);
  assert.equal(shouldShowFollowUp({}), false);
});

test("visible answers prefer option labels and preserve custom text", () => {
  const question = { options: [{ id: "one", label: "First" }] };
  assert.equal(visibleAnswer(question, "one"), "First");
  assert.equal(visibleAnswer(question, "custom"), "custom");
  assert.equal(visibleAnswer(question, ""), "");
});

test("pet selection synchronization targets the opposite window", () => {
  assert.deepEqual(petSelectionSync(true, "sample-hd"), {
    target: "pet",
    event: "pet-selection-changed",
    payload: { id: "sample-hd" },
  });
  assert.deepEqual(petSelectionSync(false, ""), {
    target: "settings",
    event: "pet-selection-changed",
    payload: { id: "" },
  });
});

test("pet selection synchronization reports a bounded failure without exposing the cause", async () => {
  const cause = new Error("sensitive /private/operator/path");
  const error = await synchronizePetSelection(false, "sample-hd", async () => {
    throw cause;
  });

  assert.equal(error, PET_SELECTION_SYNC_ERROR);
  assert.equal(error.includes("sensitive"), false);
  assert.equal(error.includes("/Users"), false);
});

test("monitor selection uses the greatest positive intersection", () => {
  const left = { position: { x: -1000, y: 0 }, size: { width: 1000, height: 800 } };
  const right = { position: { x: 0, y: 0 }, size: { width: 1000, height: 800 } };
  const rect = { x: -100, y: 50, width: 400, height: 400 };
  assert.equal(monitorIntersectionArea(rect, left), 40_000);
  assert.equal(monitorIntersectionArea(rect, right), 120_000);
  assert.equal(selectBestMonitor(rect, [left, right]), right);
  assert.equal(selectBestMonitor({ x: 2000, y: 0, width: 100, height: 100 }, [left, right]), null);
});

test("reduced motion holds frame zero and suppresses animation advance", () => {
  assert.equal(frameForMotionPreference(4, true), 0);
  assert.equal(frameForMotionPreference(4, false), 4);
  assert.equal(shouldAdvanceAnimation({ reducedMotion: true, resting: false, hasTextures: true }), false);
  assert.equal(shouldAdvanceAnimation({ reducedMotion: false, resting: true, hasTextures: true }), false);
  assert.equal(shouldAdvanceAnimation({ reducedMotion: false, resting: false, hasTextures: false }), false);
  assert.equal(shouldAdvanceAnimation({ reducedMotion: false, resting: false, hasTextures: true }), true);
});

test("runtime reduced-motion changes reset the Pixi renderer to a stable frame", () => {
  const renderer = Object.create(PixiPet.prototype);
  renderer.reducedMotion = false;
  renderer.frame = 4;
  renderer.elapsed = 120;
  let applied = 0;
  renderer.applyFrame = () => { applied += 1; };

  renderer.setReducedMotion(true);
  assert.equal(renderer.reducedMotion, true);
  assert.equal(renderer.frame, 0);
  assert.equal(renderer.elapsed, 0);
  assert.equal(applied, 1);
});

test("pet shadow tracks the sprite layout and clears with its texture set", () => {
  const renderer = Object.create(PixiPet.prototype);
  const calls = { sprite: [], shadow: [] };
  renderer.app = { screen: { width: 360, height: 480 } };
  renderer.cellWidth = 192;
  renderer.cellHeight = 208;
  renderer.sprite = {
    visible: true,
    texture: null,
    scale: { set: (...args) => calls.sprite.push(["scale", ...args]) },
    position: { set: (...args) => calls.sprite.push(["position", ...args]) },
  };
  renderer.shadow = {
    visible: true,
    clear: () => calls.shadow.push(["clear"]),
    scale: { set: (...args) => calls.shadow.push(["scale", ...args]) },
    position: { set: (...args) => calls.shadow.push(["position", ...args]) },
  };

  renderer.layout();
  const spriteScale = calls.sprite.find(([kind]) => kind === "scale");
  const shadowScale = calls.shadow.find(([kind]) => kind === "scale");
  const spritePosition = calls.sprite.find(([kind]) => kind === "position");
  const shadowPosition = calls.shadow.find(([kind]) => kind === "position");
  assert.deepEqual(shadowScale, spriteScale);
  assert.equal(shadowPosition[1], spritePosition[1]);
  assert.ok(shadowPosition[2] < spritePosition[2]);

  renderer.textures = new Map();
  renderer.lookTextures = [];
  renderer.atlas = null;
  renderer.lookTexture = null;
  renderer.destroyTextureSet = () => {};
  renderer.clear();
  assert.equal(renderer.shadow.visible, false);
  assert.equal(renderer.sprite.visible, false);
  assert.deepEqual(calls.shadow.at(-1), ["clear"]);
});

test("persisted namespaces use the CoPets product identity", () => {
  assert.equal(SELECTED_PET_KEY, "copets.selected-pet.v1");
  assert.equal(ONBOARDING_KEY, "copets.onboarding-complete.v1");
  assert.equal(WINDOW_STATE_KEY, "copets.window-state.v1");
  assert.equal(CDP_PORT_MODE_KEY, "copets.cdp-port-mode.v1");
  assert.equal(CDP_CUSTOM_PORT_KEY, "copets.cdp-custom-port.v1");
});

test("CDP bridge persists only a port preference and validates custom ports before invoke", () => {
  assert.equal(normalizeCdpPortMode("custom"), CDP_PORT_MODE_CUSTOM);
  assert.equal(normalizeCdpPortMode("anything-else"), CDP_PORT_MODE_AUTOMATIC);
  assert.equal(parseCustomCdpPort(" 52000 "), 52000);
  assert.equal(parseCustomCdpPort("1023"), null);
  assert.equal(parseCustomCdpPort("65536"), null);
  assert.equal(parseCustomCdpPort("52000/path"), null);
  assert.match(bridgeStatusLabel("cdpReady"), /ready/i);
  assert.match(bridgeStatusLabel("cdpDegraded"), /IPC/i);
  assert.equal(bridgeNeedsVerificationRetry("cdpDegraded"), true);
  assert.equal(bridgeNeedsVerificationRetry("cdpReady"), false);
  assert.equal(bridgeSummaryLabel("cdpReady"), "Ready");
  assert.equal(bridgeSummaryLabel("cdpDegraded"), "Unavailable");
  assert.equal(bridgeSummaryLabel("ipcOnly"), "Standard IPC");
});

test("bridge setup is collapsed by default and keeps its state visible", () => {
  const source = readFileSync(new URL("../ui/SettingsPanel.svelte", import.meta.url), "utf8");

  assert.match(source, /<details class="settings-row cdp-bridge">/);
  assert.match(source, /bridgeSummaryLabel\(cdpTransport\)/);
  assert.match(source, /class="cdp-bridge-details"/);
  assert.doesNotMatch(source, /cdp-bridge-card/);
});

test("bridge can explicitly connect an existing custom-port Codex while retry stays separate", () => {
  const panel = readFileSync(new URL("../ui/SettingsPanel.svelte", import.meta.url), "utf8");
  const pet = readFileSync(new URL("../ui/PetWindow.svelte", import.meta.url), "utf8");
  const settings = readFileSync(new URL("../ui/SettingsWindow.svelte", import.meta.url), "utf8");

  assert.match(panel, /bridgeNeedsVerificationRetry\(cdpTransport\)/);
  assert.match(panel, /onRetryCdpVerification/);
  assert.match(panel, /Retry verification/);
  assert.match(panel, /onConnectExistingCdp/);
  assert.match(panel, /Connect existing/);
  assert.match(pet, /invoke\("connect_existing_codex_cdp", \{ port \}\)/);
  assert.match(settings, /invoke\("connect_existing_codex_cdp", \{ port \}\)/);
  assert.match(pet, /cdpPortMode === CDP_PORT_MODE_AUTOMATIC\s*\? null/);
  assert.match(settings, /cdpPortMode === CDP_PORT_MODE_AUTOMATIC\s*\? null/);
  assert.match(pet, /invoke\("retry_cdp_bridge"\)/);
  assert.match(settings, /invoke\("retry_cdp_bridge"\)/);
  assert.doesNotMatch(panel, /on:click=\{\(\) => onRetryCdpVerification\([^)]/);
});

test("bridge restart is explicit, confirmed, and available only from standard IPC", () => {
  const panel = readFileSync(new URL("../ui/SettingsPanel.svelte", import.meta.url), "utf8");
  const pet = readFileSync(new URL("../ui/PetWindow.svelte", import.meta.url), "utf8");
  const settings = readFileSync(new URL("../ui/SettingsWindow.svelte", import.meta.url), "utf8");
  const native = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(panel, /onRestartCodexWithBridge/);
  assert.match(panel, /Restart Codex with bridge/);
  assert.match(panel, /\{#if cdpTransport === "ipcOnly"\}/);
  assert.match(pet, /title: "Restart Codex with bridge\?"/);
  assert.match(settings, /title: "Restart Codex with bridge\?"/);
  assert.match(pet, /invoke\("restart_codex_with_cdp", \{ customPort \}\)/);
  assert.match(settings, /invoke\("restart_codex_with_cdp", \{ customPort \}\)/);
  assert.match(native, /observer::commands::restart_codex_with_cdp/);
});

test("inline settings stay above conversation bubbles", () => {
  const source = readFileSync(new URL("../ui/style.css", import.meta.url), "utf8");
  const layer = (selector) => Number(source.match(new RegExp(`${selector}\\s*\\{[^}]*z-index:\\s*(\\d+)`, "s"))?.[1]);

  assert.ok(layer("\\.settings-panel") > layer("\\.conversation-bubbles"));
  assert.ok(layer("\\.settings-panel") > layer("\\.error"));
});
