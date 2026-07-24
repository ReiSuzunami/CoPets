import { EventEmitter } from "node:events";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { randomUUID } from "node:crypto";

import { inspectOwnedSocketPath } from "./local-trust.mjs";
import { hashId, sanitizeIpcParams, topLevelKeys } from "./privacy.mjs";

const DEFAULT_MAX_FRAME_BYTES = 16 * 1024 * 1024;

export function encodeFrame(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const frame = Buffer.allocUnsafe(body.length + 4);
  frame.writeUInt32LE(body.length, 0);
  body.copy(frame, 4);
  return frame;
}

export class FrameDecoder {
  #buffer = Buffer.alloc(0);
  #maxFrameBytes;

  constructor(maxFrameBytes = DEFAULT_MAX_FRAME_BYTES) {
    this.#maxFrameBytes = maxFrameBytes;
  }

  push(chunk) {
    this.#buffer = this.#buffer.length === 0 ? chunk : Buffer.concat([this.#buffer, chunk]);
    const messages = [];

    while (this.#buffer.length >= 4) {
      const length = this.#buffer.readUInt32LE(0);
      if (length === 0 || length > this.#maxFrameBytes) {
        throw new Error(`invalid IPC frame length: ${length}`);
      }
      if (this.#buffer.length < length + 4) break;

      const body = this.#buffer.subarray(4, length + 4).toString("utf8");
      this.#buffer = this.#buffer.subarray(length + 4);
      messages.push(JSON.parse(body));
    }

    return messages;
  }
}

function defaultSocketPath() {
  const codexHome = process.env.CODEX_HOME || path.join(os.homedir(), ".codex");
  return path.join(codexHome, "ipc", "ipc.sock");
}

export function sanitizeIpcBroadcast(message) {
  const signal = typeof message?.method === "string"
    && /^[A-Za-z][A-Za-z0-9._:-]{0,95}$/.test(message.method)
    ? message.method
    : "unknown";
  const version = Number.isSafeInteger(message?.version)
    && message.version >= 0
    && message.version <= 65_535
    ? message.version
    : null;
  return {
    source: "codex-app-ipc",
    diagnosticOnly: true,
    signal,
    version,
    sourceClientHash: hashId(message?.sourceClientId),
    paramKeys: topLevelKeys(message?.params),
    fields: sanitizeIpcParams(signal, message?.params),
    observedAt: new Date().toISOString(),
  };
}

export class IpcObserver extends EventEmitter {
  #clientId = "initializing-client";
  #decoder = new FrameDecoder();
  #socket = null;
  #socketPath;

  constructor({ socketPath = defaultSocketPath() } = {}) {
    super();
    this.#socketPath = socketPath;
  }

  async start() {
    await inspectOwnedSocketPath(this.#socketPath);

    await new Promise((resolve, reject) => {
      const socket = net.createConnection(this.#socketPath);
      this.#socket = socket;
      socket.once("connect", resolve);
      socket.once("error", reject);
      socket.on("data", (chunk) => this.#onData(chunk));
      socket.on("close", () => this.emit("status", { source: "codex-app-ipc", status: "disconnected", observedAt: new Date().toISOString() }));
      socket.on("error", (error) => this.emit("observerError", error));
    });

    this.#send({
      type: "request",
      requestId: randomUUID(),
      sourceClientId: this.#clientId,
      version: 0,
      method: "initialize",
      params: { clientType: "codex-pet-readonly-observer" },
    });
  }

  stop() {
    this.#socket?.destroy();
    this.#socket = null;
  }

  #send(message) {
    if (!this.#socket?.writable) return;
    this.#socket.write(encodeFrame(message));
  }

  #onData(chunk) {
    let messages;
    try {
      messages = this.#decoder.push(chunk);
    } catch (error) {
      this.emit("observerError", error);
      this.stop();
      return;
    }

    for (const message of messages) this.#onMessage(message);
  }

  #onMessage(message) {
    if (message.type === "response" && message.method === "initialize" && message.resultType === "success") {
      this.#clientId = message.result.clientId;
      this.emit("status", {
        source: "codex-app-ipc",
        status: "attached",
        clientIdHash: hashId(this.#clientId),
        observedAt: new Date().toISOString(),
      });
      return;
    }

    if (message.type === "broadcast") {
      this.emit("event", sanitizeIpcBroadcast(message));
      return;
    }

    // A passive observer never accepts follower/control requests.
    if (message.type === "client-discovery-request") {
      this.#send({
        type: "client-discovery-response",
        requestId: message.requestId,
        response: { canHandle: false },
      });
      return;
    }

    if (message.type === "request") {
      this.#send({
        type: "response",
        requestId: message.requestId,
        resultType: "error",
        error: "readonly-observer",
      });
    }
  }
}
