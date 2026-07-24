import { EventEmitter } from "node:events";
import { watch } from "node:fs";
import { readdir } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { createAppendCursor, readAppendedLines } from "./append-follower.mjs";
import { inspectOwnedRegularPath } from "./local-trust.mjs";
import { hashId } from "./privacy.mjs";
import { loadKnownThreadHashes } from "./thread-index.mjs";

const ACTIVE_WINDOW_MS = 30 * 60 * 1000;
const DISCOVERY_INTERVAL_MS = 5_000;
const POLL_INTERVAL_MS = 500;
const RECONCILE_BYTES = 2 * 1024 * 1024;
const THREAD_INDEX_INTERVAL_MS = 30_000;
const MAX_RECONCILED_FACTS = 32;
const WINDOW_APPEARANCES = new Set(["dark", "light"]);

function parseBoolean(value) {
  if (value === "true") return true;
  if (value === "false") return false;
  return null;
}

function parseBoundedInteger(value) {
  if (!/^\d{1,10}$/.test(value || "")) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

export function normalizeAppLogLine(line) {
  if (!line.includes("thread_stream_view_activity_changed")) return null;

  const fields = {};
  const pattern = /\b(active|conversationId|rendererWindowId|rendererWebContentsId|rendererWindowAppearance|rendererWindowFocused|rendererWindowVisible|resumeState|streamRole)=([^\s]+)/g;
  for (const match of line.matchAll(pattern)) fields[match[1]] = match[2];

  const active = parseBoolean(fields.active);
  if (active === null || !fields.conversationId) return null;

  const eventAt = line.match(/^(\d{4}-\d{2}-\d{2}T[^\s]+)/)?.[1] || null;
  return {
    source: "codex-app-log",
    diagnosticOnly: true,
    signal: "thread-view-activity",
    active,
    conversationIdHash: hashId(fields.conversationId),
    windowId: parseBoundedInteger(fields.rendererWindowId),
    webContentsId: parseBoundedInteger(fields.rendererWebContentsId),
    windowAppearance: WINDOW_APPEARANCES.has(fields.rendererWindowAppearance)
      ? fields.rendererWindowAppearance
      : null,
    windowFocused: parseBoolean(fields.rendererWindowFocused),
    windowVisible: parseBoolean(fields.rendererWindowVisible),
    eventAt,
    observedAt: new Date().toISOString(),
  };
}

export function decorateKnownAppActivity(event, file, knownThreadHashes, threadIndexReady) {
  if (!event || !threadIndexReady || !knownThreadHashes.has(event.conversationIdHash)) return null;
  const name = path.basename(file);
  const instance = name.match(/^(codex-desktop-[0-9a-f-]{36}-\d+)/)?.[1] || name;
  return { ...event, appInstanceHash: hashId(instance) };
}

export function latestActivityFacts(events, limit = MAX_RECONCILED_FACTS) {
  const latestByView = new Map();
  for (const event of events) {
    const key = [event.appInstanceHash, event.windowId, event.webContentsId, event.conversationIdHash].join(":");
    latestByView.set(key, event);
  }
  return [...latestByView.values()]
    .sort((left, right) => String(left.eventAt).localeCompare(String(right.eventAt)))
    .slice(-limit);
}

export class AppLogTailer extends EventEmitter {
  #files = new Map();
  #lastDiscoveryAt = 0;
  #lastThreadIndexAt = 0;
  #knownThreadHashes = new Set();
  #threadIndexReady = false;
  #threadIndexLoader;
  #polling = false;
  #root;
  #timer = null;
  #watcher = null;

  constructor({ root, threadIndexLoader = loadKnownThreadHashes } = {}) {
    super();
    this.#root = root || path.join(os.homedir(), "Library", "Logs", "com.openai.codex");
    this.#threadIndexLoader = threadIndexLoader;
  }

  async start() {
    await this.#refreshThreadIndex();
    await this.#discoverActiveFiles();
    await this.#emitRecentActivityFacts();
    this.#watcher = watch(this.#root, { recursive: true }, (_event, filename) => {
      if (!filename?.endsWith(".log")) return;
      void this.#tail(path.join(this.#root, filename));
    });
    this.#timer = setInterval(() => void this.#poll(), POLL_INTERVAL_MS);
    this.emit("status", {
      source: "codex-app-log",
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
    const cutoff = Date.now() - ACTIVE_WINDOW_MS;
    await Promise.all(names.filter((name) => name.endsWith(".log")).map(async (name) => {
      const file = path.join(this.#root, name);
      try {
        const info = await inspectOwnedRegularPath(file);
        if (info.mtimeMs >= cutoff && this.#belongsToLiveApp(file) && !this.#files.has(file)) {
          this.#files.set(file, createAppendCursor(info));
        }
      } catch {
        // Log rotation can race discovery.
      }
    }));
  }

  async #emitRecentActivityFacts() {
    const events = [];
    await Promise.all([...this.#files.keys()].map(async (file) => {
      try {
        const info = await inspectOwnedRegularPath(file);
        if (info.size === 0) return;
        const start = Math.max(0, info.size - RECONCILE_BYTES);
        const snapshotCursor = createAppendCursor();
        snapshotCursor.offset = start;
        const lines = await readAppendedLines(file, snapshotCursor);
        if (start > 0) lines.shift();
        for (const line of lines) {
          const event = this.#decorateEvent(normalizeAppLogLine(line), file);
          if (event) events.push(event);
        }
      } catch {
        // Snapshot reconciliation is best effort.
      }
    }));
    for (const event of latestActivityFacts(events)) this.emit("event", { ...event, snapshot: true });
  }

  async #poll() {
    if (this.#polling) return;
    this.#polling = true;
    try {
      if (Date.now() - this.#lastDiscoveryAt >= DISCOVERY_INTERVAL_MS) await this.#discoverActiveFiles();
      if (Date.now() - this.#lastThreadIndexAt >= THREAD_INDEX_INTERVAL_MS) await this.#refreshThreadIndex();
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
      for (const line of lines) {
        const event = this.#decorateEvent(normalizeAppLogLine(line), file);
        if (event) this.emit("event", event);
      }
    } catch (error) {
      if (error?.code !== "ENOENT") this.emit("observerError", error);
    }
  }

  async #refreshThreadIndex() {
    this.#lastThreadIndexAt = Date.now();
    try {
      this.#knownThreadHashes = await this.#threadIndexLoader();
      this.#threadIndexReady = true;
    } catch (error) {
      this.#threadIndexReady = false;
      this.emit("observerError", new Error("thread index unavailable"));
    }
  }

  #decorateEvent(event, file) {
    return decorateKnownAppActivity(
      event,
      file,
      this.#knownThreadHashes,
      this.#threadIndexReady,
    );
  }

  #belongsToLiveApp(file) {
    const pid = Number(path.basename(file).match(/-(\d+)-t\d+-i\d+-/)?.[1]);
    if (!pid) return true;
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  }
}
