import assert from "node:assert/strict";
import test from "node:test";

import { createPetPresentation } from "../ui/lib/pet-presentation.js";

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("clearing a pending pet selection cannot restore the removed pet", async () => {
  const fetched = deferred();
  const rendered = [];
  const selected = [];
  let clears = 0;
  const presentation = createPetPresentation({
    fetchPet: () => fetched.promise,
    renderPet: async (pet) => rendered.push(pet.id),
    clearPet: () => clears += 1,
    destroyPet: () => {},
    onSelected: (id) => selected.push(id),
    onError: () => {},
  });

  const pending = presentation.select("removed");
  presentation.clear();
  fetched.resolve({ id: "removed" });

  assert.equal(await pending, false);
  assert.equal(clears, 1);
  assert.deepEqual(rendered, []);
  assert.deepEqual(selected, []);
});

test("a newer selection supersedes an older fetch", async () => {
  const oldPet = deferred();
  const hdPet = deferred();
  const rendered = [];
  const selected = [];
  const presentation = createPetPresentation({
    fetchPet: (id) => (id === "old" ? oldPet.promise : hdPet.promise),
    renderPet: async (pet) => rendered.push(pet.id),
    clearPet: () => {},
    destroyPet: () => {},
    onSelected: (id) => selected.push(id),
    onError: () => {},
  });

  const oldLoad = presentation.select("old");
  const hdLoad = presentation.select("hd");
  hdPet.resolve({ id: "hd" });
  assert.equal(await hdLoad, true);
  oldPet.resolve({ id: "old" });
  assert.equal(await oldLoad, false);

  assert.deepEqual(rendered, ["hd"]);
  assert.deepEqual(selected, ["hd"]);
});

test("an invalidated preview cannot replace the current textures", async () => {
  const decode = deferred();
  let importCurrent = true;
  let committed = false;
  const presentation = createPetPresentation({
    fetchPet: async () => null,
    renderPet: async (_pet, isCurrent) => {
      await decode.promise;
      if (!isCurrent()) return false;
      committed = true;
      return true;
    },
    clearPet: () => {},
    destroyPet: () => {},
    onSelected: () => {},
    onError: () => {},
  });

  const preview = presentation.preview(
    { id: "preview" },
    () => importCurrent,
  );
  importCurrent = false;
  decode.resolve();

  assert.equal(await preview, false);
  assert.equal(committed, false);
});

test("a newer selection invalidates an older in-flight decode", async () => {
  const oldDecode = deferred();
  const rendered = [];
  const selected = [];
  const presentation = createPetPresentation({
    fetchPet: async (id) => ({ id }),
    renderPet: async (pet, isCurrent) => {
      if (pet.id === "old") await oldDecode.promise;
      if (!isCurrent()) return false;
      rendered.push(pet.id);
      return true;
    },
    clearPet: () => {},
    destroyPet: () => {},
    onSelected: (id) => selected.push(id),
    onError: () => {},
  });

  const oldLoad = presentation.select("old");
  await Promise.resolve();
  assert.equal(await presentation.select("hd"), true);
  oldDecode.resolve();
  assert.equal(await oldLoad, false);

  assert.deepEqual(rendered, ["hd"]);
  assert.deepEqual(selected, ["hd"]);
});

test("destroy invalidates pending work and releases the renderer once", async () => {
  const fetched = deferred();
  let destroys = 0;
  const presentation = createPetPresentation({
    fetchPet: () => fetched.promise,
    renderPet: async () => true,
    clearPet: () => {},
    destroyPet: () => destroys += 1,
    onSelected: () => assert.fail("destroyed selection must not commit"),
    onError: () => {},
  });

  const pending = presentation.select("old");
  presentation.destroy();
  presentation.destroy();
  fetched.resolve({ id: "old" });

  assert.equal(await pending, false);
  assert.equal(destroys, 1);
});
