import { EventEmitter } from "node:events";
import { watch } from "node:fs";
import { readdir } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { createAppendCursor, readAppendedLines } from "./append-follower.mjs";
import { inspectOwnedRegularPath } from "./local-trust.mjs";
import { hashId } from "./privacy.mjs";

const DEFAULT_ACTIVE_WINDOW_MS = 30 * 60 * 1000;
const DEFAULT_DISCOVERY_INTERVAL_MS = 5_000;
const DEFAULT_POLL_INTERVAL_MS = 500;

const EVENT_SIGNALS = new Set([
  "agent_reasoning",
  "entered_review_mode",
  "error",
  "exited_review_mode",
  "guardian_assessment",
  "task_complete",
  "task_started",
  "turn_aborted",
]);
const RESPONSE_SIGNALS = new Set([
  "computer_tool_call",
  "custom_tool_call",
  "custom_tool_call_output",
  "function_call",
  "function_call_output",
  "message",
  "reasoning",
  "web_search_call",
]);

function boundedSignal(value, allowed, fallback) {
  return allowed.has(value) ? value : fallback;
}

export function normalizeSessionRecord(record, threadIdHash) {
  const base = {
    source: "codex-session-jsonl",
    diagnosticOnly: true,
    threadIdHash,
    observedAt: new Date().toISOString(),
  };

  if (record?.type === "event_msg") {
    const payload = record.payload || {};
    return {
      ...base,
      recordKind: "event-message",
      signal: boundedSignal(payload.type, EVENT_SIGNALS, "unknown-event"),
      success: typeof payload.success === "boolean" ? payload.success : null,
      turnIdHash: hashId(payload.turn_id),
      callIdHash: hashId(payload.call_id),
    };
  }

  if (record?.type === "response_item") {
    const payload = record.payload || {};
    return {
      ...base,
      recordKind: "response-item",
      signal: boundedSignal(payload.type, RESPONSE_SIGNALS, "unknown-item"),
      operationHash: hashId(payload.name),
      turnIdHash: hashId(payload.turn_id),
      callIdHash: hashId(payload.call_id),
    };
  }

  return null;
}

export class SessionTailer extends EventEmitter {
  #activeWindowMs;
  #files = new Map();
  #lastDiscoveryAt = 0;
  #pollIntervalMs;
  #polling = false;
  #root;
  #timer = null;
  #watcher = null;

  constructor({ root, activeWindowMs = DEFAULT_ACTIVE_WINDOW_MS, pollIntervalMs = DEFAULT_POLL_INTERVAL_MS } = {}) {
    super();
    const codexHome = process.env.CODEX_HOME || path.join(os.homedir(), ".codex");
    this.#root = root || path.join(codexHome, "sessions");
    this.#activeWindowMs = activeWindowMs;
    this.#pollIntervalMs = pollIntervalMs;
  }

  async start() {
    await this.#discoverActiveFiles();
    this.#watcher = watch(this.#root, { recursive: true }, (_event, filename) => {
      if (!filename?.endsWith(".jsonl")) return;
      void this.#tail(path.join(this.#root, filename));
    });
    // FSEvents can coalesce or omit nested-directory wakeups. Size polling is
    // deliberately cheap and keeps attachment latency bounded.
    this.#timer = setInterval(() => void this.#poll(), this.#pollIntervalMs);
    this.emit("status", {
      source: "codex-session-jsonl",
      status: "watching",
      activeFiles: this.#files.size,
      observedAt: new Date().toISOString(),
    });
  }

  stop() {
    this.#watcher?.close();
    this.#watcher = null;
    if (this.#timer) clearInterval(this.#timer);
    this.#timer = null;
  }

  async #discoverActiveFiles() {
    this.#lastDiscoveryAt = Date.now();
    const names = await readdir(this.#root, { recursive: true });
    const cutoff = Date.now() - this.#activeWindowMs;
    await Promise.all(names.filter((name) => name.endsWith(".jsonl")).map(async (name) => {
      const file = path.join(this.#root, name);
      try {
        const info = await inspectOwnedRegularPath(file);
        if (info.mtimeMs >= cutoff && !this.#files.has(file)) {
          this.#files.set(file, createAppendCursor(info));
        }
      } catch {
        // Files can rotate between discovery and stat.
      }
    }));
  }

  async #poll() {
    if (this.#polling) return;
    this.#polling = true;
    try {
      if (Date.now() - this.#lastDiscoveryAt >= DEFAULT_DISCOVERY_INTERVAL_MS) {
        await this.#discoverActiveFiles();
      }
      await Promise.all([...this.#files.keys()].map((file) => this.#tail(file)));
    } finally {
      this.#polling = false;
    }
  }

  async #tail(file) {
    try {
      let cursor = this.#files.get(file);
      if (!cursor) {
        cursor = createAppendCursor();
        this.#files.set(file, cursor);
      }
      const lines = await readAppendedLines(file, cursor);
      const threadIdHash = hashId(path.basename(file).replace(/^.*-([0-9a-f-]{36})\.jsonl$/, "$1"));

      for (const line of lines) {
        if (!line) continue;
        try {
          const event = normalizeSessionRecord(JSON.parse(line), threadIdHash);
          if (event) this.emit("event", event);
        } catch (error) {
          this.emit("observerError", new Error("failed to parse appended JSONL record"));
        }
      }
    } catch (error) {
      if (error?.code !== "ENOENT") this.emit("observerError", error);
    }
  }
}
