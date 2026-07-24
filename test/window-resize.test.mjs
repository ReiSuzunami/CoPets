import test from "node:test";
import assert from "node:assert/strict";

import {
  centerWindowRect,
  createCornerResizeController,
  fitWindowRect,
  normalizeMonitor,
  resizeWindowFromCorner,
} from "../ui/lib/window-resize.js";

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};

test("monitor geometry has one canonical flat shape", () => {
  assert.deepEqual(normalizeMonitor({
    position: { x: -1728, y: 23 },
    size: { width: 1728, height: 1117 },
  }), { x: -1728, y: 23, width: 1728, height: 1117 });
  assert.throws(() => normalizeMonitor({ position: {}, size: {} }), TypeError);
});

test("saved window geometry is rounded, constrained, and clamped", () => {
  const monitor = { position: { x: -1728, y: 20 }, size: { width: 1728, height: 1080 } };
  assert.deepEqual(
    fitWindowRect({ x: -2200.4, y: 900.7, width: 100.2, height: 180.1 }, monitor),
    { x: -1728, y: 780, width: 280, height: 320 },
  );
  assert.deepEqual(
    fitWindowRect({ x: -2000, y: -200, width: 4000, height: 3000 }, monitor),
    { x: -1728, y: 20, width: 1728, height: 1080 },
  );
});

test("window centering handles monitor origins and monitors smaller than minimums", () => {
  assert.deepEqual(
    centerWindowRect({ width: 361, height: 481 }, {
      position: { x: 100, y: -900 },
      size: { width: 1000, height: 801 },
    }),
    { x: 420, y: -740, width: 361, height: 481 },
  );
  assert.deepEqual(
    centerWindowRect({ width: 360, height: 480 }, { x: 0, y: 0, width: 240, height: 300 }),
    { x: 0, y: 0, width: 240, height: 300 },
  );
});

test("corner drag keeps the top-left anchor and preserves aspect ratio", () => {
  const monitor = { x: 0, y: 0, width: 1440, height: 900 };
  const larger = resizeWindowFromCorner(
    { x: 100, y: 100, width: 360, height: 480 },
    monitor,
    { x: 40, y: 24 },
  );

  assert.deepEqual(larger, { x: 100, y: 100, width: 400, height: 533 });
  assert.ok(Math.abs(larger.width / larger.height - 0.75) < 0.002);
});

test("corner drag respects minimum size and monitor bounds", () => {
  const monitor = { x: -1920, y: 0, width: 1920, height: 1080 };
  const minimum = resizeWindowFromCorner(
    { x: -1900, y: 40, width: 280, height: 320 },
    monitor,
    { x: -80, y: -80 },
  );
  const edge = resizeWindowFromCorner(
    { x: -1000, y: 200, width: 360, height: 480 },
    monitor,
    { x: 400, y: 400 },
  );

  assert.deepEqual(minimum, { x: -1900, y: 40, width: 280, height: 320 });
  assert.deepEqual(edge, { x: -1000, y: 200, width: 660, height: 880 });
  assert.throws(
    () => resizeWindowFromCorner(
      { x: 0, y: 0, width: 0, height: 480 },
      { x: 0, y: 0, width: 1440, height: 900 },
      { x: 1, y: 1 },
    ),
    TypeError,
  );
});

test("resize controller coalesces movement and ignores a released startup", async () => {
  const startup = deferred();
  const applied = [];
  const active = [];
  let commits = 0;
  let releases = 0;
  const controller = createCornerResizeController({
    readInitialGeometry: () => startup.promise,
    readPointer: async () => ({ x: 180, y: 220 }),
    applySize: async (rect) => { applied.push(rect); },
    onActiveChange: (value) => active.push(value),
    onCommit: () => { commits += 1; },
    onError: (error) => { throw error; },
  });

  const started = controller.start({
    pointerId: 7,
    capture() {},
    release() { releases += 1; },
  });
  controller.stop();
  startup.resolve({
    rect: { x: 100, y: 100, width: 360, height: 480 },
    pointer: { x: 100, y: 100 },
    monitor: { x: 0, y: 0, width: 1440, height: 900 },
  });
  await started;
  await controller.move(7);

  assert.deepEqual(active, [true, false]);
  assert.equal(releases, 1);
  assert.equal(commits, 1);
  assert.deepEqual(applied, []);
});

test("resize controller serializes pending pointer movement", async () => {
  const firstApply = deferred();
  const pointers = [{ x: 140, y: 130 }, { x: 180, y: 160 }];
  const applied = [];
  const controller = createCornerResizeController({
    readInitialGeometry: async () => ({
      rect: { x: 100, y: 100, width: 360, height: 480 },
      pointer: { x: 100, y: 100 },
      monitor: { x: 0, y: 0, width: 1440, height: 900 },
    }),
    readPointer: async () => pointers.shift(),
    applySize: async (rect) => {
      applied.push(rect);
      if (applied.length === 1) await firstApply.promise;
    },
    onActiveChange() {},
    onCommit() {},
    onError: (error) => { throw error; },
  });

  await controller.start({ pointerId: 3, capture() {}, release() {} });
  const firstMove = controller.move(3);
  const secondMove = controller.move(3);
  firstApply.resolve();
  await Promise.all([firstMove, secondMove]);

  assert.deepEqual(applied, [
    { x: 100, y: 100, width: 400, height: 533 },
    { x: 100, y: 100, width: 440, height: 587 },
  ]);
});
