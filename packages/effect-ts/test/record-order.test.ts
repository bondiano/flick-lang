// Issue #740: structural records have no observable storage identity or written
// field order across Effect-backed builtins, schemas, codec blocks, and Struct.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Equal, Hash, Option, Result, Schema } from "effect";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";

import {
  codecBlock,
  codecDecodeResult,
  codecEncodeResult,
  codecIgnoreExtra,
  codecInt,
  codecJsonSchema,
  codecString,
  dslField,
  dslOptionalField,
  recordExtend,
  recordLabels,
  recordMergeDisjoint,
  recordProject,
  recordRemove,
  refMake,
  refSet,
  schemaInt,
  schemaJsonCodec,
  schemaRecord,
  schemaString,
} from "../src/index.ts";
import { flickModify, flickModifyAt, flickModifyHash, flickSetMany } from "../src/effect/HashMap.ts";
import { flickToType } from "../src/effect/Schema.ts";
import { flickMakeCombiner, flickMakeReducer } from "../src/effect/Struct.ts";
import { family, flickState, readable } from "../src/effect/reactivity/Atom.ts";
import {
  hashMapFromSeq,
  hashMapGet,
  hashMapInsert,
  hashMapToList,
  hashMapUnion,
  hashMapWith,
} from "../src/stdlib/hash.ts";

test("row primitives canonicalize structural record key order", () => {
  const ext1 = recordExtend(null, null, "a", 2, { z: 1 });
  const ext2 = recordExtend(null, null, "z", 1, { a: 2 });
  assert.deepEqual(Object.keys(ext1), ["a", "z"]);
  assert.deepEqual(Object.keys(ext2), ["a", "z"]);

  const merged1 = recordMergeDisjoint(null, { z: 1 }, { a: 2 });
  const merged2 = recordMergeDisjoint(null, { a: 2 }, { z: 1 });
  assert.deepEqual(Object.keys(merged1), ["a", "z"]);
  assert.deepEqual(Object.keys(merged2), ["a", "z"]);

  const proj = recordProject(["z", "a"], { z: 1, m: 3, a: 2 });
  const rem = recordRemove(null, "m", { z: 1, m: 3, a: 2 });
  assert.deepEqual(Object.keys(proj), ["a", "z"]);
  assert.deepEqual(Object.keys(rem), ["a", "z"]);
  assert.deepEqual(recordLabels(null, { z: 1, a: 2 }), ["a", "z"]);
});

const keysOf = (value: unknown): string[] => {
  assert.ok(typeof value === "object" && value !== null);
  return Object.keys(value);
};

test("schemaRecord and codecBlock decode canonically while preserving codec wire order and JSON Schema", () => {
  const rec = schemaRecord([["z", schemaInt], ["a", schemaString]]);
  const json = schemaJsonCodec(rec);
  const enc = codecEncodeResult(json, { z: 1, a: "hi" });
  assert.ok(Result.isSuccess(enc));
  assert.deepEqual(keysOf(enc.success), ["a", "z"]);

  const dec = codecDecodeResult(json, { z: 1, a: "hi" });
  assert.ok(Result.isSuccess(dec));
  assert.deepEqual(keysOf(dec.success), ["a", "z"]);

  const block = codecBlock("Pair", [
    ["z", dslField("wire_z", codecInt)],
    ["a", dslField("wire_a", codecString)],
  ]);
  const bDec = codecDecodeResult(block, { wire_z: 7, wire_a: "ok" });
  assert.ok(Result.isSuccess(bDec));
  assert.deepEqual(keysOf(bDec.success), ["a", "z"]);

  const bEnc = codecEncodeResult(block, { a: "ok", z: 7 });
  assert.ok(Result.isSuccess(bEnc));
  assert.deepEqual(keysOf(bEnc.success), ["wire_z", "wire_a"]);

  const exported = codecJsonSchema("Pair", block);
  assert.ok(Result.isSuccess(exported));
  const parsed: unknown = JSON.parse(exported.success);
  assert.ok(typeof parsed === "object" && parsed !== null);
  assert.deepEqual(Reflect.get(parsed, "required"), ["wire_z", "wire_a"]);
  assert.equal(Reflect.get(parsed, "additionalProperties"), false);

  const lax = codecIgnoreExtra(block);
  const laxDec = codecDecodeResult(lax, { wire_z: 7, wire_a: "ok", extra: true });
  assert.ok(Result.isSuccess(laxDec));
  assert.deepEqual(keysOf(laxDec.success), ["a", "z"]);

  const laxExport = codecJsonSchema("Pair", lax);
  assert.ok(Result.isSuccess(laxExport));
  const laxParsed: unknown = JSON.parse(laxExport.success);
  assert.ok(typeof laxParsed === "object" && laxParsed !== null);
  assert.equal(Reflect.get(laxParsed, "additionalProperties"), undefined);
});

