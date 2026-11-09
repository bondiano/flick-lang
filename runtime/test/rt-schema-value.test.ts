import assert from "node:assert/strict";
import { test } from "node:test";
import { err, none, ok, some } from "../src/core/data.ts";
import {
  schemaAdtOf, schemaCase, schemaError, schemaLazyOf, schemaList,
  schemaNumber, schemaOption, schemaRecordOf, schemaRefinedOf, schemaResult,
  schemaString, schemaTupleOf, schemaValidate,
} from "../src/rt/schema.ts";

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
