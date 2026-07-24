import assert from "node:assert/strict";
import test from "node:test";

import { encodeFrame, FrameDecoder, sanitizeIpcBroadcast } from "../src/ipc-observer.mjs";
import { normalizeSessionRecord } from "../src/session-tailer.mjs";
import {
  AppLogTailer,
  decorateKnownAppActivity,
  latestActivityFacts,
  normalizeAppLogLine,
} from "../src/app-log-tailer.mjs";
import { loadKnownThreadHashes } from "../src/thread-index.mjs";

test("frame decoder handles fragmented and coalesced frames", () => {
  const first = encodeFrame({ type: "broadcast", method: "one" });
  const second = encodeFrame({ type: "broadcast", method: "two" });
  const decoder = new FrameDecoder();

  assert.deepEqual(decoder.push(first.subarray(0, 3)), []);
  assert.deepEqual(decoder.push(Buffer.concat([first.subarray(3), second])), [
    { type: "broadcast", method: "one" },
    { type: "broadcast", method: "two" },
  ]);
});

test("IPC sanitizer never emits message text", () => {
  const event = sanitizeIpcBroadcast({
    method: "thread-stream-state-changed",
    version: 11,
    sourceClientId: "source-secret",
    params: {
      conversationId: "thread-secret",
      state: "streaming",
      message: "sensitive prompt",
      nested: { output: "sensitive result", status: "running" },
    },
  });

  assert.equal(event.fields.state, "streaming");
  assert.equal(event.diagnosticOnly, true);
  assert.equal(event.fields.conversationIdHash.length, 12);
  assert.equal(JSON.stringify(event).includes("sensitive"), false);
});

test("IPC sanitizer drops free-form text hidden under allowlisted keys", () => {
  const event = sanitizeIpcBroadcast({
    method: "thread-stream-state-changed",
    version: 11,
    sourceClientId: "source-secret",
    params: {
      conversationId: "thread-secret",
      state: "streaming",
      reason: "sensitive user prompt",
      status: "sensitive assistant answer",
      nested: { action: "sensitive tool output" },
    },
  });

  const serialized = JSON.stringify(event);
  assert.equal(event.fields.state, "streaming");
  assert.equal(event.fields.conversationIdHash.length, 12);
  assert.equal(serialized.includes("sensitive"), false);
  assert.equal("reason" in event.fields, false);
  assert.equal("status" in event.fields, false);
  assert.equal("nested" in event.fields, false);
});

test("IPC sanitizer keeps only bounded method-specific facts", () => {
  const event = sanitizeIpcBroadcast({
    method: "thread-stream-following-changed",
    version: 1,
    sourceClientId: "source-secret",
    params: {
      conversationId: "thread-secret",
      hostId: "host-secret",
      following: true,
      status: "sensitive free-form text",
    },
  });

  assert.equal(event.signal, "thread-stream-following-changed");
  assert.equal(event.fields.following, true);
  assert.equal(event.fields.conversationIdHash.length, 12);
  assert.equal(event.fields.hostIdHash.length, 12);
  assert.equal(JSON.stringify(event).includes("secret"), false);
  assert.equal("status" in event.fields, false);
});

test("IPC sanitizer bounds unknown schema metadata and rejects hostile shapes", () => {
  const params = JSON.parse(`{
    "__proto__": {"status": "sensitive"},
    "constructor": "sensitive",
    ${Array.from({ length: 80 }, (_, index) => `"field${index}": ${index}`).join(",")}
  }`);
  const event = sanitizeIpcBroadcast({
    method: "x".repeat(200),
    version: Number.MAX_SAFE_INTEGER,
    sourceClientId: "s".repeat(1_000),
    params,
  });

  assert.equal(event.signal, "unknown");
  assert.equal(event.version, null);
  assert.equal(event.sourceClientHash, null);
  assert.deepEqual(event.fields, {});
  assert.equal(event.paramKeys.length, 64);
  assert.equal(event.paramKeys.includes("__proto__"), false);
  assert.equal(event.paramKeys.includes("constructor"), false);
  assert.equal(JSON.stringify(event).includes("sensitive"), false);

  const inherited = Object.create({ state: "streaming" });
  assert.deepEqual(sanitizeIpcBroadcast({ method: "thread-stream-state-changed", params: inherited }).fields, {});
});

test("IPC sanitizer inspects only the declared nested change discriminator", () => {
  const event = sanitizeIpcBroadcast({
    method: "thread-stream-state-changed",
    version: 11,
    params: {
      conversationId: "thread-secret",
      change: {
        type: "snapshot",
        conversationState: {
          status: "sensitive assistant answer",
          prompt: "sensitive user prompt",
        },
      },
    },
  });

  assert.equal(event.fields.changeType, "snapshot");
  assert.equal(event.fields.conversationIdHash.length, 12);
  assert.equal(JSON.stringify(event).includes("sensitive"), false);
});

