import { createHash } from "node:crypto";

const MAX_HASH_INPUT_LENGTH = 512;
const MAX_PARAM_KEYS = 64;
const SAFE_PARAM_KEY = /^[A-Za-z][A-Za-z0-9_-]{0,63}$/;
const UNSAFE_PARAM_KEYS = new Set(["__proto__", "constructor", "prototype"]);

const STREAM_STATES = new Set([
  "aborted",
  "complete",
  "completed",
  "error",
  "failed",
  "idle",
  "interrupted",
  "loading",
  "reviewing",
  "stopped",
  "streaming",
  "waiting",
  "working",
]);
const READ_STATES = new Set(["read", "unread"]);
const CLIENT_STATUSES = new Set([
  "active",
  "attached",
  "connected",
  "disconnected",
  "inactive",
  "offline",
  "online",
  "ready",
  "unavailable",
]);
const CHANGE_TYPES = new Set(["delta", "patch", "snapshot", "update"]);

const IPC_PARAM_SCHEMAS = new Map([
  ["thread-stream-state-changed", {
    ids: ["agentThreadId", "conversationId", "hostId", "turnId"],
    booleans: ["isStreaming"],
    integers: ["revision"],
    enums: { state: STREAM_STATES, streamState: STREAM_STATES },
    changeType: true,
  }],
  ["thread-stream-following-changed", {
    ids: ["conversationId", "hostId"],
    booleans: ["following"],
  }],
  ["thread-read-state-changed", {
    ids: ["conversationId"],
    booleans: ["isRead", "isUnread"],
    enums: { readState: READ_STATES },
  }],
  ["thread-archived", {
    ids: ["conversationId"],
    booleans: ["archived", "isArchived"],
  }],
  ["thread-unarchived", {
    ids: ["conversationId"],
    booleans: ["archived", "isArchived"],
  }],
  ["thread-queued-followups-changed", {
    ids: ["conversationId"],
    integers: ["revision"],
  }],
  ["client-status-changed", {
    ids: ["clientId"],
    booleans: ["isSelf"],
    enums: { status: CLIENT_STATUSES },
  }],
  ["ipc-connection-reset", {}],
]);

function isRecord(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

export function hashId(value) {
  if (
    typeof value !== "string"
    || value.length === 0
    || value.length > MAX_HASH_INPUT_LENGTH
  ) return null;
  return createHash("sha256").update(value).digest("hex").slice(0, 12);
}

export function sanitizeIpcParams(method, value) {
  if (!isRecord(value)) return {};
  const schema = IPC_PARAM_SCHEMAS.get(method);
  if (!schema) return {};
  const result = {};
  for (const key of schema.ids || []) {
    const hash = hashId(value[key]);
    if (hash) result[`${key}Hash`] = hash;
  }
  for (const key of schema.booleans || []) {
    if (typeof value[key] === "boolean") result[key] = value[key];
  }
  for (const key of schema.integers || []) {
    if (Number.isSafeInteger(value[key]) && value[key] >= 0) result[key] = value[key];
  }
  for (const [key, allowed] of Object.entries(schema.enums || {})) {
    if (allowed.has(value[key])) result[key] = value[key];
  }
  if (schema.changeType && isRecord(value.change) && CHANGE_TYPES.has(value.change.type)) {
    result.changeType = value.change.type;
  }
  return result;
}

export function topLevelKeys(value) {
  if (!isRecord(value)) return [];
  return Object.keys(value)
    .filter((key) => SAFE_PARAM_KEY.test(key) && !UNSAFE_PARAM_KEYS.has(key))
    .sort()
    .slice(0, MAX_PARAM_KEYS);
}
