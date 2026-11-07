// Every link of the built site reaches a page or file under the deployment base, and every
// fragment an element of that page: relative or absolute, from Markdown or from a component.
//
//   node docs/scripts/links.mjs    checks docs/dist; build the site first

import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const origin = "http://site";

const attribute = (text) => text.replaceAll("&amp;", "&");

/** What a page links to: anchors, and the target of a redirect page. */
const targets = (html) => [
  ...[...html.matchAll(/<a\b[^>]*?\bhref="([^"]*)"/g)].map((match) => attribute(match[1])),
  ...[...html.matchAll(/<meta\b[^>]*?\bcontent="\d+;\s*url=([^"]*)"/g)].map((match) => attribute(match[1])),
];

const ids = (html) => new Set([...html.matchAll(/\bid="([^"]*)"/g)].map((match) => attribute(match[1])));

const decoded = (text) => {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
};

/**
 * The dead links among `pages`, a map of a page's URL path (ending in `/`) to its HTML; `files` are
 * the URL paths of everything else. Links that leave the site are not followed.
 */
export const deadLinks = (pages, files, base) => {
  const found = [];
  for (const [page, html] of pages) {
    for (const href of targets(html)) {
      const url = new URL(href, `${origin}${page}`);
      if (url.origin !== origin) continue;
      const pathname = decoded(url.pathname);
      const report = (why) => found.push(`${page}: ${href}: ${why}`);
      if (pathname !== base && !pathname.startsWith(`${base}/`)) {
        report(`outside the site's base ${base}`);
        continue;
      }
      const target = pages.has(pathname) ? pathname : pages.has(`${pathname}/`) ? `${pathname}/` : undefined;
      if (target === undefined) {
        if (!files.has(pathname)) report("no such page");
        continue;
      }
      const fragment = decoded(url.hash.slice(1));
      if (fragment !== "" && !ids(pages.get(target)).has(fragment)) report(`no element with id "${fragment}"`);
    }
  }
  return found;
};

/** The pages and files of a built site, by the URL paths that serve them under `base`. */
export const site = (dist, base) => {
  const pages = new Map();
  const files = new Set();
  const walk = (dir, prefix) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(file, `${prefix}/${entry.name}`);
      else if (entry.name === "index.html") pages.set(`${prefix}/`, readFileSync(file, "utf8"));
      else files.add(`${prefix}/${entry.name}`);
    }
  };
  walk(dist, base);
  return { pages, files };
};

if (import.meta.main) {
  const base = "/flick-lang";
  const { pages, files } = site(path.join(import.meta.dirname, "../dist"), base);
  if (pages.size === 0) throw new Error("docs/dist has no pages: build the site first");
  const found = deadLinks(pages, files, base);
  if (found.length > 0) {
    console.error(found.join("\n"));
    console.error(`${found.length} dead links in ${pages.size} pages`);
    process.exitCode = 1;
  }
}
