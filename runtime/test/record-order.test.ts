// Issue #740: structural records have no observable storage identity or written
// field order across the own runtime's row primitives, schemas, codec blocks,
// struct combiners/reducers, JSON diffs, hashes, and reactive atom families.
import assert from "node:assert/strict";
import { test } from "node:test";
import { copyData, equals, gadt, gadtMarker, hash as coreHash, sameValue } from "../src/core/data.ts";
import { tag } from "../src/tags.ts";
import { Registry, readable, state } from "../src/rt/atoms/registry.ts";
import {
  recordExtend,
  recordLabels,
  recordMergeDisjoint,
  recordProject,
  recordRemove,
  refMake,
  refSet,
} from "../src/rt/builtins.ts";
import { codecBlock, codecInt, codecString, dslField } from "../src/rt/codec.ts";
import {
  hashMapFromList,
  hashMapGet,
  hashMapInsert,
  hashMapToList,
  hashMapUnion,
  hashMapWith,
} from "../src/rt/collections.ts";
import { currentCtx } from "../src/rt/fiber.ts";
import { hash as rtHash } from "../src/rt/hash.ts";
import { schemaInt, schemaJsonCodec, schemaNumber, schemaRecordOf, schemaString } from "../src/rt/schema.ts";
import { flickDiff } from "../src/rt/effect/Differ.ts";
import { equals as filterEquals, equalsStrict as filterEqualsStrict } from "../src/rt/effect/Filter.ts";
import { flickGet } from "../src/rt/effect/JsonPatch.ts";
import { flickMakeCombiner, flickMakeReducer } from "../src/rt/effect/Struct.ts";
import { family } from "../src/rt/effect/reactivity/Atom.ts";

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

test("schemaRecordOf and codecBlock decode into canonical order while preserving codec wire order", () => {
  const recSchema = schemaRecordOf([["z", schemaInt], ["a", schemaString]]);
  const jsonCodec = schemaJsonCodec(recSchema);
  const enc = jsonCodec.encode({ z: 1, a: "hi" });
  assert.equal(enc._tag, "Success");
  if (enc._tag === "Success") assert.deepEqual(keysOf(enc.success), ["a", "z"]);

  const dec = jsonCodec.decode({ z: 1, a: "hi" });
  assert.equal(dec._tag, "Success");
  if (dec._tag === "Success") assert.deepEqual(keysOf(dec.success), ["a", "z"]);

  const block = codecBlock("Pair", [
    ["z", dslField("wire_z", codecInt)],
    ["a", dslField("wire_a", codecString)],
  ]);
  const bDec = block.decode({ wire_z: 7, wire_a: "ok" });
  assert.equal(bDec._tag, "Success");
  if (bDec._tag === "Success") assert.deepEqual(keysOf(bDec.success), ["a", "z"]);

  const bEnc = block.encode({ a: "ok", z: 7 });
  assert.equal(bEnc._tag, "Success");
  if (bEnc._tag === "Success") assert.deepEqual(keysOf(bEnc.success), ["wire_z", "wire_a"]);
});

test("Struct combiner and reducer emit canonical key order and evaluate fields canonically", () => {
  const log: string[] = [];
  const track = (name: string) => ({
    initial: 0,
    combine: (x: unknown, y: unknown) => {
      log.push(name);
      return Number(x) + Number(y);
    },
  });
  const comb = flickMakeCombiner(null, { z: track("z"), a: track("a") });
  const out = comb.combine({ z: 1, a: 2 }, { a: 3, z: 4 });
  assert.deepEqual(Object.keys(out), ["a", "z"]);
  assert.deepEqual(log, ["a", "z"]);

  const red = flickMakeReducer(null, { z: track("z"), a: track("a") });
  assert.deepEqual(Object.keys(red.initial), ["a", "z"]);
});

test("Differ and JsonPatch emit object patch operations in canonical key order", () => {
  const codec = schemaJsonCodec(schemaRecordOf([["z", schemaInt], ["a", schemaInt]]));
  const diff1 = flickDiff({ z: 1, a: 2 }, { z: 10, a: 20 }, { codec });
  const diff2 = flickDiff({ a: 2, z: 1 }, { a: 20, z: 10 }, { codec });
  assert.deepEqual(diff1, [
    { op: "replace", path: "/a", value: 20 },
    { op: "replace", path: "/z", value: 10 },
  ]);
  assert.deepEqual(diff1, diff2);

  const patch = flickGet({ z: 1, m: 2 }, { b: 3, a: 4 });
  assert.deepEqual(
    patch.map((op) => op._0),
    ["/a", "/b", "/m", "/z"],
  );
});