test("Struct combiner and reducer emit canonical key order and evaluate fields canonically", () => {
  const log: string[] = [];
  const track = (name: string) => ({
    initialValue: 0,
    combine: (x: number, y: number) => {
      log.push(name);
      return x + y;
    },
    combineAll: (xs: Iterable<number>) => {
      let acc = 0;
      for (const x of xs) acc += x;
      return acc;
    },
  });
  const comb = flickMakeCombiner(null, { z: track("z"), a: track("a") });
  const out = comb.combine({ z: 1, a: 2 }, { a: 3, z: 4 });
  assert.deepEqual(Object.keys(out), ["a", "z"]);
  assert.deepEqual(log, ["a", "z"]);

  const red = flickMakeReducer(null, { z: track("z"), a: track("a") });
  assert.deepEqual(Object.keys(red.initialValue), ["a", "z"]);
});

test("unsorted codecBlock preserves Schema.toType validation, flick/block AOT metadata, and optional fields", () => {
  const block = codecBlock("Pair", [
    ["z", dslField("wire_z", codecInt)],
    ["a", dslOptionalField("wire_a", codecString)],
  ]);
  // Outer AST stays an Objects node annotated with flick/block for AOT fingerprinting
  assert.equal(block.ast._tag, "Objects");
  assert.equal(block.ast.annotations?.["flick/block"], "Pair");

  const decodedSchema = Schema.toType(block);
  assert.equal(Schema.is(decodedSchema)("garbage"), false);
  assert.equal(Schema.is(decodedSchema)({ a: Option.some("ok"), z: "not-int" }), false);
  assert.equal(Schema.is(decodedSchema)({ a: Option.some("ok"), z: 7 }), true);
  assert.equal(Schema.is(decodedSchema)({ a: Option.none(), z: 7 }), true);

  const fromCodec = flickToType(block);
  assert.equal(Schema.is(fromCodec)("garbage"), false);
  assert.equal(Schema.is(fromCodec)({ a: Option.some("ok"), z: 7 }), true);

  const dec = codecDecodeResult(block, { wire_z: 7 });
  assert.ok(Result.isSuccess(dec));
  assert.deepEqual(keysOf(dec.success), ["a", "z"]);
});

test("Effect Cell identity and Reactivity.Atom.family align with Flick structural/identity contract", () => {
  const c1 = refMake(1);
  const c2 = refMake(1);
  assert.equal(Equal.equals(c1, c2), false);
  assert.equal(Equal.equals(c1, c1), true);
  const h1 = Hash.hash(c1);
  refSet(c1, 2);
  assert.equal(Hash.hash(c1), h1);

  let created = 0;
  const byKey = family((key: unknown) => ({ id: ++created, key }));
  assert.equal(byKey({ z: 1, a: "x" }), byKey({ a: "x", z: 1 }));
  assert.equal(created, 1);

  // GADT held dictionary keys ($0) are ignored even inside Result/Option
  const g1 = Result.succeed({ _tag: "Box", _0: 1, $0: { tag: "first" } });
  const g2 = Result.succeed({ _tag: "Box", _0: 1, $0: { tag: "second" } });
  assert.equal(byKey(g1), byKey(g2));
  assert.equal(created, 2);

  // Distinct mutable cells never conflate and mutating a cell keeps its bucket
  const cellAtom = byKey(c1);
  assert.notEqual(cellAtom, byKey(c2));
  assert.equal(byKey(c1), cellAtom);
  refSet(c1, 3);
  assert.equal(byKey(c1), cellAtom);
  assert.equal(created, 4);
});

