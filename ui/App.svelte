<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import PetWindow from "./PetWindow.svelte";
  import SettingsWindow from "./SettingsWindow.svelte";

  const isSettingsWindow = getCurrentWindow().label === "settings";
</script>

{#if isSettingsWindow}
  <SettingsWindow />
{:else}
  <PetWindow />
{/if}
