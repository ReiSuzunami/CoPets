export function createPetPresentation({
  fetchPet,
  renderPet,
  clearPet,
  destroyPet,
  onSelected,
  onError,
}) {
  let generation = 0;
  let destroyed = false;

  const beginOperation = (externalCurrent = () => true) => {
    const operation = ++generation;
    return () => !destroyed && operation === generation && externalCurrent();
  };

  async function select(id) {
    if (!id || destroyed) return false;
    const isCurrent = beginOperation();
    try {
      const pet = await fetchPet(id);
      if (!isCurrent()) return false;
      const rendered = await renderPet(pet, isCurrent);
      if (!isCurrent() || rendered === false) return false;
      onSelected(id);
      onError("");
      return true;
    } catch (cause) {
      if (isCurrent()) onError(String(cause));
      return false;
    }
  }

  async function preview(pet, externalCurrent = () => true) {
    if (destroyed) return false;
    const isCurrent = beginOperation(externalCurrent);
    const rendered = await renderPet(pet, isCurrent);
    return isCurrent() && rendered !== false;
  }

  function cancel() {
    generation += 1;
  }

  function clear() {
    cancel();
    clearPet();
  }

  function destroy() {
    if (destroyed) return;
    destroyed = true;
    generation += 1;
    destroyPet();
  }

  return { cancel, clear, destroy, preview, select };
}
