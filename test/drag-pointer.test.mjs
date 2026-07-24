import test from "node:test";
import assert from "node:assert/strict";

import { createDragPointerTracker } from "../ui/lib/drag-pointer.js";

test("held drag follows live pointer direction until the primary button is released", async () => {
  const callbacks = [];
  const moves = [];
  let releases = 0;
  const snapshots = [
    { pressed: true, x: 112, y: 100 },
    { pressed: true, x: 96, y: 100 },
    { pressed: false, x: 96, y: 100 },
  ];
  const tracker = createDragPointerTracker({
    readSnapshot: async () => snapshots.shift(),
    onMove: (position) => moves.push(position),
    onRelease: () => releases += 1,
    setTimer: (callback) => callbacks.push(callback),
    clearTimer: () => {},
  });

  tracker.start();
  await callbacks.shift()();
  await callbacks.shift()();
  await callbacks.shift()();

  assert.deepEqual(moves, [
    { x: 112, y: 100 },
    { x: 96, y: 100 },
  ]);
  assert.equal(releases, 1);
  assert.equal(callbacks.length, 0);
});
