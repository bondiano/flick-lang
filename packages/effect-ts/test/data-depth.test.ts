import assert from "node:assert/strict";
import { test } from "node:test";
import { Effect, Option, Result, Scope } from "effect";
import { Owner as NativeOwner } from "@flick-lang/runtime/rt/owner";
import { Owner as HostOwner } from "../src/owner.ts";
import { copyData } from "../src/data.ts";
import { dataFromEffect, dataToEffect } from "../src/bridge_data.ts";

const depth = 20_000;
const field = (value: unknown, key: string): unknown => {
  assert.ok(typeof value === "object" && value !== null);
  return Reflect.get(value, key);
};

test("Effect defensive copy handles 20,000 mixed native Option/Result/record/array nodes", () => {
  const leaf = [1];
  let value: unknown = leaf;
  for (let i = 0; i < depth; i++) {
    switch (i % 4) {
      case 0: value = Option.some(value); break;
      case 1: value = Result.succeed(value); break;
      case 2: value = { child: value }; break;
      default: value = [value];
    }
  }
  let copy: unknown = copyData(value);
  for (let i = depth - 1; i >= 0; i--) copy = field(copy, ["value", "success", "child", "0"][i % 4] ?? "");
  assert.ok(Array.isArray(copy));
  copy.push(2);
  assert.deepEqual(leaf, [1]);
  leaf.push(3);
  assert.deepEqual(copy, [1, 2]);
});

test("generic data conversion handles 20,000 nested containers and variants in both directions", () => {
  let value: unknown = 17;
  for (let i = 0; i < depth; i++) value = i % 3 === 0 ? { _tag: "Some", value } : i % 3 === 1 ? { child: value } : [value];
  const host = dataToEffect(value);
  let cursor: unknown = host;
  for (let i = depth - 1; i >= 0; i--) {
    if (i % 3 === 0) assert.ok(Option.isOption(cursor));
    cursor = field(cursor, ["value", "child", "0"][i % 3] ?? "");
  }
  assert.equal(cursor, 17);
  cursor = dataFromEffect(host);
  for (let i = depth - 1; i >= 0; i--) {
    if (i % 3 === 0) assert.equal(Object.getPrototypeOf(cursor), Object.prototype);
    cursor = field(cursor, ["value", "child", "0"][i % 3] ?? "");
  }
  assert.equal(cursor, 17);
});

test("generic conversion keeps unchanged identity, completed aliases and active-cycle behavior", () => {
  const unchanged = { list: [1] };
  assert.equal(dataToEffect(unchanged), unchanged);
  assert.equal(dataFromEffect(unchanged), unchanged);
  const shared = { _tag: "Some", value: 1 };
  const host = dataToEffect([shared, shared]);
  assert.ok(Array.isArray(host));
  assert.equal(host[0], host[1]);
  assert.ok(Option.isOption(host[0]));
  const cycle: unknown[] = [];
  cycle.push(cycle);
  assert.equal(dataToEffect(cycle), cycle);
  assert.equal(dataFromEffect(cycle), cycle);
});

test("generic conversion leaves functions cold, opaque classes and native owned values untouched", () => {
  let calls = 0;
  const fn = () => { calls++; return Effect.succeed(1); };
  class Opaque { readonly content = Option.some(1); }
  const opaque = new Opaque();
  const effect = Effect.succeed(1);
  for (const convert of [dataToEffect, dataFromEffect]) {
    assert.equal(convert(fn), fn);
    assert.equal(convert(opaque), opaque);
    assert.equal(convert(effect), effect);
  }
  assert.equal(calls, 0);
});

test("native wrapper-only chains finish iteratively for Some, Success and Failure", () => {
  for (const wrap of [Option.some, Result.succeed, Result.fail]) {
    let source: unknown = 5;
    for (let i = 0; i < depth; i++) source = wrap(source);
    let copied: unknown = copyData(source);
    let converted: unknown = dataFromEffect(source);
    for (let i = 0; i < depth; i++) {
      const key = Option.isOption(copied) ? "value" : Result.isResult(copied) && Result.isSuccess(copied) ? "success" : "failure";
      copied = field(copied, key);
      converted = field(converted, key);
    }
    assert.equal(copied, 5);
    assert.equal(converted, 5);
  }
});

