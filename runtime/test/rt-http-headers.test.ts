// `Http.Headers` on Flick's own runtime, against what Effect's `Headers` does
// with the same calls.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  Equivalence,
  flickRedact,
  flickShow,
  flickToList,
  flickWithRedactedNames,
  fromInput,
  merge,
  redactedNames,
  set,
} from "../src/rt/effect/http/Headers.ts";
import { root } from "./rt-support.ts";

test("names are lowercase, a later value replaces an earlier one in its place, and merge lets the other win", () => {
  const h = fromInput([["X-A", "1"], ["Content-Type", "text/plain"], ["x-a", "2"]]);
  assert.deepEqual(flickToList(h), [["x-a", "2"], ["content-type", "text/plain"]]);
  assert.deepEqual(flickToList(set("X-A", "3")(h)), [["x-a", "3"], ["content-type", "text/plain"]]);
  assert.deepEqual(flickToList(merge(fromInput([["x-a", "9"], ["x-b", "b"]]))(h)), [["x-a", "9"], ["content-type", "text/plain"], ["x-b", "b"]]);
  assert.ok(Equivalence(h, fromInput([["content-type", "text/plain"], ["x-a", "2"]])));
  assert.ok(!Equivalence(h, fromInput([["x-a", "2"]])));
});

test("show masks the default secret names; redact masks the given ones", () => {
  const h = fromInput([["Authorization", "Bearer s"], ["X-Key", "k"], ["Accept", "*/*"]]);
  assert.equal(flickShow(h), 'Headers [("authorization", "<redacted>"), ("x-key", "k"), ("accept", "*/*")]');
  const redacted = flickRedact(["X-KEY"], h);
  assert.equal(redacted[1]?.[1]._tag.startsWith("Secret#"), true);
  assert.equal(String(redacted[1]?.[1]._0), "<redacted>");
  assert.equal(redacted[0]?.[1]._tag.startsWith("Plain#"), true);
});

test("with_redacted_names gives a task its secret names, and only that task", async () => {
  const inside = await root(() => flickWithRedactedNames(["x-token"], () => redactedNames()));
  assert.deepEqual(inside.value, ["x-token"]);
  assert.deepEqual(redactedNames(), ["authorization", "cookie", "set-cookie", "x-api-key"]);
});
