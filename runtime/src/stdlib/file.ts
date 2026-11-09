// The Node.js file system behind `File.node`. Failures of the file system
// are typed errors; an opened file is released by the owner of its scope.
import type { FileHandle } from "node:fs/promises";

import { Effect, type Scope } from "effect";

import type { Tagged } from "../data.ts";
import { nodeFsPromises as fs } from "../host.ts";
import { acquireRelease } from "../owner.ts";
import { tag } from "../tags.ts";

type Kind = "OpenError" | "ReadError" | "WriteError";

/** `File.OpenError`, `File.ReadError` and `File.WriteError`. */
export type FileError = Tagged<string, string>;

const failure = (kind: Kind) => {
  const _tag = tag(`File.${kind}.${kind}`);
  return (cause: unknown): FileError => ({ _tag, _0: cause instanceof Error ? cause.message : String(cause) });
};

export const fileOpen = (path: string): Effect.Effect<FileHandle, FileError, Scope.Scope> =>
  acquireRelease(Effect.tryPromise({ try: () => fs().open(path, "r"), catch: failure("OpenError") }), (handle) =>
    Effect.promise(() => handle.close()),
  );

export const fileReadHandle = (handle: FileHandle): Effect.Effect<string, FileError> =>
  Effect.tryPromise({ try: () => handle.readFile({ encoding: "utf8" }), catch: failure("ReadError") });

export const fileRead = (path: string): Effect.Effect<string, FileError> =>
  Effect.tryPromise({ try: () => fs().readFile(path, "utf8"), catch: failure("ReadError") });

export const fileWrite = (path: string, text: string): Effect.Effect<void, FileError> =>
  Effect.tryPromise({ try: () => fs().writeFile(path, text), catch: failure("WriteError") });
