<script>
  import Archive from "@lucide/svelte/icons/archive";
  import FolderInput from "@lucide/svelte/icons/folder-input";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import {
    bridgeNeedsVerificationRetry,
    bridgeSummaryLabel,
  } from "./lib/cdp-bridge-settings.js";

  export let connected = false;
  export let onboardingVisible = false;
  export let pets = [];
  export let catalogIssues = [];
  export let selectedPet = "";
  export let importPreview = null;
  export let managementNotice = "";
  export let actionError = "";
  export let submitting = "";
  export let cdpTransport = "ipcOnly";
  export let cdpPortMode = "automatic";
  export let cdpCustomPort = "";
  export let onClose;
  export let onCompleteOnboarding;
  export let onSelectPet;
  export let onRefreshPets;
  export let onBeginPetImport;
  export let onCancelPetImport;
  export let onInstallPetImport;
  export let onClearActionError;
  export let onOpenPetsFolder;
  export let onRemoveSelectedPet;
  export let onResetWindowPlacement;
  export let onCdpPortModeChange;
  export let onCdpCustomPortChange;
  export let onLaunchCdpBridge;
  export let onRestartCodexWithBridge;
  export let onConnectExistingCdp;
  export let onRetryCdpVerification;
</script>

<section id="pet-settings" class="settings-panel" aria-label="Pet settings">
  <header class="settings-header">
    <h2>Settings</h2>
    <button
      class="settings-close"
      type="button"
      aria-label="Close settings"
      title="Close"
      on:click={onClose}
    ><X size={15} strokeWidth={1.8} aria-hidden="true" /></button>
  </header>

  {#if onboardingVisible}
    <div class="settings-guide">
      <strong>Your pet follows the selected Codex task</strong>
      <span>Keep Codex App open; the pet reflects the foreground local task.</span>
      <button type="button" on:click={onCompleteOnboarding}>Got it</button>
    </div>
  {/if}

  {#if !connected}
    <p class="settings-connection-note" role="status">
      <strong>Codex is not connected.</strong>
      <span>Open a local task to activate controls.</span>
    </p>
  {/if}

  <details class="settings-row cdp-bridge">
    <summary class="cdp-bridge-summary">
      <span class="cdp-bridge-summary-copy">
        <span class="cdp-bridge-title">Experimental bridge</span>
        <span class="cdp-bridge-summary-status">{bridgeSummaryLabel(cdpTransport)}</span>
      </span>
      <span class="cdp-bridge-summary-action">
        {cdpTransport === "cdpReady" ? "Manage" : "Set up"}
        <span class="cdp-bridge-disclosure" aria-hidden="true">›</span>
      </span>
    </summary>
    <div class="cdp-bridge-details">
      <p class="cdp-bridge-copy">
        Launch, restart, or connect a local CDP Codex. This opens or uses a private debug port,
        not an official OpenAI interface.
      </p>
      {#if bridgeNeedsVerificationRetry(cdpTransport)}
        <p class="cdp-bridge-copy">
          CoPets re-checks only the same verified Codex process.
        </p>
        <button
          class="settings-action cdp-retry"
          type="button"
          disabled={Boolean(submitting)}
          on:click={onRetryCdpVerification}
        >{submitting === "cdp-verify" ? "Verifying…" : "Retry verification"}</button>
      {/if}
      <fieldset class="cdp-port-mode">
        <legend class="visually-hidden">Bridge port mode</legend>
        <label class:cdp-port-mode-active={cdpPortMode === "automatic"}>
          <input
            class="cdp-port-mode-input"
            type="radio"
            name="cdp-port-mode"
            checked={cdpPortMode === "automatic"}
            on:change={() => onCdpPortModeChange("automatic")}
          />
          <span>Automatic</span>
        </label>
        <label class:cdp-port-mode-active={cdpPortMode === "custom"}>
          <input
            class="cdp-port-mode-input"
            type="radio"
            name="cdp-port-mode"
            checked={cdpPortMode === "custom"}
            on:change={() => onCdpPortModeChange("custom")}
          />
          <span>Custom port</span>
        </label>
      </fieldset>
      {#if cdpPortMode === "custom"}
        <label class="cdp-custom-port" for="cdp-custom-port">
          <span>Loopback port</span>
          <input
            id="cdp-custom-port"
            type="number"
            inputmode="numeric"
            min="1024"
            max="65535"
            step="1"
            placeholder="1024–65535"
            value={cdpCustomPort}
            on:input={(event) => onCdpCustomPortChange(event.currentTarget.value)}
          />
        </label>
      {/if}
      <div class="cdp-bridge-actions">
        <button
          class="settings-action cdp-launch"
          type="button"
          disabled={Boolean(submitting)}
          on:click={onLaunchCdpBridge}
        >{submitting === "cdp-bridge" ? "Launching…" : "Launch Codex"}</button>
        <button
          class="settings-action cdp-connect"
          type="button"
          disabled={Boolean(submitting)}
          on:click={onConnectExistingCdp}
        >{submitting === "cdp-connect" ? "Connecting…" : "Connect existing"}</button>
      </div>
      {#if cdpTransport === "ipcOnly"}
        <button
          class="settings-action cdp-restart"
          type="button"
          disabled={Boolean(submitting)}
          on:click={onRestartCodexWithBridge}
        >{submitting === "cdp-restart" ? "Restarting…" : "Restart Codex with bridge"}</button>
      {/if}
      <span class="cdp-bridge-status" aria-live="polite">
        {cdpTransport === "cdpReady"
          ? "Bridge ready for this CoPets session."
          : cdpTransport === "cdpDegraded"
            ? "Bridge unavailable. Standard IPC controls remain active."
            : "Standard IPC remains default until you launch or connect."}
      </span>
    </div>
  </details>

  <div class="settings-row">
    <label for="pet-selection">Pet</label>
    <div class="settings-field">
      {#if pets.length}
        <select
          id="pet-selection"
          value={selectedPet}
          on:change={(event) => onSelectPet(event.currentTarget.value)}
        >
          {#each pets as pet}
            <option value={pet.id}>{pet.displayName}</option>
          {/each}
        </select>
      {:else}
        <span class="settings-empty">Import a Pet Creator folder, pet.json, or ZIP package</span>
      {/if}
      <button
        class="settings-icon"
        type="button"
        on:click={onRefreshPets}
        aria-label="Rescan Codex Pet Creator packages"
        title="Rescan pets"
      ><RefreshCw size={15} strokeWidth={1.8} aria-hidden="true" /></button>
    </div>
  </div>

  <div class="settings-row">
    <span class="settings-label">Import</span>
    <div class="settings-actions">
      <button
        class="settings-action"
        type="button"
        disabled={Boolean(submitting)}
        on:click={() => onBeginPetImport("folder")}
      ><FolderInput size={14} strokeWidth={1.7} aria-hidden="true" />Folder</button>
      <button
        class="settings-action"
        type="button"
        disabled={Boolean(submitting)}
        on:click={() => onBeginPetImport("file")}
      ><Archive size={14} strokeWidth={1.7} aria-hidden="true" />ZIP / pet.json</button>
    </div>
  </div>

  {#if importPreview}
    <div class="import-preview" aria-live="polite">
      <div>
        <strong>{importPreview.pet.displayName}</strong>
        <span>{importPreview.pet.id} · v{importPreview.pet.spriteVersionNumber}</span>
        <span>{importPreview.pet.atlasWidth}×{importPreview.pet.atlasHeight} · {importPreview.pet.renderScale}×</span>
      </div>
      {#if importPreview.targetExists}
        <span class="replace-note">An installed package with this ID will be replaced.</span>
      {/if}
      <div class="import-actions">
        <button type="button" disabled={Boolean(submitting)} on:click={onCancelPetImport}>Cancel</button>
        <button class="primary" type="button" disabled={Boolean(submitting)} on:click={onInstallPetImport}>
          {submitting === "pet-install" ? "Installing…" : importPreview.targetExists ? "Replace" : "Install"}
        </button>
      </div>
    </div>
  {/if}

  {#if catalogIssues.length}
    <div class="package-issues" aria-label="Invalid pet packages">
      <strong>Needs attention</strong>
      {#each catalogIssues as issue}
        <span><b>{issue.folderName}</b>: {issue.message}</span>
      {/each}
    </div>
  {/if}

  {#if managementNotice}
    <p class="settings-notice" role="status">{managementNotice}</p>
  {/if}

  {#if actionError}
    <div class="settings-error" role="alert" aria-atomic="true">
      <span>{actionError}</span>
      <button class="error-close" type="button" aria-label="Dismiss error" title="Dismiss" on:click={onClearActionError}>×</button>
    </div>
  {/if}

  <div class="settings-row settings-management">
    <span class="settings-label">Manage</span>
    <div class="settings-actions">
      <button class="settings-action" type="button" on:click={onOpenPetsFolder}>
        <FolderOpen size={14} strokeWidth={1.7} aria-hidden="true" />Show in Finder
      </button>
      <button
        class="settings-action settings-remove"
        type="button"
        disabled={!selectedPet || Boolean(submitting)}
        aria-label="Remove selected pet"
        on:click={onRemoveSelectedPet}
      ><Trash2 size={14} strokeWidth={1.7} aria-hidden="true" />Remove</button>
    </div>
  </div>

  <div class="settings-row">
    <span class="settings-label">Window</span>
    <button
      class="settings-reset"
      type="button"
      disabled={submitting === "window-reset"}
      on:click={onResetWindowPlacement}
    >
      <RotateCcw size={14} strokeWidth={1.8} aria-hidden="true" />
      <span>{submitting === "window-reset" ? "Resetting…" : "Reset size & position"}</span>
    </button>
  </div>
</section>
