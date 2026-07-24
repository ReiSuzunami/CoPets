import assert from "node:assert/strict";
import { appendFile, mkdtemp, rename, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { createAppendCursor, readAppendedLines } from "../src/append-follower.mjs";
import { SessionTailer } from "../src/session-tailer.mjs";

async function temporaryDirectory(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "copets-tail-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}

function waitForEvent(emitter, name, predicate, timeoutMs = 1_000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      emitter.off(name, onEvent);
      reject(new Error(`timed out waiting for ${name}`));
    }, timeoutMs);
    function onEvent(event) {
      if (!predicate(event)) return;
      clearTimeout(timer);
      emitter.off(name, onEvent);
      resolve(event);
    }
    emitter.on(name, onEvent);
  });
}

test("append follower preserves split UTF-8 and incomplete-line carry", async (t) => {
  const root = await temporaryDirectory(t);
  const file = path.join(root, "events.jsonl");
  await writeFile(file, Buffer.alloc(0));
  const cursor = createAppendCursor();
  const encoded = Buffer.from("你好\n", "utf8");

  await appendFile(file, encoded.subarray(0, 2));
  assert.deepEqual(await readAppendedLines(file, cursor), []);
  await appendFile(file, encoded.subarray(2));
  assert.deepEqual(await readAppendedLines(file, cursor), ["你好"]);
});

test("append follower resets on truncation and file rotation", async (t) => {
  const root = await temporaryDirectory(t);
  const file = path.join(root, "events.log");
  const rotated = path.join(root, "events.log.1");
  await writeFile(file, "first-long-line\n");
  const cursor = createAppendCursor();
  assert.deepEqual(await readAppendedLines(file, cursor), ["first-long-line"]);

  await writeFile(file, "short\n");
  assert.deepEqual(await readAppendedLines(file, cursor), ["short"]);

  await rename(file, rotated);
  await writeFile(file, "rotated\n");
  assert.deepEqual(await readAppendedLines(file, cursor), ["rotated"]);
});

test("append follower rejects symlinked evidence", async (t) => {
  const root = await temporaryDirectory(t);
  const target = path.join(root, "target.jsonl");
  const link = path.join(root, "link.jsonl");
  await writeFile(target, "private\n");
  await symlink(target, link);

  await assert.rejects(readAppendedLines(link, createAppendCursor()));
});

test("session tailer stops watch and polling delivery", async (t) => {
  const root = await temporaryDirectory(t);
  const file = path.join(root, "rollout-00000000-0000-0000-0000-000000000001.jsonl");
  await writeFile(file, "");
  const tailer = new SessionTailer({ root, pollIntervalMs: 10 });
  const received = [];
  tailer.on("event", (event) => received.push(event));
  await tailer.start();
  t.after(() => tailer.stop());

  const firstEvent = waitForEvent(tailer, "event", (event) => event.signal === "task_started");
  await appendFile(file, `${JSON.stringify({ type: "event_msg", payload: { type: "task_started" } })}\n`);
  await firstEvent;
  tailer.stop();

  const count = received.length;
  await appendFile(file, `${JSON.stringify({ type: "event_msg", payload: { type: "task_complete" } })}\n`);
  await new Promise((resolve) => setTimeout(resolve, 80));
  assert.equal(received.length, count);
});
