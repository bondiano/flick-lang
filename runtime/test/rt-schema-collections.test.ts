import assert from "node:assert/strict";
import { test } from "node:test";

import { hashMapToList, hashSetToList, type FlickHashMap, type FlickHashSet } from "../src/rt/collections.ts";
import {
  flickEntries,
  flickHashMapJson,
  flickHashSetJson,
  flickIsMinProperties,
  flickIsPropertyNames,
} from "../src/rt/effect/Schema.ts";
import { schemaInt, schemaString } from "../src/rt/schema.ts";

const strings = {
  hash: (value: unknown) => typeof value === "string" ? value.charCodeAt(0) : 0,
  $Eq: { eq: (a: unknown) => (b: unknown) => a === b },
};
const isMap = (value: unknown): value is FlickHashMap<unknown, unknown> =>
  typeof value === "object" && value !== null && "entries" in value && Array.isArray(value.entries);
const isSet = (value: unknown): value is FlickHashSet<unknown> =>
  typeof value === "object" && value !== null && "map" in value && isMap(value.map);

test("collection JSON codecs round trip native collections", () => {
  const set = flickHashSetJson(strings, schemaString);
  const decodedSet = set.decode(["b", "a", "a"]);
  assert.equal(decodedSet._tag, "Success");
  if (decodedSet._tag !== "Success") return;
  assert.ok(isSet(decodedSet.success));
  assert.deepEqual(hashSetToList(decodedSet.success), ["a", "b"]);
  assert.deepEqual(set.encode(decodedSet.success), { _tag: "Success", success: ["a", "b"] });

  const map = flickHashMapJson(strings, schemaString, schemaInt);
  const decodedMap = map.decode([["b", 2], ["a", 1]]);
  assert.equal(decodedMap._tag, "Success");
  if (decodedMap._tag !== "Success") return;
  assert.ok(isMap(decodedMap.success));
  assert.deepEqual(hashMapToList(decodedMap.success), [["a", 1], ["b", 2]]);
  assert.deepEqual(map.encode(decodedMap.success), { _tag: "Success", success: [["a", 1], ["b", 2]] });
});

test("entries checks retain the object shape and key checks", () => {
  const entries = flickIsPropertyNames(schemaString, flickIsMinProperties(2, flickEntries(schemaInt)));
  assert.equal(entries.decode({ a: 1 })._tag, "Failure");
  assert.deepEqual(entries.decode({ a: 1, b: 2 }), { _tag: "Success", success: [["a", 1], ["b", 2]] });
  assert.deepEqual(entries.encode([["a", 1], ["b", 2]]), { _tag: "Success", success: { a: 1, b: 2 } });
});
