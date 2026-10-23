// The Node.js file system behind `File.node`. Failures of the file system
// are typed errors; an opened file is released by the owner of its scope.
import type { FileHandle } from "node:fs/promises";

import { Effect, type Scope } from "effect";

import type { Tagged } from "../data.ts";
import { nodeFsPromises as fs } from "../host.ts";
import { acquireRelease } from "../owner.ts";

type Kind = "OpenError" | "ReadError" | "WriteError";

/** `File.OpenError`, `File.ReadError` and `File.WriteError`. */
export type FileError<K extends Kind> = Tagged<`File.${K}.${K}`, string>;

const failure =
  <K extends Kind>(kind: K) =>
  (cause: unknown): FileError<K> => ({
    _tag: `File.${kind}.${kind}`,
    _0: cause instanceof Error ? cause.message : String(cause),
  });

export const fileOpen = (path: string): Effect.Effect<FileHandle, FileError<"OpenError">, Scope.Scope> =>
  acquireRelease(Effect.tryPromise({ try: () => fs().open(path, "r"), catch: failure("OpenError") }), (handle) =>
    Effect.promise(() => handle.close()),
  );

export const fileReadHandle = (handle: FileHandle): Effect.Effect<string, FileError<"ReadError">> =>
  Effect.tryPromise({ try: () => handle.readFile({ encoding: "utf8" }), catch: failure("ReadError") });

export const fileRead = (path: string): Effect.Effect<string, FileError<"ReadError">> =>
  Effect.tryPromise({ try: () => fs().readFile(path, "utf8"), catch: failure("ReadError") });

export const fileWrite = (path: string, text: string): Effect.Effect<void, FileError<"WriteError">> =>
  Effect.tryPromise({ try: () => fs().writeFile(path, text), catch: failure("WriteError") });
