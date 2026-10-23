import assert from "node:assert/strict";
import { rmSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { Effect, Exit, Option } from "effect";

import {
  codecBlock,
  codecCheckWhole,
  codecDecode,
  codecJsonSchema,
  codecList,
  codecNumberFinite,
  codecString,
  dslField,
  dslOnMissing,
  dslOptionalField,
  enableSchemaAot,
  installSchemaAot,
  schemaArbitrary,
  schemaJsonCodec,
  schemaNumber,
  schemaOption,
  schemaRecord,
  schemaRefined,
  schemaString,
  schemaValidate,
  writeSchemaAot,
} from "../src/index.js";
import { codecs, tree } from "./fixtures/codecs.js";

/** A small seeded generator of JSON-like inputs, NaN and undefined included. */
const random = (seed) => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), a | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

const KEYS = ["name", "nick", "note", "alias", "count", "xs", "s", "_tag", "_0", "extra"];

const input = (rng, depth = 0) => {
  const r = rng();
  if (depth > 3 || r < 0.35) {
    const leaves = [null, undefined, Number.NaN, 1, -2.5, "x", "abc", "Leaf", "Branch", true];
    return leaves[Math.floor(rng() * leaves.length)];
  }
  if (r < 0.6) return Array.from({ length: Math.floor(rng() * 3) }, () => input(rng, depth + 1));
  const out = {};
  for (const key of KEYS) if (rng() < 0.5) out[key] = input(rng, depth + 1);
  return out;
};

const outcome = (codec, value) => {
  const exit = Effect.runSyncExit(codecDecode(codec, value));
  return Exit.isSuccess(exit) ? { value: exit.value } : { issues: exit.cause.reasons[0].error._0 };
};

test("compiled decoders agree with the interpreter on every input", async () => {
  const entry = new URL("./fixtures/codecs.js?generate", import.meta.url).href;
  const out = fileURLToPath(new URL("./fixtures/codecs.aot.js", import.meta.url));
  try {
    await writeSchemaAot(entry, out);
    enableSchemaAot();
    const compiled = await import("./fixtures/codecs.js?compiled");
    assert.ok(installSchemaAot(await import("./fixtures/codecs.aot.js")), "the module matches");
    for (const [name, codec] of Object.entries(codecs)) {
      for (let seed = 0; seed < 400; seed++) {
        const value = input(random(seed));
        assert.deepEqual(
          outcome(compiled.codecs[name], value),
          outcome(codec, value),
          `${name}, seed ${seed}: ${JSON.stringify(value)}`,
        );
      }
    }
  } finally {
    rmSync(out, { force: true });
  }
});

test("a stale compiled module is not installed", () => {
  enableSchemaAot();
  schemaJsonCodec(schemaRecord([["a", schemaNumber]]));
  assert.equal(installSchemaAot({ fingerprint: "other", install: () => assert.fail("installed") }), false);
});

test("a pure codec with a missing default calls it once per decode", () => {
  let calls = 0;
  const codec = codecBlock("R", [["n", dslOnMissing(() => ++calls, dslField("n", codecNumberFinite))]]);
  assert.deepEqual(outcome(codec, {}), { value: { n: 1 } });
  assert.equal(calls, 1);
});

test("decoding refuses cycles and getters and accepts shared values", () => {
  const leaf = { _tag: "Leaf", _0: 1 };
  assert.ok("value" in outcome(codecs.tree, { _tag: "Branch", _0: [leaf, leaf] }));
  const node = { _tag: "Branch", _0: [] };
  node._0.push(node);
  assert.deepEqual(outcome(codecs.tree, node).issues, [{ path: "_0[0]", message: "a cycle in the input" }]);
  const getter = Object.defineProperty({ _tag: "Leaf" }, "_0", { get: () => 1, enumerable: true });
  assert.deepEqual(outcome(codecs.tree, getter).issues, [{ path: "_0", message: "a getter, not data" }]);
});

