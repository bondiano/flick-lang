import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { renderMarkdown } from "../src/playground/markdown.ts";
import { loadHighlighting } from "../src/playground/highlight.ts";

const document = new JSDOM("", { url: "https://example.com/play/" }).window.document;

test("hover Markdown formats prose, inline code, lists, references and highlighted fenced code", async () => {
  await loadHighlighting();
  const dom = renderMarkdown('```flick\nmain : Unit ->{} Unit\n```\n\nfrom `Main`\n\nA **bold** and *clear*\nparagraph.\n\n3. first\n4. second\n\n> note\n\n[Docs][docs]\n\n[docs]: ../reference/ "Reference"\n\n```javascript\nconst value = "hello";\n```\n\n```text\nkeep <text>\n```', document);
  assert.equal(dom.querySelector("pre code")?.textContent, "main : Unit ->{} Unit");
  assert.ok([...dom.querySelectorAll("pre .fp-token-keyword")].some(token => token.textContent === "Unit"));
  assert.equal(dom.querySelector("p code")?.textContent, "Main");
  assert.equal(dom.querySelector("strong")?.textContent, "bold");
  assert.equal(dom.querySelector("em")?.textContent, "clear");
  assert.equal(dom.querySelector("ol")?.getAttribute("start"), "3");
  assert.equal(dom.querySelectorAll("li").length, 2);
  assert.equal(dom.querySelector("blockquote")?.textContent, "note");
  assert.equal(dom.querySelector("a")?.getAttribute("href"), "../reference/");
  assert.equal(dom.querySelector("a")?.getAttribute("title"), "Reference");
  assert.equal(dom.querySelectorAll("pre code")[1]?.textContent, 'const value = "hello";');
  assert.ok([...dom.querySelectorAll("pre .fp-token-string")].some(token => token.textContent?.includes('"hello"')));
  assert.equal(dom.querySelectorAll("pre code")[2]?.textContent, "keep <text>");
  assert.ok(!dom.textContent?.includes("```"));
});

test("hover Markdown keeps HTML inert and refuses executable links and remote images", () => {
  const dom = renderMarkdown('<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[bad](javascript:alert%281%29) [encoded](java&#x73;cript:alert%281%29) [data](data:text/html,bad) [good](https://example.com)\n\n![description](https://example.com/track.png)', document);
  assert.equal(dom.querySelectorAll("script, img").length, 0);
  assert.ok(dom.textContent?.includes("<script>alert(1)</script>"));
  assert.ok(dom.textContent?.includes("description"));
  assert.deepEqual([...dom.querySelectorAll("a")].map(a => a.getAttribute("href")), [null, null, null, "https://example.com"]);
});

test("hover references find nested definitions and keep the first definition", () => {
  const dom = renderMarkdown('[Quote][quote] [List][list] [First][same] [Unsafe][unsafe]\n\n> [quote]: ../quote/ "Quoted"\n\n- [list]: https://example.com/list\n\n[same]: /first/\n[same]: /second/\n\n> [unsafe]: javascript:alert%281%29', document);
  assert.deepEqual([...dom.querySelectorAll("a")].map(a => a.getAttribute("href")), ["../quote/", "https://example.com/list", "/first/", null]);
  assert.equal(dom.querySelector("a")?.getAttribute("title"), "Quoted");
});