test("Reactivity.Atom.family keys structurally on data values without observing identity or field order", () => {
  let created = 0;
  const byKey = family((key: unknown) => ({ id: ++created, key }));
  const r1 = byKey({ z: 1, a: "x" });
  const r2 = byKey({ a: "x", z: 1 });
  assert.equal(r1, r2);
  assert.equal(created, 1);

  const t1 = byKey([1, { b: true, a: false }]);
  const t2 = byKey([1, { a: false, b: true }]);
  assert.equal(t1, t2);
  assert.equal(created, 2);

  // Explicit mutable cells retain identity and stable hashes across mutations
  const c1 = refMake(1);
  const c2 = refMake(1);
  const beforeHash = coreHash(c1);
  const atom1 = byKey(c1);
  assert.notEqual(atom1, byKey(c2));
  assert.equal(byKey(c1), atom1);
  refSet(c1, 2);
  assert.equal(coreHash(c1), beforeHash);
  assert.equal(byKey(c1), atom1);
  assert.equal(created, 4);
});

test("rt hash ignores written field order and GADT held dictionary keys", () => {
  const a = { _tag: "Box", _0: 1, $0: { tag: "first" } };
  const b = { _tag: "Box", _0: 1, $0: { tag: "second" } };
  assert.equal(equals(a, b), true);
  assert.equal(coreHash(a), coreHash(b));
  assert.equal(rtHash(a), rtHash(b));
  assert.equal(rtHash({ z: 1, a: 2 }), rtHash({ a: 2, z: 1 }));
});

test("HashMap overwrites replace keys unconditionally without observing value identity or conflating -0.0", () => {
  interface Key {
    readonly id: number;
    readonly tag: string;
  }
  const byId = { eq: (a: Key, b: Key) => a.id === b.id, hash: () => 7 };
  const shared = { a: 1 };
  const k1: Key = { id: 1, tag: "old" };
  const k2: Key = { id: 1, tag: "new" };
  const other: Key = { id: 2, tag: "other" };

  // Overwrite with shared value reference vs fresh record allocation both replace the key
  const m0 = hashMapInsert(other, { a: 9 }, hashMapInsert(k1, shared, hashMapWith<Key, { a: number }>(byId)));
  const mSame = hashMapInsert(k2, shared, m0);
  const mFresh = hashMapInsert(k2, { a: 1 }, m0);
  assert.deepEqual(hashMapToList(mSame), [[k2, shared], [other, { a: 9 }]]);
  assert.deepEqual(hashMapToList(mFresh), [[k2, { a: 1 }], [other, { a: 9 }]]);

  // Batch construction from list also replaces the key when the value shares storage
  const built = hashMapFromList(byId, [[k1, shared], [other, { a: 9 }], [k2, shared]]);
  assert.deepEqual(hashMapToList(built), [[k2, shared], [other, { a: 9 }]]);

  // Union replaces the left key with the right key even when values share storage
  const left = hashMapFromList(byId, [[k1, shared]]);
  const right = hashMapFromList(byId, [[k2, shared]]);
  assert.deepEqual(hashMapToList(hashMapUnion(left, right)), [[k2, shared]]);

  // Overwriting 0.0 with -0.0 preserves -0.0
  const strWitness = { eq: (a: string, b: string) => a === b, hash: (s: string) => coreHash(s) };
  const zm = hashMapInsert("z", -0, hashMapInsert("z", 0, hashMapWith<string, number>(strWitness)));
  const zgot = hashMapGet("z", zm);
  assert.equal(zgot._tag, "Some");
  if (zgot._tag === "Some") assert.equal(Object.is(zgot.value, -0), true);
});

test("RT reactive atoms compare values structurally with SameValue without observing data identity", () => {
  assert.equal(sameValue({ x: 1 }, { x: 1 }), true);
  assert.equal(sameValue(0, -0), false);
  assert.equal(sameValue(NaN, NaN), true);

  const registry = new Registry(currentCtx());
  const rec = state<{ readonly x: number }>({ x: 1 });
  const derived = readable((r) => ({ x: r.get(rec).x }));
  const constantDerived = readable((r) => ({ tag: r.get(rec).x > 0 ? "pos" : "neg" }));
  let recCalls = 0;
  let derivedCalls = 0;
  let constantCalls = 0;
  registry.subscribe(rec, () => recCalls++, true);
  registry.subscribe(derived, () => derivedCalls++, true);
  registry.subscribe(constantDerived, () => constantCalls++, true);
  assert.equal(recCalls, 1);
  assert.equal(derivedCalls, 1);
  assert.equal(constantCalls, 1);

  // Fresh equal record suppresses redundant notifications just like a shared reference
  registry.set(rec, { x: 1 });
  assert.equal(recCalls, 1);
  assert.equal(derivedCalls, 1);
  assert.equal(constantCalls, 1);

  // Changing rec to { x: 2 } notifies rec & derived, while constantDerived recomputes
  // a fresh equal record { tag: "pos" } and suppresses notification
  registry.set(rec, { x: 2 });
  assert.equal(recCalls, 2);
  assert.equal(derivedCalls, 2);
  assert.equal(constantCalls, 1);

  // -0.0 vs 0.0 and distinct mutable cells still notify
  const num = state(0);
  let numCalls = 0;
  registry.subscribe(num, () => numCalls++, true);
  registry.set(num, -0);
  assert.equal(numCalls, 2);

  const c1 = refMake(1);
  const c2 = refMake(1);
  const refAtom = state(c1);
  let refCalls = 0;
  registry.subscribe(refAtom, () => refCalls++, true);
  registry.set(refAtom, c2);
  assert.equal(refCalls, 2);
});

