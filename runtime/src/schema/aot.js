// Ahead-of-time compiled decoders (`schema-aot`). The build imports the
// app's modules with collection on, hands the codecs they made while loading
// to Effect's `SchemaAOTCompiler` and writes the static module it generates.
// The app's runner collects the same codecs in the same order and installs
// that module; a module that no longer matches is skipped, leaving the
// interpreter.
import { SchemaAOTCompiler } from "effect/schema";
import { runtime } from "effect/schema/SchemaCompiler/runtime";

/** What the generated module imports in place of Effect's compiler runtime. */
export const schemaCompilerRuntime = runtime;

let collected;

/** Records a codec made while collecting; returns it. */
export const registerCodec = (codec) => {
  if (collected && !collected.includes(codec.ast)) collected.push(codec.ast);
  return codec;
};

/** Starts collecting the codecs the modules make as they load. */
export const enableSchemaAot = () => {
  collected ??= [];
};

const stop = () => {
  const asts = collected ?? [];
  collected = undefined;
  return asts;
};

/** A summary of the collected codecs, to tell a stale module apart. */
const fingerprint = (asts) =>
  asts.map((ast) => `${ast._tag}:${ast.annotations?.["flick/block"] ?? ast.annotations?.expected ?? ""}`).join("|");

/** Installs the generated module `aot` for the codecs collected so far; whether it matched. */
export const installSchemaAot = (aot) => {
  const asts = stop();
  if (aot.fingerprint !== fingerprint(asts)) {
    process.stderr.write("warning: the compiled schemas are stale, decoding is interpreted; rebuild with --schema-aot\n");
    return false;
  }
  aot.install(asts);
  return true;
};

/** Imports the module at `entry`, a path or a `file:` URL, and writes the compiled decoders of its codecs to `out`. */
export const writeSchemaAot = async (entry, out) => {
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
