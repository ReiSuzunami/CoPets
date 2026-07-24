import assert from "node:assert/strict";
import test from "node:test";
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";

import {
  ONBOARDING_KEY,
  SELECTED_PET_KEY,
  WINDOW_STATE_KEY,
} from "../ui/lib/storage-keys.js";

const execFileAsync = promisify(execFile);
const readJson = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), "utf8"));
const packageConfig = await readJson("../package.json");
const packageLock = await readJson("../package-lock.json");
const tauriConfig = await readJson("../src-tauri/tauri.conf.json");
const signingHelper = new URL("../scripts/signing_identity.sh", import.meta.url).pathname;

test("desktop and package identity use the CoPets product name", () => {
  assert.equal(packageConfig.name, "copets");
  assert.equal(packageLock.name, "copets");
  assert.equal(packageLock.packages[""].name, "copets");
  assert.equal(tauriConfig.productName, "CoPets");
  assert.equal(tauriConfig.app.windows.find(({ label }) => label === "pet")?.title, "CoPets");
});

test("local signing identity precedence remains caller-visible", async () => {
  const resolveIdentity = async (env) => {
    const { stdout } = await execFileAsync("bash", ["-c", 'source "$1"; copets_signing_identity', "bash", signingHelper], {
      env: { PATH: process.env.PATH, ...env },
    });
    return stdout.trim();
  };
  assert.equal(await resolveIdentity({}), "CoPets Local Signing");
  assert.equal(await resolveIdentity({
    COPETS_SIGNING_IDENTITY: "CoPets Identity",
  }), "CoPets Identity");
});

test("bundle and persisted namespaces use the CoPets product identity", () => {
  assert.equal(tauriConfig.identifier, "dev.copets.sidecar");
  assert.equal(SELECTED_PET_KEY, "copets.selected-pet.v1");
  assert.equal(ONBOARDING_KEY, "copets.onboarding-complete.v1");
  assert.equal(WINDOW_STATE_KEY, "copets.window-state.v1");
});
