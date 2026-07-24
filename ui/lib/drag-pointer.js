export function createDragPointerTracker({
  readSnapshot,
  onMove,
  onRelease,
  onError = () => {},
  setTimer = setTimeout,
  clearTimer = clearTimeout,
  intervalMs = 32,
}) {
  let active = false;
  let generation = 0;
  let timer;

  function stop() {
    active = false;
    generation += 1;
    clearTimer(timer);
    timer = undefined;
  }

  function schedule(currentGeneration) {
    timer = setTimer(() => poll(currentGeneration), intervalMs);
  }

  async function poll(currentGeneration) {
    if (!active || currentGeneration !== generation) return;
    try {
      const snapshot = await readSnapshot();
      if (!active || currentGeneration !== generation) return;
      if (!snapshot?.pressed) {
        stop();
        onRelease();
        return;
      }
      onMove({ x: snapshot.x, y: snapshot.y });
      schedule(currentGeneration);
    } catch (cause) {
      if (!active || currentGeneration !== generation) return;
      stop();
      onError(cause);
      onRelease();
    }
  }

  return {
    start() {
      stop();
      active = true;
      generation += 1;
      schedule(generation);
    },
    stop,
  };
}
