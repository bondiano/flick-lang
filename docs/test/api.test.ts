import assert from "node:assert/strict";
import { test } from "node:test";
import { apiPages, contentId } from "../scripts/api.mjs";

test("API uses CLI Markdown verbatim for declarations, comments, examples and source links", () => {
  const body = '\n## Values\n\n### `length`\n\n```flick\nlength : String -> Int\n```\n\nA length.\n\n[Source](https://example.org/src/String.fl#L3)\n\n```flick\nlet n = String.length "abc"\n```\n';
  const input = new Map([["index.md", "# API\n"], ["String.md", `# String\n${body}`]]);
  const pages = apiPages(input);
  assert.equal(pages.size, 4);
  assert.equal(pages.get("api/string.md"), `---\ntitle: "String"\n---\n${body}`);
  assert.equal(pages.get("ru/api/string.md"), pages.get("api/string.md"));
  assert.match(pages.get("api/index.md") ?? "", /\[String\]\(\.\/string\/\)/);
  assert.match(pages.get("ru/api/index.md") ?? "", /API стандартной библиотеки/);
  assert.deepEqual(apiPages(input), pages, "generation is deterministic");
});

test("routes retain existing handwritten slugs and separate generated locales", () => {
  for (const [entry, expected] of [["src/content/docs/handbook/typed-dsl.md", "handbook/typed-dsl"], ["src/content/docs/ru/spec/index.md", "ru/spec"], [".generated/api/index.md", "api"], [".generated/ru/api/http.server.md", "ru/api/http.server"]]) {
    assert.equal(contentId({ entry, data: {} }), expected);
  }
  assert.equal(contentId({ entry: "src/content/docs/a.md", data: { slug: "custom" } }), "custom");
});

test("module links use site routes without rewriting source or external prose links", () => {
  const pages = apiPages(new Map([["A.md", "# A\n\n[B](B.md#value) [source](https://example.org/B.md)"], ["B.md", "# B\n"]]));
  assert.match(pages.get("api/a.md") ?? "", /\[B\]\(\.\.\/b\/#value\)/);
  assert.match(pages.get("api/a.md") ?? "", /\[source\]\(https:\/\/example.org\/B.md\)/);
});
