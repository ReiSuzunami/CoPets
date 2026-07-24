export const ANIMATIONS = {
  idle: { row: 0, durations: [15_000, 110, 110, 140, 140, 320] },
  "running-right": { row: 1, durations: [120, 120, 120, 120, 120, 120, 120, 220] },
  "running-left": { row: 2, durations: [120, 120, 120, 120, 120, 120, 120, 220] },
  waving: { row: 3, durations: [140, 140, 140, 280] },
  jumping: { row: 4, durations: [140, 140, 140, 140, 280] },
  failed: { row: 5, durations: [140, 140, 140, 140, 140, 140, 140, 240] },
  waiting: { row: 6, durations: [150, 150, 150, 150, 150, 260] },
  running: { row: 7, durations: [220, 220, 240, 220, 220, 360] },
  review: { row: 8, durations: [150, 150, 150, 150, 150, 280] },
};

const TERMINAL_STATES = new Set(["completed", "failed", "interrupted"]);
const STATE_LABELS = {
  idle: "Ready",
  working: "Working",
  reviewing: "Reviewing",
  completed: "Complete",
  failed: "Failed",
  interrupted: "Interrupted",
  disconnected: "Waiting for Codex",
};

export function animationForState(state) {
  return {
    idle: "idle",
    working: "running",
    reviewing: "review",
    completed: "waving",
    failed: "failed",
    interrupted: "failed",
    disconnected: "idle",
  }[state] || "idle";
}

export function isTerminalState(state) {
  return TERMINAL_STATES.has(state);
}

export function labelForState(state) {
  return STATE_LABELS[state] || STATE_LABELS.idle;
}

export function shouldAdvanceAnimation({ reducedMotion, resting, hasTextures }) {
  return !reducedMotion && !resting && hasTextures;
}

export function frameForMotionPreference(frame, reducedMotion) {
  return reducedMotion ? 0 : frame;
}

export function animationForDragDirection(direction) {
  return direction === "left" ? "running-left" : "running-right";
}

export function advanceAnimationFrame(animation, frame, playOnce) {
  const lastFrame = ANIMATIONS[animation].durations.length - 1;
  if (frame < lastFrame) {
    return { animation, frame: frame + 1, resting: false };
  }
  if (playOnce) {
    return { animation: "idle", frame: 0, resting: true };
  }
  return { animation, frame: 0, resting: false };
}
