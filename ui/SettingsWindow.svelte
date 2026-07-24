<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import SettingsPanel from "./SettingsPanel.svelte";
  import { createPetCatalogController } from "./lib/pet-catalog-controller.js";
  import { synchronizePetSelection } from "./lib/pet-catalog.js";
  import { ONBOARDING_KEY, SELECTED_PET_KEY } from "./lib/storage-keys.js";
  import { createTransientMessage } from "./lib/transient-message.js";

  const settingsWindow = getCurrentWindow();
  let runtime = { connected: false };
  let pets = [];
  let catalogIssues = [];
  let selectedPet = localStorage.getItem(SELECTED_PET_KEY) || "";
  let onboardingVisible = localStorage.getItem(ONBOARDING_KEY) !== "true";
  let importPreview = null;
  let importSourcePath = "";
  let importRequestGeneration = 0;
  let managementNotice = "";
  let actionError = "";
  let submitting = "";
  const actionErrorMessage = createTransientMessage({
    durationMs: 5000,
    onChange: (value) => (actionError = value),
  });
  const catalog = createPetCatalogController({
    initialSelectedId: selectedPet,
    listPets: () => invoke("list_pets"),
    persistSelected: persistSelectedPet,
    onChange: (next) => {
      pets = next.pets;
      catalogIssues = next.issues;
      selectedPet = next.selectedId;
    },
    onError: showActionError,
  });

  function clearActionError() {
    actionErrorMessage.clear();
  }

  function showActionError(cause) {
    actionErrorMessage.show(cause);
  }

  function completeOnboarding() {
    onboardingVisible = false;
    localStorage.setItem(ONBOARDING_KEY, "true");
  }

  function persistSelectedPet(id) {
    if (id) localStorage.setItem(SELECTED_PET_KEY, id);
    else localStorage.removeItem(SELECTED_PET_KEY);
    void synchronizePetSelection(true, id, emitTo).then((error) => {
      if (error) showActionError(error);
    });
  }

  function invalidatePetImport() {
    const hadPreview = Boolean(importPreview);
    importRequestGeneration += 1;
    importPreview = null;
    importSourcePath = "";
    return hadPreview;
  }

  async function refreshPets(preferredId = "") {
    invalidatePetImport();
    clearActionError();
    await catalog.refresh(preferredId);
  }

  async function rescanPets() {
    await refreshPets();
    try {
      await emitTo("pet", "pet-catalog-changed", {
        preferredId: selectedPet,
        forceReload: true,
      });
    } catch (cause) {
      showActionError(cause);
    }
  }

  async function beginPetImport(kind) {
    invalidatePetImport();
    const requestGeneration = importRequestGeneration;
    clearActionError();
    managementNotice = "";
    submitting = "pet-import";
    try {
      const sourcePath = await open(kind === "folder"
        ? { directory: true, multiple: false, title: "Choose a Pet package folder" }
        : {
            directory: false,
            multiple: false,
            title: "Choose a Pet package",
            filters: [{ name: "Pet package", extensions: ["zip", "json"] }],
          });
      if (requestGeneration !== importRequestGeneration || !sourcePath || Array.isArray(sourcePath)) return;
      const preview = await invoke("preview_pet_import", { sourcePath });
      if (requestGeneration !== importRequestGeneration) return;
      importSourcePath = sourcePath;
      importPreview = preview;
    } catch (cause) {
      if (requestGeneration === importRequestGeneration) showActionError(cause);
    } finally {
      if (submitting === "pet-import") submitting = "";
    }
  }

  function cancelPetImport() {
    invalidatePetImport();
  }

  async function installPetImport() {
    if (!importPreview || !importSourcePath) return;
    const replace = importPreview.targetExists;
    if (replace) {
      const accepted = await confirm(
        `Replace the installed “${importPreview.pet.displayName}” package?`,
        { title: "Replace pet", kind: "warning", okLabel: "Replace", cancelLabel: "Cancel" },
      );
      if (!accepted) return;
    }
    submitting = "pet-install";
    clearActionError();
    try {
      const result = await invoke("install_pet", { sourcePath: importSourcePath, replace });
      invalidatePetImport();
      managementNotice = result.replaced ? "Pet replaced." : "Pet installed.";
      completeOnboarding();
      await refreshPets(result.pet.id);
      await emitTo("pet", "pet-catalog-changed", {
        preferredId: result.pet.id,
        forceReload: true,
      });
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function removeSelectedPet() {
    const pet = pets.find((candidate) => candidate.id === selectedPet);
    if (!pet) return;
    const accepted = await confirm(
      `Remove “${pet.displayName}” from this Mac?`,
      { title: "Remove pet", kind: "warning", okLabel: "Remove", cancelLabel: "Cancel" },
    );
    if (!accepted) return;
    submitting = "pet-remove";
    clearActionError();
    try {
      const result = await invoke("remove_pet", { id: selectedPet });
      invalidatePetImport();
      managementNotice = "Pet removed.";
      await catalog.applyCatalog(result.catalog);
      await emitTo("pet", "pet-catalog-changed", { preferredId: selectedPet });
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function openPetsFolder() {
    clearActionError();
    try {
      await invoke("open_pets_folder");
    } catch (cause) {
      showActionError(cause);
    }
  }

  async function resetWindowPlacement() {
    submitting = "window-reset";
    clearActionError();
    try {
      await emitTo("pet", "reset-pet-window");
      managementNotice = "Pet window reset.";
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  function closeSettings() {
    completeOnboarding();
    invalidatePetImport();
    void settingsWindow.close().catch(showActionError);
  }

  onMount(() => {
    let unlistenPet = () => {};
    let unlistenSelection = () => {};
    let unlistenRefresh = () => {};
    let disposed = false;
    const retainUnlisten = async (registration) => {
      const unlisten = await registration;
      if (!disposed) return unlisten;
      unlisten();
      return () => {};
    };
    (async () => {
      const initialRuntime = await invoke("get_runtime_state");
      if (disposed) return;
      runtime = initialRuntime;
      unlistenPet = await retainUnlisten(listen("pet-state", ({ payload }) => {
        runtime = payload;
      }));
      if (disposed) return;
      unlistenSelection = await retainUnlisten(listen("pet-selection-changed", ({ payload }) => {
        void catalog.acceptExternalSelection(payload?.id || "");
      }));
      if (disposed) return;
      unlistenRefresh = await retainUnlisten(listen("refresh-settings", () => {
        void refreshPets();
      }));
      if (disposed) return;
      await refreshPets();
    })().catch((cause) => {
      if (!disposed) showActionError(cause);
    });
    return () => {
      disposed = true;
      unlistenPet();
      unlistenSelection();
      unlistenRefresh();
      invalidatePetImport();
      actionErrorMessage.destroy();
      catalog.destroy();
    };
  });
</script>

<main class="settings-window">
  <SettingsPanel
    connected={runtime.connected}
    {onboardingVisible}
    {pets}
    {catalogIssues}
    selectedPet={selectedPet}
    {importPreview}
    {managementNotice}
    {actionError}
    {submitting}
    onClose={closeSettings}
    onCompleteOnboarding={completeOnboarding}
    onSelectPet={(id) => catalog.select(id)}
    onRefreshPets={rescanPets}
    onBeginPetImport={beginPetImport}
    onCancelPetImport={cancelPetImport}
    onInstallPetImport={installPetImport}
    onClearActionError={clearActionError}
    onOpenPetsFolder={openPetsFolder}
    onRemoveSelectedPet={removeSelectedPet}
    onResetWindowPlacement={resetWindowPlacement}
  />
</main>