const SIMPLE_TYPES = new Set(["array", "boolean", "integer", "null", "number", "object", "string"]);

/** The rules of the JSON Schema 2020-12 meta-schema for the keywords the exporter writes. */
const valid = (schema, path = "#") => {
  if (typeof schema === "boolean") return;
  assert.equal(typeof schema, "object", path);
  assert.ok(Object.keys(schema).length > 0, `${path}: an empty schema accepts anything`);
  const schemas = (list, at) => {
    assert.ok(Array.isArray(list) && list.length > 0, at);
    list.forEach((s, i) => valid(s, `${at}/${i}`));
  };
  for (const [key, value] of Object.entries(schema)) {
    const at = `${path}/${key}`;
    switch (key) {
      case "$schema":
      case "$ref":
      case "format":
      case "pattern":
        assert.equal(typeof value, "string", at);
        break;
      case "type":
        assert.ok(SIMPLE_TYPES.has(value), at);
        break;
      case "properties":
      case "$defs":
        for (const [k, s] of Object.entries(value)) valid(s, `${at}/${k}`);
        break;
      case "required":
        assert.ok(Array.isArray(value) && new Set(value).size === value.length, at);
        assert.ok(value.every((v) => typeof v === "string"), at);
        break;
      case "additionalProperties":
      case "items":
      case "not":
        valid(value, at);
        break;
      case "prefixItems":
      case "anyOf":
      case "oneOf":
        schemas(value, at);
        break;
      case "enum":
        assert.ok(Array.isArray(value), at);
        break;
      case "const":
        break;
      case "minLength":
        assert.ok(Number.isInteger(value) && value >= 0, at);
        break;
      case "exclusiveMinimum":
        assert.equal(typeof value, "number", at);
        break;
      default:
        assert.fail(`${at}: a keyword the exporter does not write`);
    }
  }
};

test("exported JSON Schemas follow the 2020-12 meta-schema and never accept anything", () => {
  const email = schemaRefined("Email", schemaString, (s) => s.includes("@"), "email");
  const profile = codecBlock("Profile", [
    ["email", dslField("email", schemaJsonCodec(email))],
    ["tags", dslField("tags", codecList(codecString))],
    ["nick", dslOptionalField("nick", codecString)],
    ["tree", dslField("tree", schemaJsonCodec(tree))],
    ["maybe", dslField("maybe", schemaJsonCodec(schemaOption(schemaNumber)))],
  ]);
  for (const codec of [profile, codecs.tree, schemaJsonCodec(schemaRecord([["a", schemaNumber]]))]) {
    const out = codecJsonSchema("Root", codec);
    assert.equal(out._tag, "Success", JSON.stringify(out));
    const json = JSON.parse(out.success);
    assert.equal(json.$schema, "https://json-schema.org/draft/2020-12/schema");
    valid(json);
    assert.ok(!out.success.includes("{}"), out.success);
  }
  const unsupported = codecJsonSchema("Root", codecCheckWhole((s) => s.length < 3, "too long", codecString));
  assert.equal(unsupported._tag, "Failure");
  assert.match(unsupported.failure._0, /^Root -> JSON Schema: the check "too long"/);
});

test("arbitrary values are values of their schema", () => {
  const schema = schemaRecord([
    ["name", schemaString],
    ["tree", tree],
    ["maybe", schemaOption(schemaNumber)],
  ]);
  const gen = schemaArbitrary(schema);
  for (let seed = 0; seed < 50; seed++) {
    const value = gen(random(seed));
    assert.equal(schemaValidate(schema, value)._tag, "Success", JSON.stringify(value));
  }
  assert.ok(Option.isOption(gen(random(1)).maybe));
  const refined = schemaRefined("Even", schemaNumber, (n) => n % 2 === 0, null);
  assert.throws(() => schemaArbitrary(refined)(random(1)), /needs an explicit generator/);
});
