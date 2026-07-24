import { ANIMATIONS, animationForState, isTerminalState } from "./pet.js";

export const TERMINAL_HOLD_MS = 800;
export const BUBBLE_EXIT_MS = 180;
export const MAX_CONVERSATION_BUBBLES = 2;

function compact(value) {
  return typeof value === "string" ? value.trim() : "";
}

export function terminalKey(runtime) {
  return `${runtime?.threadIdHash || "none"}:${runtime?.epoch || 0}:${runtime?.state || "idle"}`;
}

export function createConversationBubbleQueue({ limit = MAX_CONVERSATION_BUBBLES } = {}) {
  let contextId;
  let sequence = 0;
  let lastUserText = "";
  let lastCodexText = "";
  let bubbles = [];

  function reset() {
    contextId = undefined;
    sequence = 0;
    lastUserText = "";
    lastCodexText = "";
    bubbles = [];
  }

  function push(side, text) {
    sequence += 1;
    bubbles.push({ id: `${contextId}:${side}:${sequence}`, side, text });
    bubbles = bubbles.slice(-limit);
  }

  function snapshot() {
    return bubbles.map((bubble, index) => {
      const groupedWithPrevious = bubbles[index - 1]?.side === bubble.side;
      const groupedWithNext = bubbles[index + 1]?.side === bubble.side;
      return {
        ...bubble,
        groupedWithPrevious,
        groupedWithNext,
        showArrow: !groupedWithNext,
      };
    });
  }

  return {
    update(runtime) {
      if (!runtime || runtime.state === "idle" || runtime.state === "disconnected") {
        reset();
        return [];
      }

      const nextContextId = `${runtime.threadIdHash || "none"}:${runtime.epoch || 0}`;
      if (nextContextId !== contextId) {
        reset();
        contextId = nextContextId;
      }

      const userText = compact(runtime.currentQuestion) || compact(runtime.taskSummary);
      const codexText = compact(runtime.latestUpdate);
      if (userText && userText !== lastUserText) {
        push("user", userText);
        lastUserText = userText;
        lastCodexText = "";
      }
      if (codexText && codexText !== lastCodexText) {
        push("codex", codexText);
        lastCodexText = codexText;
      }
      return snapshot();
    },
    reset,
  };
}

export function terminalPresentationDuration(state, reducedMotion = false) {
  if (!isTerminalState(state)) return 0;
  const animation = animationForState(state);
  const animationDuration = reducedMotion
    ? 0
    : ANIMATIONS[animation].durations.reduce((sum, duration) => sum + duration, 0);
  return animationDuration + TERMINAL_HOLD_MS;
}

export function createTerminalPresentationController({
  onExit,
  onSettle,
  setTimer = setTimeout,
  clearTimer = clearTimeout,
}) {
  let generation = 0;
  let exitTimer;
  let settleTimer;

  function cancel() {
    generation += 1;
    clearTimer(exitTimer);
    clearTimer(settleTimer);
    exitTimer = undefined;
    settleTimer = undefined;
  }

  return {
    schedule(runtime, reducedMotion = false) {
      cancel();
      if (!isTerminalState(runtime?.state)) return false;
      const currentGeneration = generation;
      const key = terminalKey(runtime);
      exitTimer = setTimer(() => {
        if (currentGeneration !== generation) return;
        onExit(key);
        settleTimer = setTimer(() => {
          if (currentGeneration !== generation) return;
          onSettle(key);
        }, BUBBLE_EXIT_MS);
      }, terminalPresentationDuration(runtime.state, reducedMotion));
      return true;
    },
    cancel,
  };
}
