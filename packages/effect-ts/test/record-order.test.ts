// Issue #740: structural records have no observable storage identity or written
// field order across Effect-backed builtins, schemas, codec blocks, and Struct.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Cache, Chunk, Effect, Equal, Hash, Option, Result, Schema } from "effect";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";

import {
  codecBlock,
  codecDecodeResult,
  codecEncodeResult,
  codecIgnoreExtra,
  codecInt,
  codecJsonSchema,
  codecString,
  copyData,
  dslField,
  dslOptionalField,
  equals as flickEquals,
  gadt,
  hash as flickHash,
  sameValue as flickSameValue,
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
import { equals as filterEquals, equalsStrict as filterEqualsStrict } from "../src/effect/Filter.ts";
import { flickModify, flickModifyAt, flickModifyHash, flickSetMany } from "../src/effect/HashMap.ts";
import { flickToType } from "../src/effect/Schema.ts";
import { flickMakeCombiner, flickMakeReducer } from "../src/effect/Struct.ts";
import * as TxHashMap from "../src/effect/TxHashMap.ts";
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
  const g1 = Result.succeed(gadt({ _tag: "Box", _0: 1, $0: { tag: "first" } }));
  const g2 = Result.succeed(gadt({ _tag: "Box", _0: 1, $0: { tag: "second" } }));
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

test("Effect Filter.equalsStrict uses structural SameValue without observing data identity", () => {
  const r1 = { z: 1, a: "hi" };
  const r2 = { a: "hi", z: 1 };
  assert.equal(Result.isSuccess(filterEqualsStrict(r1)(r2)), true);
  assert.equal(Result.isSuccess(filterEqualsStrict([1, r1])([1, r2])), true);
  assert.equal(Result.isFailure(filterEqualsStrict([1, r1])([2, r2])), true);
  assert.equal(Result.isFailure(filterEqualsStrict(0)(-0)), true);
  assert.equal(Result.isSuccess(filterEqualsStrict(-0)(-0)), true);
  assert.equal(Result.isSuccess(filterEqualsStrict(NaN)(NaN)), true);
  assert.equal(Result.isSuccess(filterEquals(0)(-0)), true);

  const c1 = refMake(1);
  const c2 = refMake(1);
  assert.equal(Result.isSuccess(filterEqualsStrict(c1)(c1)), true);
  assert.equal(Result.isFailure(filterEqualsStrict(c1)(c2)), true);
  const f1 = new Map<string, number>();
  const f2 = new Map<string, number>();
  assert.equal(Result.isSuccess(filterEqualsStrict(f1)(f1)), true);
  assert.equal(Result.isFailure(filterEqualsStrict(f1)(f2)), true);

  const g1 = gadt({ _tag: "Box", _0: 0, $0: { show: (): string => "a" } });
  const g2 = gadt({ _tag: "Box", _0: 0, $0: { show: (): string => "b" } });
  const gNeg = gadt({ _tag: "Box", _0: -0, $0: { show: (): string => "c" } });
  assert.equal(Result.isSuccess(filterEqualsStrict(g1)(g2)), true);
  assert.equal(Result.isFailure(filterEqualsStrict(g1)(gNeg)), true);
  assert.equal(Result.isSuccess(filterEquals(g1)(gNeg)), true);
});

test("Effect TxHashMap overwrites replace keys unconditionally without observing value identity or conflating -0.0", () => {
  interface Key {
    readonly id: number;
    readonly tag: string;
  }
  const byId = { eq: (a: Key, b: Key) => a.id === b.id, hash: () => 7 };
  const shared = { a: 1 };
  const k1: Key = { id: 1, tag: "old" };
  const k2: Key = { id: 1, tag: "new" };

  const keyTag = (m: TxHashMap.FlickTxHashMap<Key, { readonly a: number }>): string => {
    const entries = Effect.runSync(TxHashMap.flickEntries(m));
    assert.equal(entries.length, 1);
    const first = entries[0];
    assert.ok(first !== undefined);
    return first[0].tag;
  };

  assert.equal(keyTag(Effect.runSync(TxHashMap.flickMake(byId, [[k1, shared], [k2, shared]]))), "new");
  assert.equal(keyTag(Effect.runSync(TxHashMap.flickFromIterable(byId, [[k1, shared], [k2, shared]]))), "new");

  const mSet = Effect.runSync(TxHashMap.flickMake(byId, [[k1, shared]]));
  Effect.runSync(TxHashMap.flickSet(k2, shared, mSet));
  assert.equal(keyTag(mSet), "new");

  const mMod = Effect.runSync(TxHashMap.flickMake(byId, [[k1, shared]]));
  const prev = Effect.runSync(TxHashMap.flickModify(k2, () => shared, mMod));
  assert.ok(Option.isSome(prev));
  assert.deepEqual(prev.value, shared);
  assert.equal(keyTag(mMod), "new");

  const mModAt = Effect.runSync(TxHashMap.flickMake(byId, [[k1, shared]]));
  Effect.runSync(TxHashMap.flickModifyAt(k2, () => Option.some(shared), mModAt));
  assert.equal(keyTag(mModAt), "new");

  const mUnion = Effect.runSync(TxHashMap.flickMake(byId, [[k1, shared]]));
  Effect.runSync(TxHashMap.flickUnion(hashMapFromSeq(byId, [[k2, shared]]), mUnion));
  assert.equal(keyTag(mUnion), "new");

  const mMany = Effect.runSync(TxHashMap.flickMake(byId, [[k1, shared]]));
  Effect.runSync(TxHashMap.flickSetMany([[k2, shared]], mMany));
  assert.equal(keyTag(mMany), "new");

  const mFlatSrc = Effect.runSync(
    TxHashMap.flickMake(
      { eq: (a: number, b: number) => a === b, hash: (n: number) => n },
      [[1, k1], [2, k2]],
    ),
  );
  const mFlat = Effect.runSync(
    TxHashMap.flickFlatMap(
      (k) => TxHashMap.flickMake(byId, [[k, shared]]),
      // Different key type: build over Key directly with two distinct keys mapping to k1 then k2
      Effect.runSync(TxHashMap.flickMake(byId, [[{ id: 10, tag: "a" }, k1], [{ id: 20, tag: "b" }, k2]])),
    ),
  );
  assert.equal(Effect.runSync(TxHashMap.flickSize(mFlatSrc)), 2);
  assert.equal(keyTag(mFlat), "new");

  // Overwriting 0.0 with -0.0 preserves -0.0
  const strWitness = { eq: (a: string, b: string) => a === b, hash: (s: string) => Hash.string(s) };
  const zm = Effect.runSync(TxHashMap.flickMake(strWitness, [["z", 0]]));
  Effect.runSync(TxHashMap.flickSet("z", -0, zm));
  const zgot = Effect.runSync(TxHashMap.flickGet("z", zm));
  assert.ok(Option.isSome(zgot));
  assert.equal(Object.is(zgot.value, -0), true);
});

test("Effect GADT values skip held dictionaries while plain $-prefixed objects and Effect Chunks preserve exact semantics", () => {
  const g1 = gadt({ _tag: "Box", _0: { z: 1, a: "x" }, $0: { show: (): string => "first" } });
  const g2 = gadt({ _tag: "Box", _0: { a: "x", z: 1 }, $0: { show: (): string => "second" } });

  // Direct Flick equals/hash skip held dictionaries on gadt-tagged values
  assert.equal(flickEquals(g1, g2), true);
  assert.equal(flickSameValue(g1, g2), true);
  assert.equal(flickHash(g1), flickHash(g2));

  // Plain objects (Json / ReadonlyRecord) preserve $-prefixed keys ($ref, $schema, $defs, $id)
  const refA: Record<string, string> = { $ref: "A" };
  const refB: Record<string, string> = { $ref: "B" };
  const schemaV1: Record<string, string> = { $schema: "v1" };
  assert.equal(flickEquals(refA, refB), false);
  assert.equal(flickEquals(schemaV1, {}), false);
  assert.equal(flickSameValue(refA, refB), false);
  assert.equal(flickSameValue(schemaV1, {}), false);
  assert.notEqual(flickHash(refA), flickHash(refB));
  assert.notEqual(flickHash(schemaV1), flickHash({}));
  assert.equal(Equal.equals(refA, refB), false);
  assert.notEqual(Hash.hash(refA), Hash.hash(refB));
  assert.equal(Result.isFailure(filterEquals(refA)(refB)), true);
  assert.equal(Result.isFailure(filterEqualsStrict(refA)(refB)), true);
  assert.equal(Result.isFailure(filterEqualsStrict(schemaV1)({})), true);

  const byKey = family((key: unknown) => ({ key }));
  assert.notEqual(byKey(refA), byKey(refB));
  assert.notEqual(byKey(schemaV1), byKey({}));
  const registry = AtomRegistry.make();
  const doc = flickState<Record<string, string>>(refA);
  let docCalls = 0;
  registry.subscribe(doc, () => { docCalls++; }, { immediate: true });
  registry.set(doc, refB);
  assert.equal(docCalls, 2);

  // Effect Chunk distinguishes 0.0 from -0.0 under sameValue, Filter.equalsStrict, and Atom updates
  const chPos = Chunk.fromIterable([0]);
  const chNeg = Chunk.fromIterable([-0]);
  const chRecPos = Chunk.fromIterable([{ x: 0 }]);
  const chRecNeg = Chunk.fromIterable([{ x: -0 }]);
  assert.equal(flickEquals(chPos, chNeg), true);
  assert.equal(flickSameValue(chPos, chNeg), false);
  assert.equal(flickSameValue(chRecPos, chRecNeg), false);
  assert.equal(flickSameValue(chPos, [0]), false);
  assert.equal(Result.isFailure(filterEqualsStrict(chPos)(chNeg)), true);
  assert.equal(Result.isFailure(filterEqualsStrict(chRecPos)(chRecNeg)), true);
  assert.equal(Result.isSuccess(filterEquals(chPos)(chNeg)), true);
  const chunkAtom = flickState(chPos);
  let chunkCalls = 0;
  registry.subscribe(chunkAtom, () => { chunkCalls++; }, { immediate: true });
  registry.set(chunkAtom, chNeg);
  assert.equal(chunkCalls, 2);

  // Effect's native Equal/Hash and polymorphic stdlib structures ignore $0 via GADT_PROTO
  assert.equal(Equal.equals(g1, g2), true);
  assert.equal(Hash.hash(g1), Hash.hash(g2));
  assert.equal(Equal.equals(Option.some(g1), Option.some(g2)), true);
  assert.equal(Hash.hash(Option.some(g1)), Hash.hash(Option.some(g2)));
  assert.equal(Hash.hash(Option.some(g1)), flickHash(Option.some(g1)));
  assert.equal(Hash.hash(Result.succeed(g1)), flickHash(Result.succeed(g1)));

  const deduped = Chunk.dedupe(Chunk.fromIterable([g1, g2]));
  assert.equal(deduped.length, 1);
  assert.equal(Result.isSuccess(filterEquals(g1)(g2)), true);

  // Polymorphic Effect Cache deduplicates lookups on GADT keys carrying distinct dictionaries
  let lookups = 0;
  const cached = Effect.runSync(
    Effect.gen(function* () {
      const cache = yield* Cache.make({
        capacity: 10,
        lookup: (k: typeof g1) =>
          Effect.sync(() => {
            lookups += 1;
            return k._0.z + 10;
          }),
      });
      const first = yield* Cache.get(cache, g1);
      const second = yield* Cache.get(cache, g2);
      return [first, second] as const;
    }),
  );
  assert.deepEqual(cached, [11, 11]);
  assert.equal(lookups, 1);

  // copyData preserves GADT prototype and held dictionaries
  const copied = copyData(g1);
  assert.notEqual(copied, g1);
  assert.equal(copied.$0.show(), "first");
  assert.equal(Equal.equals(copied, g2), true);
  assert.equal(Hash.hash(copied), Hash.hash(g2));
});
