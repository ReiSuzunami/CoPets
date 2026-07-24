import test from "node:test";
import assert from "node:assert/strict";

import {
  ANIMATIONS,
  advanceAnimationFrame,
  animationForDragDirection,
  animationForState,
  isTerminalState,
  labelForState,
} from "../ui/lib/pet.js";

test("terminal animations play once and then hold the idle rest frame", () => {
  for (const state of ["completed", "failed", "interrupted"]) {
    const animation = animationForState(state);
    const lastFrame = ANIMATIONS[animation].durations.length - 1;
    assert.equal(isTerminalState(state), true);
    assert.deepEqual(advanceAnimationFrame(animation, lastFrame, true), {
      animation: "idle",
      frame: 0,
      resting: true,
    });
  }
});

test("active and idle animations continue looping", () => {
  for (const state of ["idle", "working", "reviewing"]) {
    const animation = animationForState(state);
    const lastFrame = ANIMATIONS[animation].durations.length - 1;
    assert.equal(isTerminalState(state), false);
    assert.deepEqual(advanceAnimationFrame(animation, lastFrame, false), {
      animation,
      frame: 0,
      resting: false,
    });
  }
});

test("idle waits before replaying the blink sequence", () => {
  assert.ok(ANIMATIONS.idle.durations[0] >= 12_000);
  assert.ok(ANIMATIONS.idle.durations.reduce((sum, duration) => sum + duration, 0) >= 12_500);
});

test("window drag direction selects the directional running rows", () => {
  assert.equal(animationForDragDirection("left"), "running-left");
  assert.equal(animationForDragDirection("right"), "running-right");
});

test("unknown lifecycle states fail closed to idle", () => {
  const state = "unknown-state";
  assert.equal(animationForState(state), "idle");
  assert.equal(isTerminalState(state), false);
  assert.equal(labelForState(state), "Ready");
});

test("working animation uses a calm readable cadence", () => {
  assert.ok(ANIMATIONS.running.durations.every((duration) => duration >= 180));
  assert.ok(ANIMATIONS.running.durations.reduce((sum, duration) => sum + duration, 0) >= 1_300);
});
