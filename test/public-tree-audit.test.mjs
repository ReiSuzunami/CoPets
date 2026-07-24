import assert from "node:assert/strict";
import test from "node:test";

import { auditPath, auditText } from "../scripts/check-public-tree.mjs";

test("public tree audit rejects local generation paths", () => {
  assert.deepEqual(auditPath("tmp/review.md"), [
    "tmp/review.md: local/generated path is not public source",
  ]);
  assert.deepEqual(auditPath("scripts/__pycache__/tool.pyc"), [
    "scripts/__pycache__/tool.pyc: local/generated path is not public source",
    "scripts/__pycache__/tool.pyc: compiled cache is not public source",
  ]);
});

test("public tree audit rejects personal paths and high-confidence secrets", () => {
  const personalPath = ["/Users", "operator", "private", "file"].join("/");
  const privateKeyHeader = ["-----BEGIN", "PRIVATE", "KEY-----"].join(" ");
  assert.deepEqual(auditText("note.md", `open ${personalPath}`), [
    "note.md: contains personal macOS home path",
  ]);
  assert.deepEqual(auditText("key.txt", privateKeyHeader), [
    "key.txt: contains private key",
  ]);
});

test("public tree audit accepts ordinary source text", () => {
  assert.deepEqual(auditPath("src/observer.mjs"), []);
  assert.deepEqual(auditText("README.md", "Local-first macOS companion."), []);
});
