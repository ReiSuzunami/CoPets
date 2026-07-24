import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizeControlAnswers,
  prepareFollowUp,
  visibleAnswer,
} from "../ui/lib/control-input.js";
import {
  PET_SELECTION_SYNC_ERROR,
  petSelectionSync,
  synchronizePetSelection,
} from "../ui/lib/pet-catalog.js";
import {
  ONBOARDING_KEY,
  SELECTED_PET_KEY,
  WINDOW_STATE_KEY,
} from "../ui/lib/storage-keys.js";
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

test("persisted namespaces use the CoPets product identity", () => {
  assert.equal(SELECTED_PET_KEY, "copets.selected-pet.v1");
  assert.equal(ONBOARDING_KEY, "copets.onboarding-complete.v1");
  assert.equal(WINDOW_STATE_KEY, "copets.window-state.v1");
});
