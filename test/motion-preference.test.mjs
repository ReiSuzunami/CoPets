import assert from "node:assert/strict";
import test from "node:test";

import { createMotionPreference } from "../ui/lib/motion-preference.js";

test("motion preference owns one media query and releases its listener", () => {
  const listeners = new Set();
  const values = [];
  let queryCount = 0;
  const media = {
    matches: true,
    addEventListener(type, listener) {
      assert.equal(type, "change");
      listeners.add(listener);
    },
    removeEventListener(type, listener) {
      assert.equal(type, "change");
      listeners.delete(listener);
    },
  };
  const preference = createMotionPreference({
    matchMedia(query) {
      queryCount += 1;
      assert.equal(query, "(prefers-reduced-motion: reduce)");
      return media;
    },
    onChange(value) { values.push(value); },
  });

  assert.equal(queryCount, 1);
  assert.deepEqual(values, [true]);
  assert.equal(preference.current(), true);

  media.matches = false;
  for (const listener of listeners) listener({ matches: false });
  assert.deepEqual(values, [true, false]);

  preference.destroy();
  assert.equal(listeners.size, 0);
});
