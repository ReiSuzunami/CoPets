export function createTransientMessage({
  onChange,
  durationMs = 5000,
  schedule = setTimeout,
  cancel = clearTimeout,
}) {
  if (typeof onChange !== "function") throw new TypeError("onChange must be a function");

  let timer;
  let destroyed = false;

  function cancelPending() {
    if (timer === undefined) return;
    cancel(timer);
    timer = undefined;
  }

  function clear() {
    cancelPending();
    if (!destroyed) onChange("");
  }

  function show(cause) {
    if (destroyed) return;
    cancelPending();
    onChange(String(cause));
    timer = schedule(() => {
      timer = undefined;
      if (!destroyed) onChange("");
    }, durationMs);
  }

  function destroy() {
    destroyed = true;
    cancelPending();
  }

  return { clear, destroy, show };
}
