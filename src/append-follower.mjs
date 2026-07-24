import { StringDecoder } from "node:string_decoder";

import { openOwnedRegularFile } from "./local-trust.mjs";

function resetCursor(cursor) {
  cursor.offset = 0;
  cursor.carry = "";
  cursor.decoder = new StringDecoder("utf8");
}

function updateIdentity(cursor, info) {
  cursor.dev = info.dev;
  cursor.ino = info.ino;
  cursor.mtimeMs = info.mtimeMs;
}

async function readRange(handle, start, end, decoder) {
  const chunks = [];
  await new Promise((resolve, reject) => {
    const stream = handle.createReadStream({ start, end, autoClose: false });
    stream.on("data", (chunk) => chunks.push(decoder.write(chunk)));
    stream.on("end", resolve);
    stream.on("error", reject);
  });
  return chunks.join("");
}

export function createAppendCursor(info = null) {
  return {
    offset: info?.size || 0,
    carry: "",
    decoder: new StringDecoder("utf8"),
    dev: info?.dev ?? null,
    ino: info?.ino ?? null,
    mtimeMs: info?.mtimeMs ?? null,
    reading: false,
  };
}

export async function readAppendedLines(file, cursor) {
  if (cursor.reading) return [];
  cursor.reading = true;
  let handle;
  try {
    const opened = await openOwnedRegularFile(file);
    handle = opened.handle;
    const { info } = opened;
    const identityChanged = cursor.dev !== null
      && cursor.ino !== null
      && (cursor.dev !== info.dev || cursor.ino !== info.ino);
    const sameSizeRewrite = cursor.mtimeMs !== null
      && info.size === cursor.offset
      && info.mtimeMs !== cursor.mtimeMs;
    if (identityChanged || info.size < cursor.offset || sameSizeRewrite) resetCursor(cursor);

    if (info.size === cursor.offset) {
      updateIdentity(cursor, info);
      return [];
    }

    const text = cursor.carry
      + await readRange(handle, cursor.offset, info.size - 1, cursor.decoder);
    cursor.offset = info.size;
    updateIdentity(cursor, info);
    const lines = text.split("\n");
    cursor.carry = lines.pop() || "";
    return lines;
  } finally {
    await handle?.close();
    cursor.reading = false;
  }
}
