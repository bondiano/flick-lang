#!/usr/bin/env node
// Refuse immediately-invoked throwaway closures `(|| …)()` / `(move || …)()`.
// Clippy's `redundant_closure_call` only catches the trivial form; try/finally
// IIFEs that exist only so `?` and `return` stay local are the usual offenders.
// Prefer a helper, `and_then`, or "compute Result → cleanup → `?`" instead.
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const skipDirs = new Set(["target", "node_modules", ".git", "dist", "vendor"]);

export function rustFiles(dir = root, out = []) {
  for (const name of readdirSync(dir)) {
    if (skipDirs.has(name)) continue;
    const full = path.join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) rustFiles(full, out);
    else if (name.endsWith(".rs")) out.push(full);
  }
  return out;
}

/** Strip line and block comments so string/`//` noise does not fake a hit. */
export function stripComments(source) {
  let out = "";
  let i = 0;
  while (i < source.length) {
    if (source.startsWith("//", i)) {
      const end = source.indexOf("\n", i);
      const stop = end < 0 ? source.length : end;
      out += " ".repeat(stop - i);
      i = stop;
      continue;
    }
    if (source.startsWith("/*", i)) {
      const end = source.indexOf("*/", i + 2);
      const stop = end < 0 ? source.length : end + 2;
      for (let j = i; j < stop; j++) out += source[j] === "\n" ? "\n" : " ";
      i = stop;
      continue;
    }
    const c = source[i];
    if (c === '"' || c === "'") {
      out += c;
      i++;
      while (i < source.length) {
        if (source[i] === "\\" && i + 1 < source.length) {
          out += "  ";
          i += 2;
          continue;
        }
        if (source[i] === c) {
          out += c;
          i++;
          break;
        }
        out += source[i] === "\n" ? "\n" : " ";
        i++;
      }
      continue;
    }
    if (c === "r" && (source[i + 1] === '"' || source[i + 1] === "#")) {
      // raw string: r"…" / r#"…"# / r##"…"##
      let hashes = 0;
      let j = i + 1;
      while (source[j] === "#") {
        hashes++;
        j++;
      }
      if (source[j] !== '"') {
        out += c;
        i++;
        continue;
      }
      const closer = `"${"#".repeat(hashes)}`;
      const start = i;
      i = j + 1;
      const end = source.indexOf(closer, i);
      const stop = end < 0 ? source.length : end + closer.length;
      for (let k = start; k < stop; k++) out += source[k] === "\n" ? "\n" : " ";
      i = stop;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

function skipSpace(text, i) {
  while (i < text.length && /\s/.test(text[i])) i++;
  return i;
}

function matchCallParen(text, i) {
  i = skipSpace(text, i);
  if (text[i] !== "(") return -1;
  i = skipSpace(text, i + 1);
  if (text[i] !== ")") return -1;
  return i + 1;
}

/** Closing index after a balanced `{…}`, or -1. `open` points at `{`. */
function closeBrace(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Find `(|| …)()` / `(move || …)()` spans.
 * Returns `{ index, end, line }` for each hit in `source` (comments stripped for
 * matching; line numbers come from the stripped text, which keeps newlines).
 */
export function findIife(source) {
  const text = stripComments(source);
  const hits = [];
  const startRe = /\(\s*(?:move\s+)?\|\|/g;
  let m;
  while ((m = startRe.exec(text))) {
    const start = m.index;
    let i = skipSpace(text, m.index + m[0].length);
    if (text[i] === "{") {
      const close = closeBrace(text, i);
      if (close < 0) continue;
      i = skipSpace(text, close + 1);
      if (text[i] !== ")") continue;
      const after = matchCallParen(text, i + 1);
      if (after < 0) continue;
      hits.push({ index: start, end: after, line: text.slice(0, start).split("\n").length });
      startRe.lastIndex = after;
      continue;
    }
    // Expression body: from the opening `(` of `(||`, find the matching `)` then `()`.
    let depth = 0;
    for (let j = start; j < text.length; j++) {
      const c = text[j];
      if (c === "(") depth++;
      else if (c === ")") {
        depth--;
        if (depth === 0) {
          const after = matchCallParen(text, j + 1);
          if (after >= 0) {
            hits.push({ index: start, end: after, line: text.slice(0, start).split("\n").length });
            startRe.lastIndex = after;
          }
          break;
        }
      } else if (c === "{") {
        // Nested block inside an expression-bodied IIFE — keep scanning with paren depth.
        continue;
      }
    }
  }
  return hits;
}

export function lintFiles(files, { repoRoot = root } = {}) {
  const findings = [];
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const hit of findIife(source)) {
      findings.push({
        file: path.relative(repoRoot, file),
        line: hit.line,
        snippet: source.split("\n")[hit.line - 1]?.trim() ?? "",
      });
    }
  }
  return findings;
}

function main(argv = process.argv.slice(2)) {
  const files = argv.length > 0 ? argv.map((f) => path.resolve(f)) : rustFiles();
  const findings = lintFiles(files);
  if (findings.length === 0) {
    console.log("no immediately-invoked closures");
    return 0;
  }
  for (const f of findings) {
    console.error(`${f.file}:${f.line}: immediately-invoked closure (|| …)() — extract a helper, use and_then, or compute then clean up`);
    if (f.snippet) console.error(`  ${f.snippet}`);
  }
  console.error(`\n${findings.length} immediately-invoked closure(s); see tools/style/no-iife.mjs`);
  return 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) process.exit(main());
