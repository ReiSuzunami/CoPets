import { selectPetFromCatalog } from "./pet-catalog.js";

function normalizeCatalog(catalog) {
  return {
    pets: Array.isArray(catalog?.pets) ? catalog.pets : [],
    issues: Array.isArray(catalog?.issues) ? catalog.issues : [],
  };
}

export function createPetCatalogController({
  initialSelectedId = "",
  listPets,
  presentation = null,
  persistSelected,
  onChange = () => {},
  onError = () => {},
}) {
  let state = {
    pets: [],
    issues: [],
    selectedId: initialSelectedId,
  };
  let renderedId = "";
  let committedId = null;
  let generation = 0;
  let destroyed = false;

  const publish = () => onChange({ ...state, pets: [...state.pets], issues: [...state.issues] });

  function snapshot() {
    return { ...state, pets: [...state.pets], issues: [...state.issues] };
  }

  function commitSelection(id) {
    state = { ...state, selectedId: id };
    publish();
    if (committedId === id) return;
    committedId = id;
    persistSelected(id);
  }

  async function renderSelection(id, operation, forceReload = false) {
    if (!presentation) {
      commitSelection(id);
      return true;
    }
    if (renderedId === id && !forceReload) {
      commitSelection(id);
      return true;
    }
    const selected = await presentation.select(id);
    if (destroyed || operation !== generation || !selected) return false;
    renderedId = id;
    commitSelection(id);
    return true;
  }

  async function applyCatalog(catalog, preferredId = "", options = {}) {
    if (destroyed) return snapshot();
    const operation = options.operation ?? ++generation;
    if (operation !== generation) return snapshot();
    presentation?.cancel();
    const normalized = normalizeCatalog(catalog);
    state = { ...state, pets: normalized.pets, issues: normalized.issues };
    publish();

    const nextId = selectPetFromCatalog(state.pets, state.selectedId, preferredId);
    if (nextId) {
      await renderSelection(nextId, operation, Boolean(options.forceReload));
      return snapshot();
    }

    if (presentation && (renderedId || state.selectedId)) presentation.clear();
    renderedId = "";
    commitSelection("");
    return snapshot();
  }

  async function refresh(preferredId = "", options = {}) {
    if (destroyed) return snapshot();
    const operation = ++generation;
    presentation?.cancel();
    try {
      const nextCatalog = await listPets();
      if (destroyed || operation !== generation) return snapshot();
      return await applyCatalog(nextCatalog, preferredId, { ...options, operation });
    } catch (cause) {
      if (!destroyed && operation === generation) onError(String(cause));
      return snapshot();
    }
  }

  async function select(id) {
    if (destroyed || !state.pets.some((pet) => pet.id === id)) return false;
    const operation = ++generation;
    presentation?.cancel();
    return renderSelection(id, operation);
  }

  async function acceptExternalSelection(id) {
    if (destroyed) return false;
    committedId = id;
    state = { ...state, selectedId: id };
    publish();
    if (!presentation || !state.pets.some((pet) => pet.id === id)) return true;
    if (renderedId === id) return true;
    const operation = ++generation;
    presentation.cancel();
    const selected = await presentation.select(id);
    if (destroyed || operation !== generation || !selected) return false;
    renderedId = id;
    return true;
  }

  function cancel() {
    generation += 1;
    presentation?.cancel();
  }

  function destroy() {
    if (destroyed) return;
    destroyed = true;
    generation += 1;
    presentation?.destroy?.();
  }

  return {
    acceptExternalSelection,
    applyCatalog,
    cancel,
    destroy,
    refresh,
    select,
    snapshot,
  };
}
