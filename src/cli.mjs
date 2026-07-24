#!/usr/bin/env node
import { IpcObserver } from "./ipc-observer.mjs";
import { SessionTailer } from "./session-tailer.mjs";
import { AppLogTailer } from "./app-log-tailer.mjs";

const args = new Set(process.argv.slice(2));
const hasExclusiveSource = ["--ipc-only", "--sessions-only", "--logs-only"].some((arg) => args.has(arg));
const useIpc = !hasExclusiveSource || args.has("--ipc-only");
const useSessions = !hasExclusiveSource || args.has("--sessions-only");
const useLogs = !hasExclusiveSource || args.has("--logs-only");
const observers = [];

function output(event) {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

function attach(observer) {
  observer.on("status", output);
  observer.on("event", output);
  observer.on("observerError", (error) => output({
    source: observer.constructor.name,
    status: "observer-error",
    error: error.message,
    observedAt: new Date().toISOString(),
  }));
  observers.push(observer);
}

if (useIpc) attach(new IpcObserver());
if (useSessions) attach(new SessionTailer());
if (useLogs) attach(new AppLogTailer());

for (const observer of observers) {
  try {
    await observer.start();
  } catch (error) {
    output({
      source: observer.constructor.name,
      status: "unavailable",
      error: error.message,
      observedAt: new Date().toISOString(),
    });
  }
}

function shutdown() {
  for (const observer of observers) observer.stop();
  process.exit(0);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
