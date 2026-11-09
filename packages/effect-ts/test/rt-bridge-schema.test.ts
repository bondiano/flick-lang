import assert from "node:assert/strict";
import { test } from "node:test";

import { Arbitrary, Effect, Exit, Schema } from "effect";

import { err, none, ok } from "@flick-lang/runtime/core/data";
import { codecTransformPure, witness, schemaLiterals, schemaInt, schemaAdtOf, schemaCase, schemaError, schemaLazyOf, schemaOption, schemaRecordOf, schemaRefinedOf, schemaString, type Witness } from "@flick-lang/runtime/rt/schema";
import { codecJsonSchema } from "@flick-lang/runtime/rt/schema_export";

import { codecToEffect, schemaFromEffect, schemaToEffect } from "../src/bridge.ts";
import { schema as arbitrarySchema } from "../src/effect/Arbitrary.ts";
import { schemaRecord, schemaString as hostString } from "../src/schema/schemas.ts";
import { isPure } from "../src/schema/index.ts";

test("a native schema round trip retains its identity and canonical export", () => {
  assert.equal(schemaFromEffect(schemaToEffect(schemaString)), schemaString);
});

test("a bridged ADT codec decodes wire names and encodes qualified runtime tags", () => {
  const native = schemaAdtOf("Item", [
    schemaCase((value: unknown) => ({ _tag: "Item#123", _0: value }), [schemaString]),
  ]);
  const host = codecToEffect(native);
  assert.ok(Schema.isSchema(host));
  assert.ok(isPure(host));
  const decoded = Schema.decodeUnknownExit(host)({ _tag: "Item", _0: "book" });
  assert.ok(Exit.isSuccess(decoded));
  assert.deepEqual(decoded.value, { _tag: "Item#123", _0: "book" });
  const encoded = Schema.encodeUnknownExit(host)({ _tag: "Item#123", _0: "book" });
  assert.ok(Exit.isSuccess(encoded));
  assert.deepEqual(encoded.value, { _tag: "Item", _0: "book" });
});

test("a native record containing an Effect schema exports its structural wire shape", () => {
  const field = schemaFromEffect(schemaRecord([["name", hostString]]));
  assert.ok(typeof field === "object" && field !== null && "decodeAt" in field);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- schemaFromEffect produced the witness checked above
  const record = schemaRecordOf([["person", field as Witness]]);
  const result = codecJsonSchema("Person", record);
  assert.equal(result._tag, "Success");
  if (result._tag !== "Success") return;
  assert.deepEqual(JSON.parse(result.success), {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    type: "object",
    properties: { person: { type: "object", properties: { name: { type: "string" } }, required: ["name"], additionalProperties: false } },
    required: ["person"], additionalProperties: false,
  });
});

test("native ADT schemas with lazy refined fields generate values of their own type in Effect", async () => {
  const slug = schemaRefinedOf("Slug", none, schemaString, (value) =>
    typeof value === "string" && value.length > 0 ? ok(value) : err(schemaError("", "empty slug")));
  const page = schemaAdtOf("Page", [
    schemaCase({ _tag: "Home#1" }, []),
    schemaCase((value: unknown) => ({ _tag: "User#2", _0: value }), [schemaLazyOf("Slug", () => slug)]),
    schemaCase((value: unknown) => ({ _tag: "Search#3", _0: value }), [
      schemaRecordOf([["page", schemaOption(schemaString)]]),
    ]),
  ]);
  const host = schemaToEffect(page);
  assert.ok(Schema.isSchema(host));
  const values = await Effect.runPromise(Arbitrary.sampleEffect(arbitrarySchema(host), { count: 100, seed: 11 }));
  assert.equal(values.length, 100);
  for (const value of values) {
    assert.equal(page.value(value, "")._tag, "Success", JSON.stringify(value));
    assert.ok(typeof value === "object" && value !== null);
    assert.equal(Object.getPrototypeOf(value), Object.prototype);
    if (Reflect.get(value, "_tag") === "Search#3") {
      const record: unknown = Reflect.get(value, "_0");
      assert.ok(typeof record === "object" && record !== null);
      assert.equal(Object.getPrototypeOf(record), Object.prototype);
      const option: unknown = Reflect.get(record, "page");
      assert.ok(typeof option === "object" && option !== null);
      assert.ok(Reflect.get(option, "_tag") === "None" || Reflect.get(option, "_tag") === "Some");
    }
  }
});

test("lazy native enums reject unknown route words and coerce numeric enum segments", () => {
  const method = schemaLazyOf("Method", () => schemaLiterals(schemaString, undefined, ["GET", "POST"]));
  const status = schemaLazyOf("Status", () => schemaLiterals(schemaInt, undefined, [200, 404]));
  const methodHost = schemaToEffect(method);
  const statusHost = schemaToEffect(status);
  assert.ok(Schema.isSchema(methodHost));
  assert.ok(Schema.isSchema(statusHost));
  assert.ok(isPure(methodHost));
  assert.ok(isPure(statusHost));
  assert.equal(Schema.decodeUnknownExit(methodHost)("post")._tag, "Failure");
  assert.equal(Schema.decodeUnknownExit(methodHost)("POST")._tag, "Success");
  assert.equal(Schema.decodeUnknownExit(statusHost)("200")._tag, "Failure");
  assert.equal(Schema.decodeUnknownExit(statusHost)(200)._tag, "Success");
});

test("an undescribed native schema validates runtime values and retains its separate JSON codec", () => {
  const native = witness("numeric-string",
    (input, path) => typeof input === "string" && /^\d+$/.test(input) ? ok(Number(input)) : err(schemaError(path, "digits required")),
    (input, path) => typeof input === "number" ? ok(String(input)) : err(schemaError(path, "number required")),
    (input, path) => typeof input === "number" ? ok(input) : err(schemaError(path, "number required")));
  const host = schemaToEffect(native);
  const codec = codecToEffect(native);
  assert.ok(Schema.isSchema(host));
  assert.ok(Schema.isSchema(codec));
  assert.ok(isPure(host));
  assert.ok(isPure(codec));
  assert.equal(Schema.decodeUnknownExit(host)(4)._tag, "Success");
  assert.equal(Schema.decodeUnknownExit(host)("4")._tag, "Failure");
  const decoded = Schema.decodeUnknownExit(codec)("4");
  assert.ok(Exit.isSuccess(decoded));
  assert.equal(decoded.value, 4);
  const encoded = Schema.encodeUnknownExit(codec)(4);
  assert.ok(Exit.isSuccess(encoded));
  assert.equal(encoded.value, "4");
});

test("an untyped native transform remains opaque when HTTP error codecs are discriminated", () => {
  const native = codecTransformPure((text) => ({ _tag: "Conflict#1", _0: text }), (value) =>
    typeof value === "object" && value !== null ? Reflect.get(value, "_0") : undefined, schemaString);
  const host = codecToEffect(native);
  assert.ok(Schema.isSchema(host));
  assert.equal(Schema.is(host)({ _tag: "flick/opaque-probe" }), true);
});
