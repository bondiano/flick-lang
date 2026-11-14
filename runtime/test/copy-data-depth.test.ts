import assert from "node:assert/strict";
import { test } from "node:test";
import { copyData, copyThrough, none } from "../src/core/data.ts";

const depth = 20_000;
const fn = () => 1;

test("defensive copying traverses 20,000 arrays and records without sharing mutable leaves", () => {
  for (const arrays of [true, false]) {
    const leaf = [1];
    let value: unknown = leaf;
    for (let i = 0; i < depth; i++) value = arrays ? [value] : { child: value };
    let copy: unknown = copyData(value);
    assert.notEqual(copy, value);
    for (let i = 0; i < depth; i++) {
      assert.ok(typeof copy === "object" && copy !== null);
      copy = Reflect.get(copy, arrays ? "0" : "child");
    }
    assert.ok(Array.isArray(copy));
    copy.push(2);
    assert.deepEqual(leaf, [1]);
    leaf.push(3);
    assert.deepEqual(copy, [1, 2]);
  }
});

test("copy traversal preserves eager record reads, lazy sparse array reads and other order", () => {
  const events: string[] = [];
  class Opaque { readonly name: string; constructor(name: string) { this.name = name; } }
  const a = new Opaque("a");
  const b = new Opaque("b");
  const sparse: unknown[] = [];
  sparse.length = 3;
  Object.defineProperty(sparse, "1", { enumerable: true, get: () => { events.push("array"); return b; } });
  const value = {
    get first() { events.push("first"); return a; },
    get second() { events.push("second"); return sparse; },
  };
  const copy = copyThrough(value, (x) => { assert.ok(x instanceof Opaque); events.push(x.name); return x; });
  assert.deepEqual(events, ["first", "second", "a", "array", "b"]);
  assert.ok(typeof copy === "object" && copy !== null);
  const array: unknown = Reflect.get(copy, "second");
  assert.ok(Array.isArray(array));
  assert.equal(array.length, 3);
  assert.equal(0 in array, false);
  assert.equal(2 in array, false);
});

test("copying preserves opaque/frozen identity, null-record normalization and independent repeated copies", () => {
  const shared = { items: [1] };
  const opaque = new Date(0);
  const nullRecord: unknown = Object.create(null);
  assert.ok(typeof nullRecord === "object" && nullRecord !== null);
  Reflect.set(nullRecord, "item", shared);
  const source = { left: shared, right: shared, opaque, fn, none, nullRecord };
  const copy = copyData(source);
  assert.notEqual(copy.left, copy.right);
  copy.left.items.push(2);
  assert.deepEqual(copy.right.items, [1]);
  assert.deepEqual(shared.items, [1]);
  assert.equal(copy.opaque, opaque);
  assert.equal(copy.fn, fn);
  assert.equal(copy.none, none);
  assert.equal(Object.getPrototypeOf(copy.nullRecord), Object.prototype);
  const frozenArray = Object.freeze([shared]);
  assert.notEqual(copyData(frozenArray), frozenArray);
  assert.equal(copyData(Object.freeze(shared)), shared);
});

test("copying propagates exact getter/other errors and rejects cycles as RangeError", () => {
  const error = new Error("sentinel");
  assert.throws(() => copyData({ get item(): unknown { throw error; } }), (e) => e === error);
  assert.throws(() => copyThrough(new Date(), () => { throw error; }), (e) => e === error);
  const cycle: unknown[] = [];
  cycle.push(cycle);
  assert.throws(() => copyData(cycle), { name: "RangeError", message: "Maximum call stack size exceeded" });
});

test("record __proto__ remains an own field without mutating the copy prototype", () => {
  const source: Record<string, unknown> = { normal: 2 };
  Object.defineProperty(source, "__proto__", { value: { items: [1] }, enumerable: true });
  const copy = copyData(source);
  assert.equal(Object.getPrototypeOf(copy), Object.prototype);
  assert.equal(Object.hasOwn(copy, "__proto__"), true);
  const field: unknown = copy["__proto__"];
  assert.ok(typeof field === "object" && field !== null);
  const items: unknown = Reflect.get(field, "items");
  assert.ok(Array.isArray(items));
  items.push(2);
  assert.deepEqual(source["__proto__"], { items: [1] });
});

test("defensive arrays ignore foreign map and species hooks and always allocate plain isolated arrays", () => {
  let calls = 0;
  const source = [[1]];
  Object.defineProperty(source, "map", { value: () => { calls++; return source; } });
  const copy = copyData(source);
  assert.equal(calls, 0);
  assert.equal(Object.getPrototypeOf(copy), Array.prototype);
  assert.notEqual(copy, source);
  copy[0]?.push(2);
  assert.deepEqual(source[0], [1]);
  class HostArray extends Array<unknown> {
    static override get [Symbol.species](): ArrayConstructor { calls++; return Array; }
  }
  const subclass = new HostArray();
  subclass.push([1]);
  const plain = copyData(subclass);
  assert.equal(calls, 0);
  assert.equal(Object.getPrototypeOf(plain), Array.prototype);
  assert.notEqual(plain, subclass);
});

test("array traversal includes inherited indices and observes later-index mutations after children", () => {
  class Opaque { readonly marker = true; }
  const source: unknown[] = [];
  source.length = 3;
  const prototype: unknown = Object.create(Array.prototype);
  assert.ok(typeof prototype === "object" && prototype !== null);
  Object.defineProperty(prototype, "0", { value: new Opaque(), enumerable: true });
  Object.setPrototypeOf(source, prototype);
  source[1] = "before";
  const copy = copyThrough(source, () => { source[1] = "after"; return 7; });
  assert.ok(Array.isArray(copy));
  assert.equal(copy[0], 7);
  assert.equal(copy[1], "after");
  assert.equal(copy.length, 3);
  assert.equal(2 in copy, false);
  assert.equal(Object.getPrototypeOf(copy), Array.prototype);
});

test("opaque callback results are passed through, not re-traversed", () => {
  const returned = { items: [1] };
  const copy = copyThrough([new Date()], () => returned);
  assert.ok(Array.isArray(copy));
  assert.equal(copy[0], returned);
});