test("session records expose sanitized source facts without lifecycle policy", () => {
  const event = normalizeSessionRecord({
    type: "event_msg",
    payload: {
      type: "task_started",
      turn_id: "turn-secret",
      status: "sensitive status",
      message: "sensitive message",
    },
  }, "thread-hash");

  assert.equal(event.recordKind, "event-message");
  assert.equal(event.signal, "task_started");
  assert.equal(event.diagnosticOnly, true);
  assert.equal(event.threadIdHash, "thread-hash");
  assert.equal(event.turnIdHash.length, 12);
  assert.equal("state" in event, false);
  assert.equal("status" in event, false);
  assert.equal(JSON.stringify(event).includes("sensitive"), false);
});

test("session response items hash operations and omit free-form values", () => {
  const event = normalizeSessionRecord({
    type: "response_item",
    payload: {
      type: "function_call",
      name: "sensitive-operation-name",
      status: "sensitive assistant output",
    },
  }, "thread-hash");

  assert.equal(event.recordKind, "response-item");
  assert.equal(event.signal, "function_call");
  assert.equal(event.operationHash.length, 12);
  assert.equal("operation" in event, false);
  assert.equal("state" in event, false);
  assert.equal("status" in event, false);
  assert.equal(JSON.stringify(event).includes("sensitive"), false);
});

test("session records collapse unknown discriminators", () => {
  const event = normalizeSessionRecord({
    type: "event_msg",
    payload: { type: "sensitive-private-schema-name" },
  }, "thread-hash");

  assert.equal(event.signal, "unknown-event");
  assert.equal(JSON.stringify(event).includes("sensitive"), false);
});

test("app log parser emits activity facts without selection authority", () => {
  const event = normalizeAppLogLine(
    "2026-07-18T12:00:00.000Z info [electron-message-handler] thread_stream_view_activity_changed active=true conversationId=thread-secret rendererWebContentsId=1 rendererWindowFocused=true rendererWindowId=2 rendererWindowVisible=true rendererWindowAppearance=light resumeState=sensitive streamRole=sensitive summary=sensitive",
  );

  assert.equal(event.active, true);
  assert.equal(event.diagnosticOnly, true);
  assert.equal("selected" in event, false);
  assert.equal("state" in event, false);
  assert.equal(event.conversationIdHash.length, 12);
  assert.equal(event.windowId, 2);
  assert.equal(event.windowAppearance, "light");
  assert.equal(event.windowFocused, true);
  assert.equal(JSON.stringify(event).includes("sensitive"), false);
});

test("app log facts fail closed until the thread index confirms the thread", () => {
  const event = normalizeAppLogLine(
    "2026-07-18T12:00:00.000Z thread_stream_view_activity_changed active=true conversationId=thread-secret rendererWindowId=2",
  );
  const file = "/tmp/codex-desktop-instance-123-t1-i1.log";

  assert.equal(decorateKnownAppActivity(event, file, new Set(), false), null);
  assert.equal(decorateKnownAppActivity(event, file, new Set(), true), null);

  const known = decorateKnownAppActivity(
    event,
    file,
    new Set([event.conversationIdHash]),
    true,
  );
  assert.equal(known.conversationIdHash, event.conversationIdHash);
  assert.equal(known.appInstanceHash.length, 12);
  assert.equal("selectionConfidence" in known, false);
});

test("app log reconciliation retains latest bounded activity facts, including inactive views", () => {
  const base = {
    appInstanceHash: "app",
    windowId: 1,
    webContentsId: 2,
    conversationIdHash: "thread",
  };
  const facts = latestActivityFacts([
    { ...base, active: true, eventAt: "2026-07-18T12:00:00.000Z" },
    { ...base, active: false, eventAt: "2026-07-18T12:00:01.000Z" },
  ]);

  assert.deepEqual(facts, [{ ...base, active: false, eventAt: "2026-07-18T12:00:01.000Z" }]);
});

test("app log tailer can be instantiated", () => {
  assert.doesNotThrow(() => new AppLogTailer({ root: "/tmp/nonexistent-codex-log-root" }));
});

test("thread index hashes identifiers without retaining raw ids", async () => {
  const hashes = await loadKnownThreadHashes({
    dbPath: "/tmp/state.sqlite",
    inspect: async () => {},
    inspectParents: async () => {},
    run: async () => ({ stdout: "thread-one\nthread-two\n" }),
  });
  assert.equal(hashes.size, 2);
  assert.equal([...hashes].some((value) => value.includes("thread")), false);
});
