import assert from "node:assert/strict";
import test from "node:test";

import { createPetCatalogController } from "../ui/lib/pet-catalog-controller.js";

const catalog = (ids) => ({
  pets: ids.map((id) => ({ id, displayName: id.toUpperCase() })),
  issues: [],
});

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};

function harness({ selectedId = "a", withPresentation = true } = {}) {
  const calls = { list: 0, select: [], clear: 0, persist: [] };
  let currentCatalog = catalog(["a", "b"]);
  const controller = createPetCatalogController({
    initialSelectedId: selectedId,
    listPets: async () => {
      calls.list += 1;
      return currentCatalog;
    },
    presentation: withPresentation
      ? {
          cancel() {},
          clear() { calls.clear += 1; },
          async select(id) { calls.select.push(id); return true; },
        }
      : null,
    persistSelected(id) { calls.persist.push(id); },
    onError(error) { throw new Error(`unexpected controller error: ${error}`); },
  });
  return {
    calls,
    controller,
    setCatalog(next) { currentCatalog = next; },
  };
}

test("catalog refresh renders once and skips an unchanged selection reload", async () => {
  const { calls, controller } = harness();

  await controller.refresh();
  await controller.refresh();

  assert.equal(calls.list, 2);
  assert.deepEqual(calls.select, ["a"]);
  assert.deepEqual(calls.persist, ["a"]);
});

test("selection-only synchronization never rescans the catalog", () => {
  const { calls, controller } = harness({ withPresentation: false });

  controller.acceptExternalSelection("b");

  assert.equal(calls.list, 0);
  assert.equal(controller.snapshot().selectedId, "b");
  assert.deepEqual(calls.persist, []);
});

test("an external selection renders from the current catalog without scanning or persisting", async () => {
  const { calls, controller } = harness();
  await controller.refresh();
  calls.select.length = 0;
  calls.persist.length = 0;
  calls.list = 0;

  await controller.acceptExternalSelection("b");

  assert.equal(calls.list, 0);
  assert.deepEqual(calls.select, ["b"]);
  assert.deepEqual(calls.persist, []);
  assert.equal(controller.snapshot().selectedId, "b");
});

test("a preferred catalog change renders and persists the exact pet", async () => {
  const { calls, controller } = harness();

  await controller.refresh();
  await controller.refresh("b");

  assert.deepEqual(calls.select, ["a", "b"]);
  assert.deepEqual(calls.persist, ["a", "b"]);
  assert.equal(controller.snapshot().selectedId, "b");
});

test("an empty catalog clears renderer and persisted selection", async () => {
  const { calls, controller, setCatalog } = harness();
  await controller.refresh();
  setCatalog(catalog([]));

  await controller.refresh();

  assert.equal(calls.clear, 1);
  assert.equal(controller.snapshot().selectedId, "");
  assert.deepEqual(calls.persist, ["a", ""]);
});

test("an older catalog response cannot overwrite a newer refresh", async () => {
  const first = deferred();
  const second = deferred();
  const pending = [first.promise, second.promise];
  const selected = [];
  const controller = createPetCatalogController({
    listPets: () => pending.shift(),
    presentation: {
      cancel() {},
      clear() {},
      async select(id) { selected.push(id); return true; },
    },
    persistSelected() {},
  });

  const olderRefresh = controller.refresh();
  const newerRefresh = controller.refresh();
  second.resolve(catalog(["b"]));
  await newerRefresh;
  first.resolve(catalog(["a"]));
  await olderRefresh;

  assert.deepEqual(controller.snapshot().pets.map((pet) => pet.id), ["b"]);
  assert.deepEqual(selected, ["b"]);
});

test("a catalog mutation can force a same-ID package reload", async () => {
  const { calls, controller } = harness();
  await controller.refresh();

  await controller.refresh("a", { forceReload: true });

  assert.deepEqual(calls.select, ["a", "a"]);
  assert.deepEqual(calls.persist, ["a"]);
});
