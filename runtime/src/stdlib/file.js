// The Node.js file system behind `File.node`. Failures of the file system
// are typed errors; an opened file is released by the owner of its scope.
import { open, readFile, writeFile } from "node:fs/promises";

import { Effect } from "effect";

import { acquireRelease } from "../owner.js";

const failure = (tag) => (cause) => ({ _tag: `File.${tag}.${tag}`, _0: cause?.message ?? String(cause) });

export const fileOpen = (path) =>
  acquireRelease(
    Effect.tryPromise({ try: () => open(path, "r"), catch: failure("OpenError") }),
    (handle) => Effect.promise(() => handle.close()),
  );

export const fileReadHandle = (handle) =>
  Effect.tryPromise({ try: () => handle.readFile({ encoding: "utf8" }), catch: failure("ReadError") });

export const fileRead = (path) =>
  Effect.tryPromise({ try: () => readFile(path, "utf8"), catch: failure("ReadError") });

export const fileWrite = (path, text) =>
  Effect.tryPromise({ try: () => writeFile(path, text), catch: failure("WriteError") });
