<script>
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import MoveDiagonal2 from "@lucide/svelte/icons/move-diagonal-2";
  import Settings2 from "@lucide/svelte/icons/settings-2";
  import Square from "@lucide/svelte/icons/square";
  import X from "@lucide/svelte/icons/x";
  import {
    LogicalSize,
    PhysicalPosition,
    PhysicalSize,
    availableMonitors,
    currentMonitor,
    cursorPosition,
    getCurrentWindow,
    primaryMonitor,
  } from "@tauri-apps/api/window";
  import SettingsPanel from "./SettingsPanel.svelte";
  import { createDragMotionController } from "./lib/drag-motion.js";
  import { createDragPointerTracker } from "./lib/drag-pointer.js";
  import { markVisibleOverflow } from "./lib/bubble-overflow.js";
  import {
    createConversationBubbleQueue,
    createTerminalPresentationController,
    terminalKey,
  } from "./lib/conversation-display.js";
  import { normalizeControlAnswers, prepareFollowUp, visibleAnswer } from "./lib/control-input.js";
  import { shouldShowFollowUp } from "./lib/follow-up-visibility.js";
  import { createPetCatalogController } from "./lib/pet-catalog-controller.js";
  import { synchronizePetSelection } from "./lib/pet-catalog.js";
  import { createPetPresentation } from "./lib/pet-presentation.js";
  import { renderMarkdown } from "./lib/markdown.js";
  import { createMotionPreference } from "./lib/motion-preference.js";
  import { isTerminalState, labelForState } from "./lib/pet.js";
  import { PixiPet } from "./lib/pixi-pet.js";
  import {
    CDP_CUSTOM_PORT_KEY,
    CDP_PORT_MODE_KEY,
    ONBOARDING_KEY,
    SELECTED_PET_KEY,
    WINDOW_STATE_KEY,
  } from "./lib/storage-keys.js";
  import {
    CDP_PORT_MODE_AUTOMATIC,
    normalizeCdpPortMode,
    parseCustomCdpPort,
  } from "./lib/cdp-bridge-settings.js";
  import {
    centerWindowRect,
    createCornerResizeController,
    fitWindowRect,
    normalizeMonitor,
    selectBestMonitor,
  } from "./lib/window-resize.js";
  import { createTransientMessage } from "./lib/transient-message.js";

  let stage;
  let renderer;
  let pets = [];
  let catalogIssues = [];
  let selectedPet = localStorage.getItem(SELECTED_PET_KEY) || "";
  let onboardingVisible = localStorage.getItem(ONBOARDING_KEY) !== "true";
  let importPreview = null;
  let importSourcePath = "";
  let importRequestGeneration = 0;
  let managementNotice = "";
  let runtime = {
    state: "disconnected",
    connected: false,
    threadIdHash: null,
    epoch: 0,
    currentQuestion: null,
    taskSummary: null,
    latestUpdate: null,
  };
  let displayState = runtime.state;
  let conversationBubbles = [];
  let bubblesLeaving = false;
  let reducedMotion = false;
  let control = {
    canStop: false,
    canReply: false,
    canStartFollowUp: false,
    showWorkingFollowUp: false,
    showReadyFollowUp: false,
    transport: "ipcOnly",
    notifications: [],
  };
  $: followUpVisible = shouldShowFollowUp(control);
  const ACTION_ERROR_TIMEOUT_MS = 5000;
  let actionError = "";
  const actionErrorMessage = createTransientMessage({
    durationMs: ACTION_ERROR_TIMEOUT_MS,
    onChange: (value) => (actionError = value),
  });
  let submitting = "";
  let cdpPortMode = normalizeCdpPortMode(localStorage.getItem(CDP_PORT_MODE_KEY));
  let cdpCustomPort = localStorage.getItem(CDP_CUSTOM_PORT_KEY) || "";
  let controlsOpen = false;
  let followUpOpen = false;
  const petWindow = getCurrentWindow();
  let settingsOpen = false;
  let followUp = "";
  let answers = {};
  let pointerOverWindow = false;
  const DEFAULT_WINDOW_WIDTH = 360;
  const DEFAULT_WINDOW_HEIGHT = 480;
  let saveWindowTimer;
  let saveWindowGeneration = 0;
  let dragging = false;
  let resizing = false;
  const settledTerminalKeys = new Set();
  const conversationBubbleQueue = createConversationBubbleQueue();
  const petPresentation = createPetPresentation({
    fetchPet: (id) => invoke("load_pet", { id }),
    renderPet: (pet, isCurrent) => renderer.load(pet, isCurrent),
    clearPet: () => renderer?.clear(),
    destroyPet: () => renderer?.destroy(),
    onSelected: () => {},
    onError: showActionError,
  });
  const petCatalog = createPetCatalogController({
    initialSelectedId: selectedPet,
    listPets: () => invoke("list_pets"),
    presentation: petPresentation,
    persistSelected: persistSelectedPet,
    onChange: (next) => {
      pets = next.pets;
      catalogIssues = next.issues;
      selectedPet = next.selectedId;
    },
    onError: showActionError,
  });
  const dragMotion = createDragMotionController({
    setDirection: (direction) => renderer?.setDragDirection(direction),
    restore: () => renderer?.restoreStateAnimation(),
    onActiveChange: (active) => dragging = active,
  });
  const dragPointer = createDragPointerTracker({
    readSnapshot: () => invoke("get_drag_pointer_snapshot"),
    onMove: (position) => dragMotion.move(position),
    onRelease: stopWindowDrag,
    onError: showActionError,
  });
  const resizeController = createCornerResizeController({
    readInitialGeometry: async () => {
      const [position, size, pointer, monitors] = await Promise.all([
        petWindow.outerPosition(),
        petWindow.outerSize(),
        cursorPosition(),
        availableMonitors(),
      ]);
      const rect = { x: position.x, y: position.y, width: size.width, height: size.height };
      const monitor = selectBestMonitor(rect, monitors) || (await primaryMonitor()) || monitors[0];
      if (!monitor) throw new Error("No display is available for window resizing.");
      return {
        rect,
        pointer: { x: pointer.x, y: pointer.y },
        monitor: normalizeMonitor(monitor),
      };
    },
    readPointer: () => cursorPosition(),
    applySize: (rect) => petWindow.setSize(new PhysicalSize(rect.width, rect.height)),
    onActiveChange: (active) => (resizing = active),
    onCommit: scheduleWindowStateSave,
    onError: showActionError,
  });
  const terminalPresentation = createTerminalPresentationController({
    onExit: (key) => {
      if (key === terminalKey(runtime)) bubblesLeaving = true;
    },
    onSettle: (key) => {
      if (key !== terminalKey(runtime)) return;
      if (settledTerminalKeys.size >= 64) settledTerminalKeys.clear();
      settledTerminalKeys.add(key);
      conversationBubbleQueue.reset();
      conversationBubbles = [];
      bubblesLeaving = false;
      displayState = "idle";
      if (!dragging) renderer?.setState("idle");
    },
  });

  function clearActionError() {
    actionErrorMessage.clear();
  }

  function showActionError(cause) {
    actionErrorMessage.show(cause);
  }

  function openFollowUp() {
    clearActionError();
    discardPetImportPreview();
    followUpOpen = !followUpOpen;
    controlsOpen = false;
    settingsOpen = false;
  }

  function toggleControls() {
    clearActionError();
    discardPetImportPreview();
    controlsOpen = !controlsOpen;
    followUpOpen = false;
    settingsOpen = false;
  }

  async function openInlineSettings() {
    clearActionError();
    try {
      await invoke("close_settings_window");
    } catch (cause) {
      showActionError(cause);
      return;
    }
    settingsOpen = true;
    controlsOpen = false;
    followUpOpen = false;
  }

  function completeOnboarding() {
    onboardingVisible = false;
    localStorage.setItem(ONBOARDING_KEY, "true");
  }

  function invalidatePetImport() {
    const hadPreview = Boolean(importPreview);
    const hadImportOperation = hadPreview || Boolean(importSourcePath) || submitting === "pet-import";
    importRequestGeneration += 1;
    importPreview = null;
    importSourcePath = "";
    if (hadImportOperation) petPresentation.cancel();
    return hadPreview;
  }

  function discardPetImportPreview() {
    if (!invalidatePetImport()) return;
    if (selectedPet) void petPresentation.select(selectedPet);
    else petPresentation.clear();
  }

  function closeSettings() {
    completeOnboarding();
    discardPetImportPreview();
    settingsOpen = false;
  }

  function persistSelectedPet(id) {
    clearActionError();
    if (id) localStorage.setItem(SELECTED_PET_KEY, id);
    else localStorage.removeItem(SELECTED_PET_KEY);
    void synchronizePetSelection(false, id, emitTo).then((error) => {
      if (error) showActionError(error);
    });
  }

  function setCdpPortMode(mode) {
    cdpPortMode = normalizeCdpPortMode(mode);
    localStorage.setItem(CDP_PORT_MODE_KEY, cdpPortMode);
  }

  function setCdpCustomPort(value) {
    cdpCustomPort = String(value ?? "").replace(/\D/g, "").slice(0, 5);
    if (cdpCustomPort) localStorage.setItem(CDP_CUSTOM_PORT_KEY, cdpCustomPort);
    else localStorage.removeItem(CDP_CUSTOM_PORT_KEY);
  }

  async function launchCdpBridge() {
    const customPort = cdpPortMode === CDP_PORT_MODE_AUTOMATIC
      ? null
      : parseCustomCdpPort(cdpCustomPort);
    if (cdpPortMode !== CDP_PORT_MODE_AUTOMATIC && customPort === null) {
      showActionError("Choose a local port from 1024 to 65535.");
      return;
    }
    submitting = "cdp-bridge";
    clearActionError();
    managementNotice = "";
    try {
      await invoke("launch_codex_with_cdp", { customPort });
      managementNotice = "Codex bridge ready for this CoPets session.";
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function restartCodexWithBridge() {
    const customPort = cdpPortMode === CDP_PORT_MODE_AUTOMATIC
      ? null
      : parseCustomCdpPort(cdpCustomPort);
    if (cdpPortMode !== CDP_PORT_MODE_AUTOMATIC && customPort === null) {
      showActionError("Choose a local port from 1024 to 65535.");
      return;
    }
    const accepted = await confirm(
      "CoPets will close the current Codex App and reopen it with a local bridge. Active work may be interrupted and unsaved UI state may be lost.",
      {
        title: "Restart Codex with bridge?",
        kind: "warning",
        okLabel: "Restart Codex",
        cancelLabel: "Cancel",
      },
    );
    if (!accepted) return;
    submitting = "cdp-restart";
    clearActionError();
    managementNotice = "";
    try {
      await invoke("restart_codex_with_cdp", { customPort });
      managementNotice = "Codex restarted with bridge.";
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function connectExistingCdp() {
    const port = cdpPortMode === CDP_PORT_MODE_AUTOMATIC
      ? null
      : parseCustomCdpPort(cdpCustomPort);
    if (cdpPortMode !== CDP_PORT_MODE_AUTOMATIC && port === null) {
      showActionError("Enter the local Codex CDP port from 1024 to 65535.");
      return;
    }
    submitting = "cdp-connect";
    clearActionError();
    managementNotice = "";
    try {
      await invoke("connect_existing_codex_cdp", { port });
      managementNotice = "Connected to the existing Codex bridge.";
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function retryCdpVerification() {
    submitting = "cdp-verify";
    clearActionError();
    managementNotice = "";
    try {
      await invoke("retry_cdp_bridge");
      managementNotice = "Codex bridge ready for this CoPets session.";
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function refreshPets(preferredId = "", options = {}) {
    const hadPreview = invalidatePetImport();
    clearActionError();
    await petCatalog.refresh(preferredId, {
      forceReload: Boolean(options.forceReload || hadPreview),
    });
  }

  async function beginPetImport(kind) {
    const hadPreview = invalidatePetImport();
    if (hadPreview) {
      if (selectedPet) void petPresentation.select(selectedPet);
      else petPresentation.clear();
    }
    const requestGeneration = importRequestGeneration;
    clearActionError();
    managementNotice = "";
    submitting = "pet-import";
    try {
      const sourcePath = await open(kind === "folder"
        ? {
            directory: true,
            multiple: false,
            title: "Choose a Pet package folder",
          }
        : {
            directory: false,
            multiple: false,
            title: "Choose a Pet package",
            filters: [{ name: "Pet package", extensions: ["zip", "json"] }],
          });
      if (
        requestGeneration !== importRequestGeneration
        || !sourcePath
        || Array.isArray(sourcePath)
      ) return;
      const preview = await invoke("preview_pet_import", { sourcePath });
      if (requestGeneration !== importRequestGeneration) return;
      importSourcePath = sourcePath;
      importPreview = preview;
      await petPresentation.preview(
        preview.pet,
        () => requestGeneration === importRequestGeneration,
      );
    } catch (cause) {
      if (requestGeneration === importRequestGeneration) showActionError(cause);
    } finally {
      if (submitting === "pet-import") submitting = "";
    }
  }

  async function cancelPetImport() {
    if (!invalidatePetImport()) return;
    if (selectedPet) await petPresentation.select(selectedPet);
    else petPresentation.clear();
  }

  async function selectPet(id) {
    invalidatePetImport();
    await petCatalog.select(id);
  }

  async function installPetImport() {
    if (!importPreview || !importSourcePath) return;
    const replace = importPreview.targetExists;
    if (replace) {
      const accepted = await confirm(
        `Replace the installed “${importPreview.pet.displayName}” package?`,
        {
          title: "Replace pet",
          kind: "warning",
          okLabel: "Replace",
          cancelLabel: "Cancel",
        },
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
      {
        title: "Remove pet",
        kind: "warning",
        okLabel: "Remove",
        cancelLabel: "Cancel",
      },
    );
    if (!accepted) return;
    submitting = "pet-remove";
    clearActionError();
    try {
      const result = await invoke("remove_pet", { id: selectedPet });
      invalidatePetImport();
      managementNotice = "Pet removed.";
      await petCatalog.applyCatalog(result.catalog);
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
      const monitor = (await currentMonitor()) || (await primaryMonitor()) || (await availableMonitors())[0];
      await petWindow.setSize(new LogicalSize(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT));
      const size = await petWindow.outerSize();
      if (monitor) {
        const centered = centerWindowRect(size, monitor);
        if (centered.width !== size.width || centered.height !== size.height) {
          await petWindow.setSize(new PhysicalSize(centered.width, centered.height));
        }
        await petWindow.setPosition(new PhysicalPosition(centered.x, centered.y));
      }
      localStorage.removeItem(WINDOW_STATE_KEY);
      scheduleWindowStateSave();
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  function stopWindowDrag() {
    dragPointer.stop();
    dragMotion.stop();
    renderer?.setState(displayState);
  }

  function startWindowDrag(event) {
    if (event.button !== 0) return;
    dragMotion.start();
    dragPointer.start();
    petWindow.startDragging().catch((cause) => {
      stopWindowDrag();
      showActionError(cause);
    });
  }

  function startWindowResize(event) {
    if (event.button !== 0) return;
    const target = event.currentTarget;
    void resizeController.start({
      pointerId: event.pointerId,
      capture: () => target.setPointerCapture?.(event.pointerId),
      release: () => {
        if (target.hasPointerCapture?.(event.pointerId)) target.releasePointerCapture(event.pointerId);
      },
    });
  }

  function updateWindowResize(event) {
    void resizeController.move(event.pointerId);
  }

  function stopWindowResize() {
    resizeController.stop();
  }

  async function restoreWindowState() {
    let saved;
    try {
      saved = JSON.parse(localStorage.getItem(WINDOW_STATE_KEY) || "null");
    } catch {
      return;
    }
    if (!saved || ![saved.x, saved.y, saved.width, saved.height].every(Number.isFinite)) return;
    const monitors = await availableMonitors();
    if (!monitors.length) return;
    const savedRect = { x: saved.x, y: saved.y, width: saved.width, height: saved.height };
    const monitor = selectBestMonitor(savedRect, monitors) || (await primaryMonitor()) || monitors[0];
    const fitted = fitWindowRect(savedRect, monitor);
    await petWindow.setSize(new PhysicalSize(fitted.width, fitted.height));
    await petWindow.setPosition(new PhysicalPosition(fitted.x, fitted.y));
  }

  function scheduleWindowStateSave() {
    clearTimeout(saveWindowTimer);
    const generation = ++saveWindowGeneration;
    saveWindowTimer = setTimeout(async () => {
      try {
        const [position, size] = await Promise.all([petWindow.outerPosition(), petWindow.outerSize()]);
        if (generation !== saveWindowGeneration) return;
        localStorage.setItem(WINDOW_STATE_KEY, JSON.stringify({
          x: position.x,
          y: position.y,
          width: size.width,
          height: size.height,
        }));
      } catch {
        // Window persistence is best-effort and must never block the pet UI.
      }
    }, 180);
  }

  function handleWindowMoved() {
    scheduleWindowStateSave();
  }

  function applyRuntimeSnapshot(nextRuntime) {
    terminalPresentation.cancel();
    runtime = nextRuntime;
    bubblesLeaving = false;
    const key = terminalKey(nextRuntime);
    if (isTerminalState(nextRuntime.state) && settledTerminalKeys.has(key)) {
      displayState = "idle";
      conversationBubbles = [];
    } else {
      displayState = nextRuntime.state;
      conversationBubbles = conversationBubbleQueue.update(nextRuntime);
      terminalPresentation.schedule(
        nextRuntime,
        reducedMotion,
      );
    }
    if (!dragging) renderer?.setState(displayState);
  }

  function streamBubbleContent(node, initialBubble) {
    let frame;
    let resizeFrame;
    let bubble;
    let completeText = "";
    let renderedText = "";
    let characters = [];
    let cursor = 0;
    const render = () => {
      node.innerHTML = renderMarkdown(renderedText);
      const overflowing = node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1;
      if (overflowing) markVisibleOverflow(node);
    };
    const scheduleBubbleRender = () => {
      cancelAnimationFrame(resizeFrame);
      resizeFrame = requestAnimationFrame(render);
    };
    const reveal = () => {
      if (reducedMotion || bubble.side !== "codex") {
        cursor = characters.length;
      } else {
        cursor = Math.min(
          characters.length,
          cursor + Math.max(1, Math.ceil(characters.length / 36)),
        );
      }
      renderedText = characters.slice(0, cursor).join("");
      render();
      if (cursor < characters.length) frame = requestAnimationFrame(reveal);
    };
    const updateBubble = (nextBubble) => {
      if (bubble?.id === nextBubble.id && completeText === nextBubble.text) return;
      const continuingBubble = bubble?.id === nextBubble.id;
      const canContinue = continuingBubble && nextBubble.text.startsWith(renderedText);
      cancelAnimationFrame(frame);
      bubble = nextBubble;
      completeText = nextBubble.text;
      characters = Array.from(completeText);
      cursor = canContinue ? Array.from(renderedText).length : 0;
      reveal();
    };
    window.addEventListener("resize", scheduleBubbleRender);
    updateBubble(initialBubble);
    return {
      update: updateBubble,
      destroy() {
        cancelAnimationFrame(frame);
        cancelAnimationFrame(resizeFrame);
        window.removeEventListener("resize", scheduleBubbleRender);
      },
    };
  }

  function setAnswer(notificationId, questionId, value) {
    answers = {
      ...answers,
      [notificationId]: {
        ...(answers[notificationId] || {}),
        [questionId]: value,
      },
    };
  }

  async function performAction(notification, action) {
    submitting = notification.id;
    clearActionError();
    const notificationAnswers = answers[notification.id] || {};
    const normalizedAnswers = normalizeControlAnswers(notificationAnswers);
    try {
      await invoke("perform_control_action", {
        input: {
          actionId: notification.id,
          action,
          answers: normalizedAnswers,
        },
      });
      const next = { ...answers };
      delete next[notification.id];
      answers = next;
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function dismiss(notification) {
    clearActionError();
    try {
      await invoke("dismiss_control_notification", { actionId: notification.id });
    } catch (cause) {
      showActionError(cause);
    }
  }

  async function stopTask() {
    submitting = "stop";
    clearActionError();
    try {
      await invoke("stop_current_task");
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  async function submitFollowUp() {
    const prompt = prepareFollowUp(followUp);
    if (!prompt) return;
    submitting = "follow-up";
    clearActionError();
    try {
      await invoke("send_follow_up", { prompt });
      followUp = "";
      followUpOpen = false;
    } catch (cause) {
      showActionError(cause);
    } finally {
      submitting = "";
    }
  }

  onMount(() => {
    let unlistenPet = () => {};
    let unlistenControl = () => {};
    let unlistenMoved = () => {};
    let unlistenResized = () => {};
    let unlistenWindowHover = () => {};
    let unlistenCloseInlineSettings = () => {};
    let unlistenPetCatalog = () => {};
    let unlistenPetSelection = () => {};
    let unlistenResetWindow = () => {};
    let disposed = false;
    const retainUnlisten = async (registration) => {
      const unlisten = await registration;
      if (!disposed) return unlisten;
      unlisten();
      return () => {};
    };
    const motionPreference = createMotionPreference({
      matchMedia: window.matchMedia.bind(window),
      onChange: (matches) => {
        reducedMotion = matches;
        renderer?.setReducedMotion(matches);
      },
    });
    (async () => {
      unlistenCloseInlineSettings = await retainUnlisten(listen("close-inline-settings", () => {
        discardPetImportPreview();
        settingsOpen = false;
      }));
      if (disposed) return;
      unlistenPetCatalog = await retainUnlisten(listen("pet-catalog-changed", ({ payload }) => {
        void refreshPets(payload?.preferredId || "", {
          forceReload: Boolean(payload?.forceReload),
        });
      }));
      if (disposed) return;
      unlistenPetSelection = await retainUnlisten(listen("pet-selection-changed", ({ payload }) => {
        void petCatalog.acceptExternalSelection(payload?.id || "");
      }));
      if (disposed) return;
      unlistenResetWindow = await retainUnlisten(listen("reset-pet-window", () => {
        void resetWindowPlacement();
      }));
      if (disposed) return;
      await restoreWindowState();
      if (disposed) return;
      unlistenMoved = await retainUnlisten(petWindow.onMoved(handleWindowMoved));
      if (disposed) return;
      unlistenResized = await retainUnlisten(petWindow.onResized(scheduleWindowStateSave));
      if (disposed) return;
      unlistenWindowHover = await retainUnlisten(listen("pet-window-hover", ({ payload }) => {
        pointerOverWindow = Boolean(payload);
      }));
      if (disposed) return;
      const hovering = await invoke("get_window_hover_state");
      if (disposed) return;
      pointerOverWindow = Boolean(hovering);
      const nextRenderer = new PixiPet(stage, { reducedMotion });
      await nextRenderer.init();
      if (disposed) {
        nextRenderer.destroy();
        return;
      }
      nextRenderer.setReducedMotion(reducedMotion);
      renderer = nextRenderer;
      await refreshPets();
      if (disposed) return;
      if (onboardingVisible || !pets.length || catalogIssues.length) settingsOpen = true;
      const initialRuntime = await invoke("get_runtime_state");
      if (disposed) return;
      applyRuntimeSnapshot(initialRuntime);
      const initialControl = await invoke("get_control_state");
      if (disposed) return;
      control = initialControl;
      unlistenPet = await retainUnlisten(listen("pet-state", ({ payload }) => {
        applyRuntimeSnapshot(payload);
      }));
      if (disposed) return;
      unlistenControl = await retainUnlisten(listen("control-state", ({ payload }) => {
        control = payload;
        if (!shouldShowFollowUp(payload)) {
          followUpOpen = false;
          followUp = "";
        }
      }));
    })().catch((cause) => {
      if (!disposed) showActionError(cause);
    });
    return () => {
      disposed = true;
      unlistenPet();
      unlistenControl();
      unlistenMoved();
      unlistenResized();
      unlistenWindowHover();
      unlistenCloseInlineSettings();
      unlistenPetCatalog();
      unlistenPetSelection();
      unlistenResetWindow();
      clearTimeout(saveWindowTimer);
      saveWindowGeneration += 1;
      actionErrorMessage.destroy();
      motionPreference.destroy();
      terminalPresentation.cancel();
      petCatalog.destroy();
      resizeController.destroy();
      dragPointer.stop();
      dragMotion.destroy();
    };
  });
</script>

<svelte:window
  on:pointermove={updateWindowResize}
  on:pointerup={() => { stopWindowDrag(); stopWindowResize(); }}
  on:pointercancel={() => { stopWindowDrag(); stopWindowResize(); }}
/>

<main
  class:disconnected={!runtime.connected}
  class:resizing
  class:pointer-over-window={pointerOverWindow}
>
  <div class="stage" bind:this={stage}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="pet-drag-surface"
    aria-hidden="true"
    on:pointerdown|preventDefault={startWindowDrag}
    on:pointerup={stopWindowDrag}
    on:pointercancel={stopWindowDrag}
  ></div>

  {#if !control.notifications.length && conversationBubbles.length}
    <section
      class="conversation-bubbles"
      class:leaving={bubblesLeaving}
      aria-label="Current Codex conversation"
      aria-live="polite"
    >
      {#each conversationBubbles as bubble (bubble.id)}
        <article
          class="speech-bubble {bubble.side}"
          class:grouped-previous={bubble.groupedWithPrevious}
          class:grouped-next={bubble.groupedWithNext}
          aria-label={`${bubble.side === "user" ? "You" : "Codex"}: ${bubble.text}`}
          animate:flip={{ duration: reducedMotion ? 0 : 200 }}
          in:fly={{ y: reducedMotion ? 0 : 12, duration: reducedMotion ? 0 : 180 }}
          out:fly={{ y: reducedMotion ? 0 : -10, duration: reducedMotion ? 0 : 140 }}
        >
          <div class="bubble-content" aria-hidden="true" use:streamBubbleContent={bubble}></div>
          {#if bubble.showArrow}
            <svg class="bubble-tail" viewBox="0 0 18 16" aria-hidden="true" focusable="false">
              <path d="M18 0h-7.8c-.5 5.5-3.4 10.6-9.2 15.3 8.1-.8 13.8-3.6 17-8.5V0Z"></path>
            </svg>
          {/if}
        </article>
      {/each}
    </section>
  {/if}

  {#if control.notifications.length}
    <section class="control-stack" aria-label="Codex requests" aria-live="polite">
      {#each control.notifications as notification (notification.id)}
        <article class="request-card" data-kind={notification.kind}>
          <header>
            <span class="request-mark" aria-hidden="true"></span>
            <div>
              <h2>{notification.title}</h2>
              <p>{notification.summary}</p>
            </div>
            <button
              class="icon-button dismiss"
              type="button"
              aria-label={`Dismiss ${notification.title}`}
              title="Dismiss"
              on:click={() => dismiss(notification)}
            >×</button>
          </header>

          {#if notification.kind === "question"}
            <div class="questions">
              {#each notification.questions as question (question.id)}
                <fieldset>
                  <legend>{question.header}</legend>
                  <p>{question.prompt}</p>
                  {#if question.options.length}
                    <div class="option-list">
                      {#each question.options as option}
                        <button
                          type="button"
                          class:chosen={answers[notification.id]?.[question.id] === option.id}
                          aria-pressed={answers[notification.id]?.[question.id] === option.id}
                          disabled={submitting === notification.id}
                          on:click={() => setAnswer(notification.id, question.id, option.id)}
                        >
                          <strong>{option.label}</strong>
                          {#if option.description}<small>{option.description}</small>{/if}
                        </button>
                      {/each}
                    </div>
                  {/if}
                  {#if question.allowOther || !question.options.length}
                    <input
                      aria-label={`Answer: ${question.prompt}`}
                      placeholder="Type an answer"
                      value={visibleAnswer(question, answers[notification.id]?.[question.id])}
                      on:input={(event) => setAnswer(notification.id, question.id, event.currentTarget.value)}
                    />
                  {/if}
                </fieldset>
              {/each}
              <button
                class="primary full"
                type="button"
                disabled={submitting === notification.id}
                on:click={() => performAction(notification, "answer")}
              >{submitting === notification.id ? "Sending…" : "Send answer"}</button>
            </div>
          {:else}
            <div class="request-actions">
              {#if notification.kind === "exec"}
                <button class="primary" type="button" disabled={submitting === notification.id} on:click={() => performAction(notification, "accept")}>Run once</button>
              {:else if notification.kind === "network"}
                <button class="primary" type="button" disabled={submitting === notification.id} on:click={() => performAction(notification, "accept")}>Allow once</button>
              {:else if notification.kind === "patch"}
                <button class="primary" type="button" disabled={submitting === notification.id} on:click={() => performAction(notification, "accept")}>Apply once</button>
              {:else}
                <button class="primary" type="button" disabled={submitting === notification.id} on:click={() => performAction(notification, "accept")}>Allow once</button>
              {/if}
              <button class="danger" type="button" disabled={submitting === notification.id} on:click={() => performAction(notification, "decline")}>Deny</button>
            </div>
          {/if}
        </article>
      {/each}
    </section>
  {/if}

  {#if followUpOpen}
    <form class="follow-up" on:submit|preventDefault={submitFollowUp}>
      <label class="visually-hidden" for="follow-up-input">Reply to Codex</label>
      <div class="follow-up-row">
        <input id="follow-up-input" bind:value={followUp} placeholder="Message Codex" />
        <button
          class="follow-up-send"
          type="submit"
          disabled={!prepareFollowUp(followUp) || submitting === "follow-up"}
          aria-label="Send reply"
          title="Send"
        >{submitting === "follow-up" ? "…" : "↑"}</button>
        <button
          class="follow-up-close"
          type="button"
          aria-label="Close steering message"
          title="Close"
          on:click={() => (followUpOpen = false)}
        ><X size={15} strokeWidth={1.8} aria-hidden="true" /></button>
      </div>
    </form>
  {/if}

  {#if settingsOpen}
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
      cdpTransport={control.transport || "ipcOnly"}
      {cdpPortMode}
      {cdpCustomPort}
      onClose={closeSettings}
      onCompleteOnboarding={completeOnboarding}
      onSelectPet={selectPet}
      onRefreshPets={refreshPets}
      onBeginPetImport={beginPetImport}
      onCancelPetImport={cancelPetImport}
      onInstallPetImport={installPetImport}
      onClearActionError={clearActionError}
      onOpenPetsFolder={openPetsFolder}
      onRemoveSelectedPet={removeSelectedPet}
      onResetWindowPlacement={resetWindowPlacement}
      onCdpPortModeChange={setCdpPortMode}
      onCdpCustomPortChange={setCdpCustomPort}
      onLaunchCdpBridge={launchCdpBridge}
      onRestartCodexWithBridge={restartCodexWithBridge}
      onConnectExistingCdp={connectExistingCdp}
      onRetryCdpVerification={retryCdpVerification}
    />
  {/if}

  <div class="pet-controls">
    {#if followUpVisible}
      <button
        class="control-orb reply-orb"
        type="button"
        disabled={submitting === "follow-up"}
        aria-label={control.canReply
          ? "Steer current Codex task"
          : control.canStartFollowUp
          ? "Continue ready Codex task"
            : control.showWorkingFollowUp
              ? "Steer current Codex task while its owner reconnects"
              : "Continue ready Codex task while its owner reconnects"}
        title={control.canReply
          ? "Steer current task"
          : control.canStartFollowUp
            ? "Start next turn"
            : control.showWorkingFollowUp
              ? "Working task; waiting for Codex owner"
              : "Ready task; waiting for Codex owner"}
        on:click={openFollowUp}
      >↗</button>
    {/if}

    <button
      class="control-orb status-orb"
      type="button"
      data-state={displayState}
      aria-label={`${controlsOpen ? "Close" : "Open"} pet controls: ${labelForState(displayState)}`}
      aria-expanded={controlsOpen}
      aria-controls="pet-control-menu"
      title={labelForState(displayState)}
      on:click={toggleControls}
    ><span class="pulse" data-state={displayState}></span></button>
  </div>

  {#if controlsOpen}
    <section id="pet-control-menu" class="pet-menu" aria-label="Pet controls" aria-live="polite">
      <span class="menu-status" data-state={pets.length ? displayState : "disconnected"}>{pets.length ? labelForState(displayState) : "No pets found"}</span>
      {#if control.canStop}
        <button class="menu-action stop" type="button" disabled={submitting === "stop"} on:click={stopTask} aria-label="Stop task" title="Stop task"><Square size={12} strokeWidth={2} aria-hidden="true" /></button>
      {/if}
      <button class="menu-action" type="button" on:click={openInlineSettings} aria-label="Open pet settings" title="Settings"><Settings2 size={15} strokeWidth={1.8} aria-hidden="true" /></button>
    </section>
  {/if}

  {#if actionError && !settingsOpen}
    <div class="error action-error" class:with-follow-up={followUpOpen} role="alert" aria-atomic="true">
      <span>{actionError}</span>
      <button class="error-close" type="button" aria-label="Dismiss error" title="Dismiss" on:click={clearActionError}>×</button>
    </div>
  {/if}

  <button
    class="resize-grip"
    type="button"
    aria-label="Resize pet window"
    title="Drag to resize"
    on:pointerdown|stopPropagation|preventDefault={startWindowResize}
  ><MoveDiagonal2 class="resize-symbol" size={20} strokeWidth={2.2} aria-hidden="true" /></button>
</main>
