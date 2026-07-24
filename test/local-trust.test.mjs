import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, symlink } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  assertOwnedRegularInfo,
  assertOwnedSocketInfo,
  validateProtectedParentChain,
} from "../src/local-trust.mjs";

test("local trust accepts only expected-owner regular files", () => {
  const regular = { isFile: () => true, uid: 501 };
  assert.doesNotThrow(() => assertOwnedRegularInfo(regular, 501));
  assert.throws(() => assertOwnedRegularInfo(regular, 502), /another user/);
  assert.throws(
    () => assertOwnedRegularInfo({ isFile: () => false, uid: 501 }, 501),
    /regular file/,
  );
});

test("local trust accepts only expected-owner Unix sockets", () => {
  const socket = { isSocket: () => true, uid: 501 };
  assert.doesNotThrow(() => assertOwnedSocketInfo(socket, 501));
  assert.throws(() => assertOwnedSocketInfo(socket, 502), /another user/);
  assert.throws(
    () => assertOwnedSocketInfo({ isSocket: () => false, uid: 501 }, 501),
    /Unix socket/,
  );
});

test("local trust rejects writable ancestors for path-based consumers", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "copets-trust-"));
  const writable = path.join(root, "writable");
  await mkdir(writable, { mode: 0o700 });
  await chmod(writable, 0o777);

  await assert.rejects(
    validateProtectedParentChain(path.join(writable, "state.sqlite")),
    /group- or world-writable/,
  );
});

test("local trust rejects a user-owned parent symlink", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "copets-trust-"));
  const target = path.join(root, "target");
  const link = path.join(root, "link");
  await mkdir(target);
  await symlink(target, link);

  await assert.rejects(
    validateProtectedParentChain(path.join(link, "state.sqlite")),
    /unsafe parent path/,
  );
});
