#!/usr/bin/env node

import { execFile } from "node:child_process";
import { lstat, readFile, readlink } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const forbiddenSegments = new Set([".impeccable", "__pycache__", "artifacts", "design", "tmp"]);
const forbiddenSuffixes = [".pyc", ".pyo"];
const sensitivePatterns = [
  { label: "personal macOS home path", pattern: /\/Users\/[^/\s]+(?:\/|$)/u },
  { label: "personal Unix home path", pattern: /\/home\/[^/\s]+(?:\/|$)/u },
  { label: "private key", pattern: /-----BEGIN (?:EC |OPENSSH |RSA )?PRIVATE KEY-----/u },
  { label: "AWS access key", pattern: /\bAKIA[0-9A-Z]{16}\b/u },
  { label: "GitHub token", pattern: /\bgh[pousr]_[A-Za-z0-9]{20,}\b/u },
  { label: "OpenAI-style secret", pattern: /\bsk-[A-Za-z0-9_-]{20,}\b/u },
];

export function auditPath(relative) {
  const normalized = relative.split(path.sep).join("/");
  const segments = normalized.split("/");
  const errors = [];
  if (segments.some((segment) => forbiddenSegments.has(segment))) {
    errors.push(`${normalized}: local/generated path is not public source`);
  }
  if (forbiddenSuffixes.some((suffix) => normalized.endsWith(suffix))) {
    errors.push(`${normalized}: compiled cache is not public source`);
  }
  return errors;
}

export function auditText(relative, source) {
  const errors = [];
  for (const { label, pattern } of sensitivePatterns) {
    if (pattern.test(source)) errors.push(`${relative}: contains ${label}`);
  }
  return errors;
}

async function candidatePaths() {
  const { stdout } = await execFileAsync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    { cwd: root, encoding: "buffer", maxBuffer: 32 * 1024 * 1024 },
  );
  return stdout.toString("utf8").split("\0").filter(Boolean).sort();
}

async function auditFile(relative) {
  const errors = auditPath(relative);
  const absolute = path.join(root, relative);
  let info;
  try {
    info = await lstat(absolute);
  } catch {
    return [...errors, `${relative}: candidate path cannot be read`];
  }

  if (info.isSymbolicLink()) {
    const target = await readlink(absolute);
    const resolved = path.resolve(path.dirname(absolute), target);
    if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
      errors.push(`${relative}: symlink escapes repository`);
    }
    return errors;
  }
  if (!info.isFile() || info.size > 2 * 1024 * 1024) return errors;

  const source = await readFile(absolute);
  if (source.includes(0)) return errors;
  return [...errors, ...auditText(relative, source.toString("utf8"))];
}

export async function auditPublicTree() {
  const errors = [];
  for (const relative of await candidatePaths()) {
    errors.push(...await auditFile(relative));
  }
  return errors;
}

async function main() {
  const errors = await auditPublicTree();
  if (errors.length > 0) {
    console.error(`Public tree audit failed (${errors.length}):`);
    for (const error of errors) console.error(`- ${error}`);
    process.exitCode = 1;
    return;
  }
  console.log("Public tree audit passed.");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
