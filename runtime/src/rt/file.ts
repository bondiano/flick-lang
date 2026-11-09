// Node file operations on the kernel. The owner of `fileOpen` closes the
// handle even when its body fails or is interrupted.
import type { FileHandle } from "node:fs/promises";

import { tag } from "../tags.ts";
import type { Run } from "./fiber.ts";
import { nodeFs } from "./host.ts";
import { addFinalizer, foreignAsync } from "./owner.ts";

type Kind = "OpenError" | "ReadError" | "WriteError";

const failure = (kind: Kind) => {
  const _tag = tag(`File.${kind}.${kind}`);
  return (cause: unknown): { readonly _tag: string; readonly _0: string } => ({
    _tag,
    _0: cause instanceof Error ? cause.message : String(cause),
  });
};

const isHandle = (value: unknown): value is FileHandle =>
  typeof value === "object" &&
  value !== null &&
  "readFile" in value &&
  typeof value.readFile === "function" &&
  "close" in value &&
  typeof value.close === "function";

export function* fileOpen(path: string): Run {
  const handle: unknown = yield foreignAsync(() => nodeFs().promises.open(path, "r"), failure("OpenError"));
  if (!isHandle(handle)) throw new TypeError("node:fs returned no file handle");
  yield addFinalizer(() => foreignAsync(() => handle.close()));
  return handle;
}

export const fileReadHandle = (handle: FileHandle) =>
  foreignAsync(() => handle.readFile({ encoding: "utf8" }), failure("ReadError"));

export const fileRead = (path: string) => foreignAsync(() => nodeFs().promises.readFile(path, "utf8"), failure("ReadError"));

export const fileWrite = (path: string, text: string) =>
  foreignAsync(() => nodeFs().promises.writeFile(path, text), failure("WriteError"));
