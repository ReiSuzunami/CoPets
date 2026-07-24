import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import path from "node:path";

function effectiveUid() {
  return typeof process.getuid === "function" ? process.getuid() : null;
}

function requireOwner(info, expectedUid) {
  if (expectedUid !== null && info.uid !== expectedUid) {
    throw new Error("local Codex source is owned by another user");
  }
}

export function assertOwnedRegularInfo(info, expectedUid = effectiveUid()) {
  if (!info.isFile()) throw new Error("local Codex evidence path is not a regular file");
  requireOwner(info, expectedUid);
}

export function assertOwnedSocketInfo(info, expectedUid = effectiveUid()) {
  if (!info.isSocket()) throw new Error("local Codex IPC path is not a Unix socket");
  requireOwner(info, expectedUid);
}

async function validateDirectoryChain(start, { allowRootSymlink }) {
  const expectedUid = effectiveUid();
  let directory = start;
  while (true) {
    const info = await lstat(directory);
    const rootOwnedSymlink = allowRootSymlink && info.uid === 0 && info.isSymbolicLink();
    if (!info.isDirectory() && !rootOwnedSymlink) {
      throw new Error("local Codex source has an unsafe parent path");
    }
    if (expectedUid !== null && info.uid !== 0 && info.uid !== expectedUid) {
      throw new Error("local Codex source parent is owned by another user");
    }
    if (!rootOwnedSymlink && (info.mode & 0o022) !== 0) {
      throw new Error("local Codex source parent is group- or world-writable");
    }
    const parent = path.dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
}

export async function validateProtectedParentChain(file) {
  const parent = path.dirname(path.resolve(file));
  await validateDirectoryChain(parent, { allowRootSymlink: true });
  const resolvedParent = await realpath(parent);
  await validateDirectoryChain(resolvedParent, { allowRootSymlink: false });
}

export async function inspectOwnedRegularPath(file) {
  const info = await lstat(file);
  assertOwnedRegularInfo(info);
  return info;
}

export async function inspectOwnedSocketPath(file) {
  await validateProtectedParentChain(file);
  const info = await lstat(file);
  assertOwnedSocketInfo(info);
  return info;
}

export async function openOwnedRegularFile(file) {
  const noFollow = constants.O_NOFOLLOW || 0;
  const handle = await open(file, constants.O_RDONLY | noFollow);
  try {
    const info = await handle.stat();
    assertOwnedRegularInfo(info);
    return { handle, info };
  } catch (error) {
    await handle.close();
    throw error;
  }
}
