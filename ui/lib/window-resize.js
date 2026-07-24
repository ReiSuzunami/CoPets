export const MIN_WINDOW_WIDTH = 280;
export const MIN_WINDOW_HEIGHT = 320;

function requireFinite(values, message) {
  if (!values.every(Number.isFinite)) throw new TypeError(message);
}

export function normalizeMonitor(monitor) {
  const normalized = {
    x: monitor?.x ?? monitor?.position?.x,
    y: monitor?.y ?? monitor?.position?.y,
    width: monitor?.width ?? monitor?.size?.width,
    height: monitor?.height ?? monitor?.size?.height,
  };
  requireFinite(
    [normalized.x, normalized.y, normalized.width, normalized.height],
    "monitor geometry must be finite",
  );
  if (normalized.width <= 0 || normalized.height <= 0) {
    throw new TypeError("monitor dimensions must be positive");
  }
  return normalized;
}

export function monitorIntersectionArea({ x, y, width, height }, monitor) {
  const bounds = normalizeMonitor(monitor);
  const left = Math.max(x, bounds.x);
  const top = Math.max(y, bounds.y);
  const right = Math.min(x + width, bounds.x + bounds.width);
  const bottom = Math.min(y + height, bounds.y + bounds.height);
  return Math.max(0, right - left) * Math.max(0, bottom - top);
}

export function selectBestMonitor(rect, monitors) {
  const best = monitors.reduce((candidate, monitor) => {
    const area = monitorIntersectionArea(rect, monitor);
    return area > candidate.area ? { monitor, area } : candidate;
  }, { monitor: null, area: 0 });
  return best.monitor;
}

function clamp(value, minimum, maximum) {
  return Math.min(Math.max(value, minimum), maximum);
}

export function fitWindowRect(rect, monitor) {
  requireFinite(
    [rect?.x, rect?.y, rect?.width, rect?.height],
    "window geometry must be finite",
  );
  const bounds = normalizeMonitor(monitor);
  const minimumWidth = Math.min(MIN_WINDOW_WIDTH, bounds.width);
  const minimumHeight = Math.min(MIN_WINDOW_HEIGHT, bounds.height);
  const width = clamp(Math.round(rect.width), minimumWidth, bounds.width);
  const height = clamp(Math.round(rect.height), minimumHeight, bounds.height);
  const x = clamp(Math.round(rect.x), bounds.x, bounds.x + bounds.width - width);
  const y = clamp(Math.round(rect.y), bounds.y, bounds.y + bounds.height - height);
  return { x, y, width, height };
}

export function centerWindowRect(size, monitor) {
  requireFinite([size?.width, size?.height], "window dimensions must be finite");
  const bounds = normalizeMonitor(monitor);
  const width = clamp(Math.round(size.width), Math.min(MIN_WINDOW_WIDTH, bounds.width), bounds.width);
  const height = clamp(Math.round(size.height), Math.min(MIN_WINDOW_HEIGHT, bounds.height), bounds.height);
  return {
    x: bounds.x + Math.round((bounds.width - width) / 2),
    y: bounds.y + Math.round((bounds.height - height) / 2),
    width,
    height,
  };
}

export function resizeWindowFromCorner(rect, monitor, pointerDelta) {
  const bounds = normalizeMonitor(monitor);
  requireFinite(
    [rect?.x, rect?.y, rect?.width, rect?.height, pointerDelta?.x, pointerDelta?.y],
    "window, monitor, and pointer geometry must be finite",
  );
  if (rect.width <= 0 || rect.height <= 0) {
    throw new TypeError("window dimensions must be positive");
  }

  const aspect = rect.width / rect.height;
  const widthFromVerticalMotion = pointerDelta.y * aspect;
  const deltaWidth = Math.abs(pointerDelta.x) >= Math.abs(widthFromVerticalMotion)
    ? pointerDelta.x
    : widthFromVerticalMotion;
  const minimumWidth = Math.max(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT * aspect);
  const maximumWidth = Math.max(
    minimumWidth,
    Math.min(
      bounds.x + bounds.width - rect.x,
      (bounds.y + bounds.height - rect.y) * aspect,
    ),
  );
  const width = Math.round(clamp(rect.width + deltaWidth, minimumWidth, maximumWidth));
  const height = Math.round(width / aspect);
  return { x: rect.x, y: rect.y, width, height };
}

export function createCornerResizeController({
  readInitialGeometry,
  readPointer,
  applySize,
  onActiveChange,
  onCommit,
  onError,
}) {
  let session;
  let destroyed = false;

  async function start({ pointerId, capture, release }) {
    if (destroyed) return false;
    if (session) stop();
    const current = {
      pointerId,
      release,
      ready: false,
      busy: false,
      pending: false,
    };
    session = current;
    onActiveChange(true);
    try {
      capture?.();
      const geometry = await readInitialGeometry();
      if (session !== current || destroyed) return false;
      current.rect = geometry.rect;
      current.pointer = geometry.pointer;
      current.monitor = normalizeMonitor(geometry.monitor);
      current.ready = true;
      return true;
    } catch (cause) {
      if (session === current) {
        try {
          onError(cause);
        } finally {
          stop();
        }
      }
      return false;
    }
  }

  async function move(pointerId) {
    const current = session;
    if (!current?.ready || current.pointerId !== pointerId) return;
    current.pending = true;
    if (current.busy) return;
    current.busy = true;
    try {
      while (session === current && current.pending) {
        current.pending = false;
        const pointer = await readPointer();
        if (session !== current) break;
        const next = resizeWindowFromCorner(current.rect, current.monitor, {
          x: pointer.x - current.pointer.x,
          y: pointer.y - current.pointer.y,
        });
        await applySize(next);
      }
    } catch (cause) {
      if (session === current) {
        try {
          onError(cause);
        } finally {
          stop();
        }
      }
    } finally {
      current.busy = false;
    }
  }

  function stop(commit = true) {
    const current = session;
    if (!current) return;
    session = undefined;
    current.release?.();
    onActiveChange(false);
    if (commit) onCommit();
  }

  function destroy() {
    if (destroyed) return;
    destroyed = true;
    stop(false);
  }

  return { destroy, move, start, stop };
}
