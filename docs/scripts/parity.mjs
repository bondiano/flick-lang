// Each page has an English and a Russian version at the same path. The
// translation records `source_hash`, the hash of its original as it was
// translated, so an edit of the original shows which translation lags.
// English is the original everywhere but `spec/`, which is written in
// Russian first.
//
//   node docs/scripts/parity.mjs                  checks every pair
//   node docs/scripts/parity.mjs --stamp FILE...  records the hash in translations

import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

/** The first 16 hex digits of the SHA-256 of `bytes`. */
export const hash = (bytes) => createHash("sha256").update(bytes).digest("hex").slice(0, 16);

const isPage = (name) => name.endsWith(".md") || name.endsWith(".mdx");

/** The pages under `dir`, relative to it, without the Russian tree. */
const pages = (dir, prefix = "") =>
  readdirSync(path.join(dir, prefix), { withFileTypes: true }).flatMap((entry) => {
    const rel = path.posix.join(prefix, entry.name);
    if (entry.isDirectory()) return rel === "ru" ? [] : pages(dir, rel);
    return isPage(entry.name) ? [rel] : [];
  });

/** `[original, translation]` of a page's path in the English tree. */
const pair = (rel) => {
  const ru = path.posix.join("ru", rel);
  return rel.startsWith("spec/") ? [ru, rel] : [rel, ru];
};

const frontmatter = (text) => /^---\n([\s\S]*?)\n---(?:\n|$)/.exec(text);

/** The `source_hash` a page records, if any. */
const recorded = (text) => /^source_hash:\s*(\S+)\s*$/m.exec(frontmatter(text)?.[1] ?? "")?.[1];

const relative = (file) => path.relative(process.cwd(), file).split(path.sep).join("/");

const command = (root, file) => `node ${relative(import.meta.filename)} --stamp ${relative(path.join(root, file))}`;

/** What is out of order in the pages under `root`, one line each. */
export function problems(root) {
  const en = pages(root);
  const ru = existsSync(path.join(root, "ru")) ? pages(path.join(root, "ru")) : [];
  const all = [...new Set([...en, ...ru])].sort();
  return all.flatMap((rel) => {
    if (!en.includes(rel)) return [`ru/${rel}: no English page at ${rel}`];
    if (!ru.includes(rel)) return [`${rel}: no Russian page at ru/${rel}`];
    const [original, translation] = pair(rel);
    const source = readFileSync(path.join(root, original));
    const found = recorded(readFileSync(path.join(root, translation), "utf8"));
    const own = recorded(source.toString("utf8"));
    return [
      ...(own ? [`${original}: an original records no source_hash; remove it`] : []),
      ...(!found
        ? [`${translation}: no source_hash of ${original}; after checking the translation, run ${command(root, translation)}`]
        : found !== hash(source)
          ? [`${translation}: ${original} changed since it was translated; update the translation, then run ${command(root, translation)}`]
          : []),
    ];
  });
}

/** Records in the translation `file` the hash of its original under `root`. */
export function stamp(root, file) {
  const rel = path.relative(root, path.resolve(file)).split(path.sep).join("/");
  const [original, translation] = pair(rel.replace(/^ru\//, ""));
  if (rel !== translation) throw new Error(`${file} is not a translation; its translation is ${translation}`);
  const text = readFileSync(path.join(root, translation), "utf8");
  const head = frontmatter(text);
  if (!head) throw new Error(`${file} has no frontmatter`);
  const line = `source_hash: ${hash(readFileSync(path.join(root, original)))}`;
  const fields = /^source_hash:.*$/m.test(head[1]) ? head[1].replace(/^source_hash:.*$/m, line) : `${head[1]}\n${line}`;
  writeFileSync(path.join(root, translation), `---\n${fields}${text.slice("---\n".length + head[1].length)}`);
}

if (import.meta.main) {
  const root = path.join(import.meta.dirname, "../src/content/docs");
  const [flag, ...files] = process.argv.slice(2);
  if (flag === "--stamp") {
    for (const file of files) stamp(root, file);
  } else {
    const found = problems(root);
    if (found.length > 0) {
      console.error(found.join("\n"));
      process.exitCode = 1;
    }
  }
}
