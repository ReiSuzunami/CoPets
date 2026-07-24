#!/usr/bin/env node

import { access, readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const docsRoot = path.join(root, "docs");
const requiredMetadata = ["Status", "Owns", "Update when", "Last verified"];
const errors = [];

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(absolute));
    if (entry.isFile() && entry.name.endsWith(".md")) files.push(absolute);
  }
  return files;
}

function displayPath(file) {
  return path.relative(root, file) || ".";
}

function markdownTargets(source) {
  const targets = [];
  const pattern = /!?\[[^\]]*\]\(([^)]+)\)/g;
  for (const match of source.matchAll(pattern)) {
    let target = match[1].trim();
    if (target.startsWith("<") && target.endsWith(">")) {
      target = target.slice(1, -1);
    } else {
      target = target.split(/\s+["']/u, 1)[0];
    }
    targets.push(target);
  }
  return targets;
}

function isExternal(target) {
  return /^(?:[a-z][a-z0-9+.-]*:|#)/iu.test(target);
}

async function checkLocalLinks(file, source) {
  for (const target of markdownTargets(source)) {
    if (!target || isExternal(target)) continue;
    const pathname = target.split("#", 1)[0];
    if (!pathname) continue;
    let decoded;
    try {
      decoded = decodeURIComponent(pathname);
    } catch {
      errors.push(`${displayPath(file)}: invalid URL encoding in ${target}`);
      continue;
    }
    const resolved = path.resolve(path.dirname(file), decoded);
    try {
      await access(resolved);
    } catch {
      errors.push(`${displayPath(file)}: missing local link ${target}`);
    }
  }
}

function checkMetadata(file, source) {
  const header = source.split("\n").slice(0, 12).join("\n");
  for (const field of requiredMetadata) {
    if (!new RegExp(`^> ${field}:\\s*\\S`, "mu").test(header)) {
      errors.push(`${displayPath(file)}: missing metadata field '${field}'`);
    }
  }
}

const docs = (await walk(docsRoot)).sort();
const topLevel = ["README.md", "AGENTS.md", "CLAUDE.md", "CONTRIBUTING.md", "CHANGELOG.md"].map((name) => path.join(root, name));

for (const file of [...topLevel, ...docs]) {
  const source = await readFile(file, "utf8");
  await checkLocalLinks(file, source);
  if (file.startsWith(`${docsRoot}${path.sep}`)) checkMetadata(file, source);
}

const indexPath = path.join(docsRoot, "README.md");
const indexSource = await readFile(indexPath, "utf8");
const indexed = new Set(
  markdownTargets(indexSource)
    .filter((target) => target && !isExternal(target))
    .map((target) => path.resolve(docsRoot, decodeURIComponent(target.split("#", 1)[0])))
);

for (const file of docs) {
  if (file !== indexPath && !indexed.has(file)) {
    errors.push(`${displayPath(file)}: not linked from docs/README.md`);
  }
}

if (errors.length > 0) {
  console.error(`Documentation check failed (${errors.length}):`);
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log(`Documentation check passed (${docs.length} docs files).`);
