const DRAG_START_DISTANCE = 5;

export function createDragMotionController({
  setDirection,
  restore,
  onActiveChange = () => {},
}) {
  let active = false;
  let running = false;
  let originPosition;
  let lastPosition;
  let direction = "right";

  function finish() {
    if (!active) return;
    active = false;
    running = false;
    originPosition = undefined;
    lastPosition = undefined;
    restore();
    onActiveChange(false);
  }

  return {
    start(position) {
      active = true;
      running = false;
      originPosition = position;
      lastPosition = position;
      direction = "right";
      onActiveChange(true);
    },

    move(position) {
      if (!active || !Number.isFinite(position?.x) || !Number.isFinite(position?.y)) return;
      if (!Number.isFinite(lastPosition?.x) || !Number.isFinite(lastPosition?.y)) {
        originPosition = position;
        lastPosition = position;
        return;
      }
      const deltaX = Number.isFinite(lastPosition?.x) ? position.x - lastPosition.x : 0;
      const deltaY = Number.isFinite(lastPosition?.y) ? position.y - lastPosition.y : 0;
      if (deltaX === 0 && deltaY === 0) return;

      if (!running) {
        if (!Number.isFinite(originPosition?.x) || !Number.isFinite(originPosition?.y)) {
          originPosition = position;
          lastPosition = position;
          return;
        }
        const originDeltaX = position.x - originPosition.x;
        const originDeltaY = position.y - originPosition.y;
        if (Math.hypot(originDeltaX, originDeltaY) < DRAG_START_DISTANCE) {
          lastPosition = position;
          return;
        }
        running = true;
        if (originDeltaX !== 0) direction = originDeltaX < 0 ? "left" : "right";
      } else if (deltaX !== 0) {
        direction = deltaX < 0 ? "left" : "right";
      }
      setDirection(direction);
      lastPosition = position;
    },

    stop: finish,

    destroy() {
      active = false;
      running = false;
      originPosition = undefined;
      lastPosition = undefined;
    },
  };
}
