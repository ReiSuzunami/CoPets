import { execFile } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import {
  inspectOwnedRegularPath,
  validateProtectedParentChain,
} from "./local-trust.mjs";
import { hashId } from "./privacy.mjs";

const execFileAsync = promisify(execFile);

function defaultStateDbPath() {
  const codexHome = process.env.CODEX_HOME || path.join(os.homedir(), ".codex");
  return path.join(codexHome, "state_5.sqlite");
}

export async function loadKnownThreadHashes({
  dbPath = defaultStateDbPath(),
  sqlite = "/usr/bin/sqlite3",
  run = execFileAsync,
  inspect = inspectOwnedRegularPath,
  inspectParents = validateProtectedParentChain,
} = {}) {
  await inspectParents(dbPath);
  await inspect(dbPath);
  const { stdout } = await run(sqlite, [
    "-readonly",
    dbPath,
    "SELECT id FROM threads WHERE id IS NOT NULL;",
  ], { maxBuffer: 16 * 1024 * 1024 });

  return new Set(stdout.split("\n").filter(Boolean).map(hashId));
}
