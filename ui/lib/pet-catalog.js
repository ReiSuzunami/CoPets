export function selectPetFromCatalog(pets, currentId = "", preferredId = "") {
  if (preferredId && pets.some((pet) => pet.id === preferredId)) return preferredId;
  if (currentId && pets.some((pet) => pet.id === currentId)) return currentId;
  return pets[0]?.id || "";
}

export function petSelectionSync(isSettingsWindow, id) {
  return isSettingsWindow
    ? { target: "pet", event: "pet-selection-changed", payload: { id } }
    : { target: "settings", event: "pet-selection-changed", payload: { id } };
}

export const PET_SELECTION_SYNC_ERROR = "Pet selection could not be synchronized.";

export async function synchronizePetSelection(isSettingsWindow, id, emit) {
  const { target, event, payload } = petSelectionSync(isSettingsWindow, id);
  try {
    await emit(target, event, payload);
    return "";
  } catch {
    return PET_SELECTION_SYNC_ERROR;
  }
}
