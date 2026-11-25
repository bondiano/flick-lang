import assert from "node:assert/strict";
import { test } from "node:test";
import { err, none, ok, some } from "../src/core/data.ts";
import {
  schemaAdtOf, schemaCase, schemaError, schemaLazyOf, schemaList,
  schemaInt, schemaNumber, schemaOption, schemaRecordOf, schemaRefinedOf, schemaResult,
  schemaString, schemaTupleOf, schemaValidate,
} from "../src/rt/schema.ts";

test("record schemas decode and encode a `__proto__` field as an own data field", () => {
  const record = schemaRecordOf([["name", schemaString], ["__proto__", schemaRecordOf([["x", schemaInt]])]]);
  const input: unknown = JSON.parse('{"name":"a","__proto__":{"x":1}}');
  for (const result of [record.decode(input), record.encode(input)]) {
    assert.equal(result._tag, "Success");
    const out = result._tag === "Success" ? result.success : undefined;
    assert.ok(typeof out === "object" && out !== null);
    assert.equal(Object.getPrototypeOf(out), Object.prototype);
    assert.deepEqual(Object.keys(out), ["__proto__", "name"]);
    assert.deepEqual(Object.getOwnPropertyDescriptor(out, "__proto__"),
      { value: { x: 1 }, writable: true, enumerable: true, configurable: true });
    assert.deepEqual(Object.getOwnPropertyDescriptor(out, "name"),
      { value: "a", writable: true, enumerable: true, configurable: true });
  }
});

test("composite runtime witnesses accept non-finite Floats while their JSON encoders reject them", () => {
  const number = schemaLazyOf("Number", () => schemaNumber);
  const record = schemaRecordOf([["numbers", schemaList(schemaTupleOf([number, schemaOption(number)]))]]);
  const result = schemaResult(schemaString, record);
  const adt = schemaAdtOf("Value", [schemaCase((value: unknown) => ({ _tag: "Value#1", _0: value }), [result])]);
  const input = { _tag: "Value#1", _0: ok({ numbers: [[Infinity, some(Number.NaN)]] }) };
  const checked = schemaValidate(adt, input);
  assert.equal(checked._tag, "Success");
  if (checked._tag === "Success") assert.equal(checked.success, input);
  assert.equal(adt.encode(input)._tag, "Failure");
});

test("composite value checks preserve refinement failures and nested diagnostic paths", () => {
  const finite = schemaRefinedOf("Finite", none, schemaNumber, (value) =>
    typeof value === "number" && Number.isFinite(value) ? ok(value) : err(schemaError("", "not finite")));
  const record = schemaRecordOf([["numbers", schemaList(finite)]]);
  assert.equal(schemaValidate(record, { numbers: [Infinity] })._tag, "Failure");
  const wrong = schemaValidate(schemaRecordOf([["numbers", schemaList(schemaNumber)]]), { numbers: [1, "bad"] });
  assert.equal(wrong._tag, "Failure");
  if (wrong._tag === "Failure") assert.equal(wrong.failure._0[0]?.path, "numbers[1]");
});
