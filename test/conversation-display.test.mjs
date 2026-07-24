import test from "node:test";
import assert from "node:assert/strict";

import { insertOverflowEllipsis } from "../ui/lib/bubble-overflow.js";
import {
  BUBBLE_EXIT_MS,
  MAX_CONVERSATION_BUBBLES,
  TERMINAL_HOLD_MS,
  createConversationBubbleQueue,
  createTerminalPresentationController,
  terminalPresentationDuration,
} from "../ui/lib/conversation-display.js";

function runtime(overrides = {}) {
  return {
    state: "working",
    threadIdHash: "thread-a",
    epoch: 3,
    contextMode: "question",
    currentQuestion: "Make the pet sharper",
    taskSummary: "Make the pet sharper",
    latestUpdate: null,
    ...overrides,
  };
}

test("conversation bubbles form a two-message IM queue", () => {
  const queue = createConversationBubbleQueue();
  const question = queue.update(runtime());
  assert.deepEqual(question.map(({ side, text }) => ({ side, text })), [
    { side: "user", text: "Make the pet sharper" },
  ]);

  const firstReply = queue.update(runtime({
    contextMode: "task",
    latestUpdate: "Inspecting the Retina atlas",
  }));
  const nextReply = queue.update(runtime({
    contextMode: "task",
    latestUpdate: "Rebuilding the native frames",
  }));
  assert.deepEqual(nextReply.map(({ side, text }) => ({ side, text })), [
    { side: "codex", text: "Inspecting the Retina atlas" },
    { side: "codex", text: "Rebuilding the native frames" },
  ]);
  assert.equal(nextReply.length, MAX_CONVERSATION_BUBBLES);
  assert.equal(firstReply[1].id, nextReply[0].id);
  assert.notEqual(nextReply[0].id, nextReply[1].id);
  assert.equal(nextReply[0].groupedWithNext, true);
  assert.equal(nextReply[0].showArrow, false);
  assert.equal(nextReply[1].groupedWithPrevious, true);
  assert.equal(nextReply[1].showArrow, true);

  const duplicate = queue.update(runtime({
    contextMode: "task",
    latestUpdate: "Rebuilding the native frames",
  }));
  assert.deepEqual(duplicate, nextReply);

  const nextQuestion = queue.update(runtime({
    epoch: 4,
    currentQuestion: "Move the speech bubble",
    taskSummary: "Move the speech bubble",
  }));
  assert.equal(nextQuestion.length, 1);
  assert.notEqual(nextQuestion[0].id, question[0].id);
  assert.equal(nextQuestion[0].showArrow, true);
});

test("visual overflow keeps only the leading text and a real ellipsis", () => {
  assert.equal(insertOverflowEllipsis("visible hidden", 7), "visible…");
  assert.equal(insertOverflowEllipsis("visible… hidden", 7), "visible…");
  assert.equal(insertOverflowEllipsis("short text", 100), "short text…");
  assert.equal(insertOverflowEllipsis("whole text", 5.9), "whole…");
});

test("terminal presentation includes animation, hold, and bubble exit", () => {
  assert.equal(terminalPresentationDuration("completed", false), 700 + TERMINAL_HOLD_MS);
  assert.equal(terminalPresentationDuration("failed", false), 1_220 + TERMINAL_HOLD_MS);
  assert.equal(terminalPresentationDuration("completed", true), TERMINAL_HOLD_MS);
  assert.equal(BUBBLE_EXIT_MS, 180);
});

test("new runtime state cancels an older terminal settlement", () => {
  const timers = new Map();
  const exits = [];
  const settlements = [];
  let nextTimer = 0;
  const controller = createTerminalPresentationController({
    onExit: (key) => exits.push(key),
    onSettle: (key) => settlements.push(key),
    setTimer: (callback) => {
      const id = ++nextTimer;
      timers.set(id, callback);
      return id;
    },
    clearTimer: (id) => timers.delete(id),
  });

  controller.schedule(runtime({ state: "completed" }), false);
  controller.cancel();
  for (const callback of [...timers.values()]) callback();
  assert.deepEqual(exits, []);
  assert.deepEqual(settlements, []);
});
