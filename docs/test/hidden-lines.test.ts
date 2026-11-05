import assert from "node:assert/strict";
import { test } from "node:test";

import { flickBlocks } from "../flick-blocks.mjs";

const code = (lang: string, meta: string | null, value: string) => ({ type: "code", lang, meta, value });

test("hidden lines of a flick block are compiled, not shown", () => {
  const value = "# type UserId = UserId String\n#\nshow_id : UserId -> String\n# let show_id (UserId id) = id\n#[not hidden]";
  assert.deepEqual(flickBlocks.code(code("flick", null, value)), code("flick", null, "show_id : UserId -> String\n#[not hidden]"));
});

test("the check words leave the meta, other words stay", () => {
  assert.equal(flickBlocks.code(code("flick", "fail", "x"))?.meta, null);
  assert.equal(flickBlocks.code(code("flick", 'ignore title="x.fl"', "x"))?.meta, 'title="x.fl"');
});

test("blocks of other languages are left as written", () => {
  assert.equal(flickBlocks.code(code("toml", "fail", "# a comment\nkey = 1")), undefined);
});