test("RT Filter.equalsStrict uses structural SameValue without observing data identity", () => {
  const r1 = { z: 1, a: "hi" };
  const r2 = { a: "hi", z: 1 };
  assert.equal(filterEqualsStrict(r1)(r2)._tag, "Success");
  assert.equal(filterEqualsStrict([1, r1])([1, r2])._tag, "Success");
  assert.equal(filterEqualsStrict([1, r1])([2, r2])._tag, "Failure");
  assert.equal(filterEqualsStrict(0)(-0)._tag, "Failure");
  assert.equal(filterEqualsStrict(-0)(-0)._tag, "Success");
  assert.equal(filterEqualsStrict(NaN)(NaN)._tag, "Success");
  assert.equal(filterEquals(0)(-0)._tag, "Success");

  // Mutable cells and foreign instances retain identity
  const c1 = refMake(1);
  const c2 = refMake(1);
  assert.equal(filterEqualsStrict(c1)(c1)._tag, "Success");
  assert.equal(filterEqualsStrict(c1)(c2)._tag, "Failure");
  const f1 = new Map<string, number>();
  const f2 = new Map<string, number>();
  assert.equal(filterEqualsStrict(f1)(f1)._tag, "Success");
  assert.equal(filterEqualsStrict(f1)(f2)._tag, "Failure");

  // GADT values ignore held dictionaries while still distinguishing 0.0 vs -0.0
  const g1 = gadt({ _tag: "Box", _0: 0, $0: { tag: "first" } });
  const g2 = gadt({ _tag: "Box", _0: 0, $0: { tag: "second" } });
  const gNeg = gadt({ _tag: "Box", _0: -0, $0: { tag: "third" } });
  assert.equal(filterEqualsStrict(g1)(g2)._tag, "Success");
  assert.equal(filterEqualsStrict(g1)(gNeg)._tag, "Failure");
  assert.equal(filterEquals(g1)(gNeg)._tag, "Success");
});

test("RT Differ and JsonPatch distinguish 0.0 from -0.0", () => {
  const patch = flickGet(0, -0);
  assert.equal(patch.length, 1);
  const op = patch[0];
  assert.ok(op !== undefined && op._tag === tag("JsonPatch.Operation.Replace"));
  assert.equal(op._0, "");
  assert.equal(Object.is(op._1, -0), true);

  const codec = schemaJsonCodec(schemaNumber);
  const diff = flickDiff(0, -0, { codec });
  assert.equal(diff.length, 1);
  assert.deepEqual(diff[0], { op: "replace", path: "", value: -0 });
  assert.equal(Object.is(diff[0]?.value, -0), true);
});

test("RT gadt values preserve held dictionaries through copyData while skipping them in equals/hash", () => {
  const d1 = { show: () => "first" };
  const d2 = { show: () => "second" };
  const g1 = gadt({ _tag: "Box", _0: { z: 1, a: "x" }, $0: d1 });
  const g2 = gadt({ _tag: "Box", _0: { a: "x", z: 1 }, $0: d2 });
  assert.equal(gadtMarker in g1, true);
  assert.equal(equals(g1, g2), true);
  assert.equal(sameValue(g1, g2), true);
  assert.equal(coreHash(g1), coreHash(g2));
  assert.equal(rtHash(g1), rtHash(g2));

  const copied = copyData(g1);
  assert.notEqual(copied, g1);
  assert.equal(gadtMarker in copied, true);
  assert.equal(copied.$0.show(), "first");
  assert.equal(equals(copied, g2), true);
  assert.equal(coreHash(copied), coreHash(g2));
  assert.equal(rtHash(copied), rtHash(g2));
});
