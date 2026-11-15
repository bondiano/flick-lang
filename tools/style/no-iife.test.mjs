import assert from "node:assert/strict";
import { test } from "node:test";

import { findIife, stripComments } from "./no-iife.mjs";

test("flags a block IIFE used as try/finally", () => {
  const src = `fn f() -> Result<i32, ()> {
    let result = (|| {
        if false { return Err(()); }
        Ok(1)
    })();
    result
}
`;
  assert.deepEqual(
    findIife(src).map((h) => h.line),
    [2],
  );
});

test("flags a one-line expression IIFE", () => {
  assert.deepEqual(
    findIife("let n = (|| 42)();\n").map((h) => h.line),
    [1],
  );
  assert.deepEqual(
    findIife("let n = (move || 42)();\n").map((h) => h.line),
    [1],
  );
});

test("ignores delayed closures and and_then", () => {
  const src = `
    let a = || 1;
    let b = x.and_then(|| Ok(1));
    let c = x.unwrap_or_else(|| 0);
    let d = LazyLock::new(|| expensive());
    x.map(|v| v + 1);
`;
  assert.deepEqual(findIife(src), []);
});

test("ignores IIFEs inside comments and strings", () => {
  const src = `
    // let bad = (|| 1)();
    let s = "(|| 1)()";
    /* (|| { return; })() */
    let ok = 1;
`;
  assert.deepEqual(findIife(src), []);
});

test("stripComments keeps line numbers stable", () => {
  const src = "a\n// (|| 1)()\nb\n";
  const stripped = stripComments(src);
  assert.equal(stripped.split("\n").length, src.split("\n").length);
  assert.match(stripped.split("\n")[1], /^\s*$/);
});