test("Effect HashMap overwrites replace keys unconditionally without observing value identity or conflating -0.0", () => {
  interface Key {
    readonly id: number;
    readonly tag: string;
  }
  const byId = { eq: (a: Key, b: Key) => a.id === b.id, hash: () => 7 };
  const shared = { a: 1 };
  const k1: Key = { id: 1, tag: "old" };
  const k2: Key = { id: 1, tag: "new" };
  const other: Key = { id: 2, tag: "other" };

  const m0 = hashMapInsert(other, { a: 9 }, hashMapInsert(k1, shared, hashMapWith<Key, { a: number }>(byId)));
  const mSame = hashMapInsert(k2, shared, m0);
  const mFresh = hashMapInsert(k2, { a: 1 }, m0);
  assert.deepEqual(hashMapToList(mSame), [[k2, shared], [other, { a: 9 }]]);
  assert.deepEqual(hashMapToList(mFresh), [[k2, { a: 1 }], [other, { a: 9 }]]);

  const built = hashMapFromSeq(byId, [[k1, shared], [other, { a: 9 }], [k2, shared]]);
  assert.deepEqual(hashMapToList(built), [[k2, shared], [other, { a: 9 }]]);

  const left = hashMapFromSeq(byId, [[k1, shared]]);
  const right = hashMapFromSeq(byId, [[k2, shared]]);
  assert.deepEqual(hashMapToList(hashMapUnion(left, right)), [[k2, shared]]);
  assert.deepEqual(hashMapToList(flickSetMany([[k2, shared]], left)), [[k2, shared]]);
  assert.deepEqual(hashMapToList(flickModifyAt(k2, () => Option.some(shared), left)), [[k2, shared]]);
  assert.deepEqual(hashMapToList(flickModify(k2, () => shared, left)), [[k2, shared]]);
  assert.deepEqual(hashMapToList(flickModifyHash(k2, 7, () => Option.some(shared), left)), [[k2, shared]]);

  const strWitness = { eq: (a: string, b: string) => a === b, hash: (s: string) => Hash.string(s) };
  const zm = hashMapInsert("z", -0, hashMapInsert("z", 0, hashMapWith<string, number>(strWitness)));
  const zgot = hashMapGet("z", zm);
  assert.ok(Option.isSome(zgot));
  assert.equal(Object.is(zgot.value, -0), true);
});

test("Effect reactive atoms compare values structurally with SameValue without observing data identity", () => {
  const registry = AtomRegistry.make();
  const rec = flickState<{ readonly x: number }>({ x: 1 });
  const derived = readable((get) => ({ x: get(rec).x }));
  const constantDerived = readable((get) => ({ tag: get(rec).x > 0 ? "pos" : "neg" }));
  let recCalls = 0;
  let derivedCalls = 0;
  let constantCalls = 0;
  registry.subscribe(rec, () => { recCalls++; }, { immediate: true });
  registry.subscribe(derived, () => { derivedCalls++; }, { immediate: true });
  registry.subscribe(constantDerived, () => { constantCalls++; }, { immediate: true });
  assert.equal(recCalls, 1);
  assert.equal(derivedCalls, 1);
  assert.equal(constantCalls, 1);

  registry.set(rec, { x: 1 });
  assert.equal(recCalls, 1);
  assert.equal(derivedCalls, 1);
  assert.equal(constantCalls, 1);

  registry.set(rec, { x: 2 });
  assert.equal(recCalls, 2);
  assert.equal(derivedCalls, 2);
  assert.equal(constantCalls, 1);

  const num = flickState(0);
  let numCalls = 0;
  registry.subscribe(num, () => { numCalls++; }, { immediate: true });
  registry.set(num, -0);
  assert.equal(numCalls, 2);

  const c1 = refMake(1);
  const c2 = refMake(1);
  const refAtom = flickState(c1);
  let refCalls = 0;
  registry.subscribe(refAtom, () => { refCalls++; }, { immediate: true });
  registry.set(refAtom, c2);
  assert.equal(refCalls, 2);
});
