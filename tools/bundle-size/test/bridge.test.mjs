import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { rolldown } from "rolldown";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");

test("a bundled schema-only bridge import retains interpreter initialization", async () => {
  const entry = path.join(root, "packages/effect-ts/test/bundle-entry.mjs");
  const source = `
    import { codecFromEffect } from "@flick-lang/effect-ts/bridge";
    import { Schema, SchemaGetter, Effect } from "effect";
    import { codecDecode } from "@flick-lang/runtime/rt/codec";
    import { currentCtx, fork } from "@flick-lang/runtime/rt/fiber";
    const host = Schema.Unknown.pipe(Schema.decodeTo(Schema.String, {
      decode: SchemaGetter.transformEffect((value) => Effect.delay(Effect.succeed(String(value)), 1)),
      encode: SchemaGetter.transform((value) => value),
    })).annotate({ "flick/effect": true });
    const codec = codecFromEffect(host);
    export const result = await new Promise((resolve, reject) => {
      fork(() => codecDecode(codec, 42), currentCtx()).observe((exit) => {
        if (exit.cause === undefined) resolve(exit.value);
        else reject(new Error(JSON.stringify(exit.cause, (_, value) => value instanceof Error ? value.message : value)));
      });
    });
  `;
  const bundle = await rolldown({
    input: entry,
    platform: "node",
    logLevel: "silent",
    plugins: [{ name: "bridge-fixture", resolveId: (id) => id === entry ? entry : undefined, load: (id) => id === entry ? source : undefined }],
  });
  try {
    const { output } = await bundle.generate({ format: "esm", codeSplitting: false, minify: true });
    const chunk = output.find((item) => item.type === "chunk");
    assert.ok(chunk);
    const module = await import(`data:text/javascript;base64,${Buffer.from(chunk.code).toString("base64")}`).catch((error) => { throw new Error(error.message); });
    assert.equal(module.result, "42");
  } finally {
    await bundle.close();
  }
});
