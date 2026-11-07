import assert from "node:assert/strict";
import { test } from "node:test";

import { deadLinks } from "../scripts/links.mjs";

const base = "/site";

const check = (pages: Record<string, string>, files: ReadonlyArray<string> = []): Array<string> =>
  deadLinks(new Map(Object.entries(pages)), new Set(files), base);

test("links that reach a page, a file or an element are alive, and others are not followed", () => {
  assert.deepEqual(
    check(
      {
        "/site/": `<a href="/site/a/">a</a> <a href="a/#one">relative</a> <a href="/site/install.sh">file</a> <a href="https://example.com/x">out</a> <a href="#top">here</a><h1 id="top"></h1>`,
        "/site/a/": `<h2 id="one">one</h2> <a href="../">up</a> <a href="/site/a">no slash</a>`,
      },
      ["/site/install.sh"],
    ),
    [],
  );
});

test("a fragment is compared with the element's id as it is written, in any alphabet", () => {
  assert.deepEqual(
    check({
      "/site/": `<a href="a/#%D1%83%D1%81%D1%82%D0%B0%D0%BD%D0%BE%D0%B2%D0%BA%D0%B0">link</a>`,
      "/site/a/": `<h2 id="установка">Установка</h2>`,
    }),
    [],
  );
});

test("a missing page, a missing element and a link past the base are reported", () => {
  assert.deepEqual(
    check({
      "/site/": `<a href="gone/">page</a> <a href="a/#gone">element</a> <a href="/a/">base</a>`,
      "/site/a/": `<h2 id="one">one</h2>`,
    }),
    [
      `/site/: gone/: no such page`,
      `/site/: a/#gone: no element with id "gone"`,
      `/site/: /a/: outside the site's base /site`,
    ],
  );
});

test("the target of a redirect page is a link too", () => {
  assert.deepEqual(
    check({
      "/site/": `<meta http-equiv="refresh" content="0;url=/a/">`,
      "/site/old/": `<meta http-equiv="refresh" content="0;url=/site/a/">`,
      "/site/a/": ``,
    }),
    [`/site/: /a/: outside the site's base /site`],
  );
});