test("Effect copies retain independent aliases, exact record/getter order and cycle failure", () => {
  const events: string[] = [];
  const leaf = [1];
  const wrapped = Option.some({ get items() { events.push("items"); return leaf; } });
  const input = {
    get left() { events.push("left"); return wrapped; },
    get right() { events.push("right"); return wrapped; },
  };
  const copied = copyData(input);
  assert.deepEqual(events, ["left", "right", "items", "items"]);
  assert.ok(Option.isSome(copied.left) && Option.isSome(copied.right));
  copied.left.value.items.push(2);
  assert.deepEqual(copied.right.value.items, [1]);
  assert.deepEqual(leaf, [1]);
  const cycle: unknown[] = [];
  cycle.push(Option.some(cycle));
  assert.throws(() => copyData(cycle), { name: "RangeError", message: "Maximum call stack size exceeded" });
  const sentinel = new Error("getter");
  assert.throws(() => copyData(Option.some({ get item(): unknown { throw sentinel; } })), (e) => e === sentinel);
});

test("Effect copies normalize arrays and preserve own __proto__ data without invoking foreign hooks", () => {
  let calls = 0;
  class HostArray extends Array<unknown> {
    static override get [Symbol.species](): ArrayConstructor { calls++; return Array; }
  }
  const items = new HostArray();
  items.push(Option.some([1]));
  Object.defineProperty(items, "map", { value: () => { calls++; return items; } });
  const record: Record<string, unknown> = { items };
  Object.defineProperty(record, "__proto__", { value: Option.some([2]), enumerable: true });
  const copied = copyData(record);
  assert.equal(calls, 0);
  assert.equal(Object.getPrototypeOf(copied), Object.prototype);
  assert.equal(Object.hasOwn(copied, "__proto__"), true);
  const array = copied["items"];
  assert.ok(Array.isArray(array));
  assert.equal(Object.getPrototypeOf(array), Array.prototype);
  const first: unknown = array[0];
  assert.ok(Option.isOption(first) && Option.isSome(first));
  assert.ok(Array.isArray(first.value));
  first.value.push(3);
  const original: unknown = items[0];
  assert.ok(Option.isOption(original) && Option.isSome(original));
  assert.deepEqual(original.value, [1]);
});

test("generic conversions retain eager field reads, lazy sparse indices, comparison reads and errors", () => {
  for (const towardHost of [true, false]) {
    const events: string[] = [];
    const variant = towardHost ? { _tag: "Some", value: 1 } : Option.some(1);
    const array: unknown[] = [];
    array.length = 3;
    Object.defineProperty(array, "1", { enumerable: true, get: () => { events.push("index"); return variant; } });
    const source = {
      get first() { events.push("first"); return variant; },
      get second() { events.push("second"); return array; },
    };
    const convert = towardHost ? dataToEffect : dataFromEffect;
    const result = convert(source);
    // Identity comparison stops at the first changed field/index.
    assert.deepEqual(events, ["first", "second", "index", "index", "first"]);
    const copiedArray = field(result, "second");
    assert.ok(Array.isArray(copiedArray));
    assert.equal(copiedArray.length, 3);
    assert.equal(0 in copiedArray, false);
    assert.equal(2 in copiedArray, false);
    const error = new Error("sentinel");
    assert.throws(() => convert({ get field(): unknown { throw error; } }), (e) => e === error);
  }
});

test("real owned markers remain inert identities through copies and generic conversions", () => {
  const native = new NativeOwner();
  const host = new HostOwner(Scope.makeUnsafe());
  native.open = false;
  host.open = false;
  for (const convert of [copyData, dataToEffect, dataFromEffect]) {
    assert.equal(field(convert({ native, host }), "native"), native);
    assert.equal(field(convert({ native, host }), "host"), host);
  }
  assert.equal(native.open, false);
  assert.equal(native.finalizers.length, 0);
  assert.equal(native.closing.size, 0);
  assert.equal(host.open, false);
  assert.equal(host.resources.state._tag, "Empty");
  assert.equal(host.closing.size, 0);
});

test("generic conversion preserves own __proto__ and active cycles even when siblings change", () => {
  for (const towardHost of [true, false]) {
    const variant = towardHost ? { _tag: "Some", value: 7 } : Option.some(7);
    const source: Record<string, unknown> = { variant };
    source["cycle"] = source;
    Object.defineProperty(source, "__proto__", { value: variant, enumerable: true });
    const copy = (towardHost ? dataToEffect : dataFromEffect)(source);
    assert.notEqual(copy, source);
    assert.equal(Object.getPrototypeOf(copy), Object.prototype);
    assert.ok(typeof copy === "object" && copy !== null);
    assert.equal(Object.hasOwn(copy, "__proto__"), true);
    assert.equal(field(copy, "cycle"), source);
    assert.equal(field(copy, "__proto__"), field(copy, "variant"));
    if (towardHost) assert.ok(Option.isOption(field(copy, "variant")));
    else assert.deepEqual(field(copy, "variant"), { _tag: "Some", value: 7 });
  }
});
