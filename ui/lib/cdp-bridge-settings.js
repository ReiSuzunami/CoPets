export const CDP_PORT_MODE_AUTOMATIC = "automatic";
export const CDP_PORT_MODE_CUSTOM = "custom";

export function normalizeCdpPortMode(value) {
  return value === CDP_PORT_MODE_CUSTOM ? CDP_PORT_MODE_CUSTOM : CDP_PORT_MODE_AUTOMATIC;
}

export function parseCustomCdpPort(value) {
  const text = String(value ?? "").trim();
  if (!/^\d+$/.test(text)) return null;
  const port = Number(text);
  return Number.isInteger(port) && port >= 1024 && port <= 65535 ? port : null;
}

export function bridgeStatusLabel(transport) {
  if (transport === "cdpReady") return "Bridge ready for this CoPets session.";
  if (transport === "cdpDegraded") return "Bridge unavailable. Standard IPC controls remain active.";
  return "Off. Standard IPC controls are active.";
}

export function bridgeNeedsVerificationRetry(transport) {
  return transport === "cdpDegraded";
}

export function bridgeSummaryLabel(transport) {
  if (transport === "cdpReady") return "Ready";
  if (transport === "cdpDegraded") return "Unavailable";
  return "Standard IPC";
}
