// Ahead-of-time compiled decoders (`schema-aot`). The build imports the
// app's modules with collection on, hands the codecs they made while loading
// to Effect's `SchemaAOTCompiler` and writes the static module it generates.
// The app's runner collects the same codecs in the same order and installs
// that module; a module that no longer matches is skipped, leaving the
// interpreter.
import type { SchemaAST } from "effect";
import { SchemaAOTCompiler } from "effect/schema";
import * as compilerRuntime from "effect/schema/SchemaCompiler/runtime";

import { notesOf } from "./notes.ts";
import type { AnySchema } from "./schemas.ts";

/**
 * What the generated module imports in place of Effect's compiler runtime,
 * which apps cannot import themselves. Effect declares no type for it.
 */
export const schemaCompilerRuntime: unknown = Reflect.get(compilerRuntime, "runtime");

/** The generated module of compiled decoders. */
export interface CompiledSchemas {
  readonly fingerprint: string;
  readonly install: (asts: ReadonlyArray<SchemaAST.AST>) => void;
}

let collected: Array<SchemaAST.AST> | undefined;

/** Records a codec made while collecting; returns it. */
export const registerCodec = <S extends AnySchema>(codec: S): S => {
  if (collected && !collected.includes(codec.ast)) collected.push(codec.ast);
  return codec;
};

/** Starts collecting the codecs the modules make as they load. */
export const enableSchemaAot = (): void => {
  collected ??= [];
};

const stop = (): Array<SchemaAST.AST> => {
  const asts = collected ?? [];
  collected = undefined;
  return asts;
};

/** A summary of the collected codecs, to tell a stale module apart. */
const fingerprint = (asts: ReadonlyArray<SchemaAST.AST>): string =>
  asts
    .map((ast) => {
      const notes = notesOf(ast);
      return `${ast._tag}:${notes["flick/block"] ?? notes.expected ?? ""}`;
    })
    .join("|");

/** Whether a module is one `writeSchemaAot` generated. */
const isCompiledSchemas = (module: unknown): module is CompiledSchemas =>
  typeof module === "object" &&
  module !== null &&
  typeof Reflect.get(module, "fingerprint") === "string" &&
  typeof Reflect.get(module, "install") === "function";

/** Installs the generated module `aot` for the codecs collected so far; whether it matched. */
export const installSchemaAot = (aot: unknown): boolean => {
  const asts = stop();
  if (!isCompiledSchemas(aot) || aot.fingerprint !== fingerprint(asts)) {
    process.stderr.write("warning: the compiled schemas are stale, decoding is interpreted; rebuild with --schema-aot\n");
    return false;
  }
  aot.install(asts);
  return true;
};

/** Imports the module at `entry`, a path or a `file:` URL, and writes the compiled decoders of its codecs to `out`. */
export const writeSchemaAot = async (entry: string, out: string): Promise<void> => {
  const [{ writeFileSync }, { pathToFileURL }] = await Promise.all([import("node:fs"), import("node:url")]);
  enableSchemaAot();
  await import(entry.startsWith("file:") ? entry : pathToFileURL(entry).href);
  const asts = stop();
  const source = SchemaAOTCompiler.compile(asts.map((ast) => ({ ast, operations: ["decode"] }))).replace(
    'import { runtime } from "effect/schema/SchemaCompiler/runtime";',
    'import { schemaCompilerRuntime as runtime } from "@flick-lang/runtime";',
  );
  writeFileSync(out, `${source}\nexport const fingerprint = ${JSON.stringify(fingerprint(asts))};\n`);
};
