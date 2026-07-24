import test from "node:test";
import assert from "node:assert/strict";

import { createDragMotionController } from "../ui/lib/drag-motion.js";

function createHarness() {
  const directions = [];
  const activeChanges = [];
  let restores = 0;
  const controller = createDragMotionController({
    setDirection: (direction) => directions.push(direction),
    restore: () => restores += 1,
    onActiveChange: (active) => activeChanges.push(active),
  });
  return {
    controller,
    directions,
    activeChanges,
    get restores() { return restores; },
  };
}

test("pet starts running only after movement, then keeps running through pauses", () => {
  const harness = createHarness();
  harness.controller.start({ x: 100, y: 100 });
  assert.deepEqual(harness.directions, []);
  assert.deepEqual(harness.activeChanges, [true]);
  assert.equal(harness.restores, 0);

  harness.controller.move({ x: 100, y: 100 });
  assert.deepEqual(harness.directions, []);

  harness.controller.move({ x: 108, y: 100 });
  harness.controller.move({ x: 104, y: 101 });
  harness.controller.move({ x: 104, y: 101 });
  assert.deepEqual(harness.directions, ["right", "left"]);
  assert.equal(harness.restores, 0);

  harness.controller.stop();
  assert.equal(harness.restores, 1);
  assert.deepEqual(harness.activeChanges, [true, false]);
  harness.controller.stop();
  assert.equal(harness.restores, 1);
});

test("initial pointer jitter does not start the running animation", () => {
  const harness = createHarness();
  harness.controller.start({ x: 100, y: 100 });

  harness.controller.move({ x: 101, y: 100 });
  harness.controller.move({ x: 102, y: 101 });
  harness.controller.move({ x: 100, y: 102 });
  assert.deepEqual(harness.directions, []);

  harness.controller.move({ x: 105, y: 100 });
  assert.deepEqual(harness.directions, ["right"]);
});

test("slow movement accumulates from the press origin before running", () => {
  const harness = createHarness();
  harness.controller.start({ x: 100, y: 100 });

  for (const x of [101, 102, 103, 104]) {
    harness.controller.move({ x, y: 100 });
  }
  assert.deepEqual(harness.directions, []);

  harness.controller.move({ x: 105, y: 100 });
  harness.controller.move({ x: 101, y: 100 });
  assert.deepEqual(harness.directions, ["right", "left"]);
});

test("first native pointer sample establishes the drag origin", () => {
  const harness = createHarness();
  harness.controller.start();

  harness.controller.move({ x: 900, y: 400 });
  assert.deepEqual(harness.directions, []);

  harness.controller.move({ x: 905, y: 400 });
  assert.deepEqual(harness.directions, ["right"]);
});

test("vertical movement does not interrupt the held running animation", () => {
  const harness = createHarness();
  harness.controller.start({ x: 100, y: 100 });
  harness.controller.move({ x: 100, y: 112 });
  assert.deepEqual(harness.directions, ["right"]);
  assert.equal(harness.restores, 0);
});
