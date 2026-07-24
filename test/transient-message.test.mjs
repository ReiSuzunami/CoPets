import test from "node:test";
import assert from "node:assert/strict";

import { createTransientMessage } from "../ui/lib/transient-message.js";

function fakeClock() {
  let nextId = 0;
  const callbacks = new Map();
  return {
    schedule(callback) {
      const id = ++nextId;
      callbacks.set(id, callback);
      return id;
    },
    cancel(id) {
      callbacks.delete(id);
    },
    run(id) {
      const callback = callbacks.get(id);
      callbacks.delete(id);
      callback?.();
    },
    pendingIds() {
      return [...callbacks.keys()];
    },
  };
}

test("transient messages replace their timer and clear after the deadline", () => {
  const clock = fakeClock();
  const values = [];
  const message = createTransientMessage({
    onChange: (value) => values.push(value),
    durationMs: 5000,
    schedule: clock.schedule,
    cancel: clock.cancel,
  });

  message.show("first");
  const firstTimer = clock.pendingIds()[0];
  message.show("second");
  const secondTimer = clock.pendingIds()[0];
  assert.notEqual(secondTimer, firstTimer);
  assert.deepEqual(clock.pendingIds(), [secondTimer]);
  clock.run(secondTimer);
  assert.deepEqual(values, ["first", "second", ""]);
});

test("destroy cancels pending work and ignores late async errors", () => {
  const clock = fakeClock();
  const values = [];
  const message = createTransientMessage({
    onChange: (value) => values.push(value),
    schedule: clock.schedule,
    cancel: clock.cancel,
  });

  message.show("visible");
  message.destroy();
  message.show("late rejection");
  assert.deepEqual(clock.pendingIds(), []);
  assert.deepEqual(values, ["visible"]);
});
