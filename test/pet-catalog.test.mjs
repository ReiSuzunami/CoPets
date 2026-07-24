import test from "node:test";
import assert from "node:assert/strict";

import { selectPetFromCatalog } from "../ui/lib/pet-catalog.js";

const pets = [{ id: "a" }, { id: "b" }];

test("catalog selection prefers an explicit newly imported pet", () => {
  assert.equal(selectPetFromCatalog(pets, "a", "b"), "b");
});

test("catalog selection keeps the current pet when it still exists", () => {
  assert.equal(selectPetFromCatalog(pets, "b"), "b");
});

test("catalog selection falls back to the first remaining pet", () => {
  assert.equal(selectPetFromCatalog(pets, "removed"), "a");
});

test("an empty catalog has no selected pet", () => {
  assert.equal(selectPetFromCatalog([], "removed"), "");
});
