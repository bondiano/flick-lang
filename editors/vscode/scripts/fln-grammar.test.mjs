// Validates `editors/vscode/syntaxes/fln.tmLanguage.json`: JSON schema,
// regex compilation, and scopes for comments, strings, suffixes, records, and constructors.
//
//   node --test editors/vscode/scripts/fln-grammar.test.mjs

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function compilePatterns(node, visited = new Set()) {
  if (!node || typeof node !== "object" || visited.has(node)) return;
  visited.add(node);
  for (const field of ["match", "begin", "end"]) {
    if (typeof node[field] === "string") {
      const sanitized = field === "end" ? node[field].replace(/\\\d+/g, "") : node[field];
      new RegExp(sanitized, "g");
    }
  }
  for (const value of Object.values(node)) {
    if (Array.isArray(value)) {
      for (const item of value) compilePatterns(item, visited);
    } else if (value && typeof value === "object") {
      compilePatterns(value, visited);
    }
  }
}

function expandIncludes(patterns, repo, rootPatterns, seen = new Set()) {
  const out = [];
  for (const rule of patterns ?? []) {
    if (rule.include === "$self") {
      if (!seen.has("$self")) {
        seen.add("$self");
        out.push(...expandIncludes(rootPatterns, repo, rootPatterns, seen));
      }
    } else if (typeof rule.include === "string" && rule.include.startsWith("#")) {
      const key = rule.include.slice(1);
      if (!seen.has(key)) {
        seen.add(key);
        const target = repo[key];
        assert.ok(target, `missing repository rule ${rule.include}`);
        if (!target.begin && !target.match && Array.isArray(target.patterns)) {
          out.push(...expandIncludes(target.patterns, repo, rootPatterns, seen));
        } else {
          out.push(target);
        }
      }
    } else {
      out.push(rule);
    }
  }
  return out;
}

function execAt(pattern, line, from) {
  const re = new RegExp(pattern, "g");
  re.lastIndex = from;
  return re.exec(line);
}

function applyCaptures(match, captures, baseScopes, tokens) {
  if (!captures) return;
  const entries = Object.entries(captures)
    .map(([idx, cfg]) => [Number(idx), cfg])
    .filter(([idx]) => Number.isInteger(idx) && idx > 0 && match[idx] !== undefined && match[idx].length > 0);
  if (entries.length === 0) return;

  let cursor = match.index;
  const fullText = match[0];
  const fullEnd = match.index + fullText.length;

  for (const [idx, cfg] of entries) {
    const sub = match[idx];
    const rel = fullText.indexOf(sub, cursor - match.index);
    if (rel < 0) continue;
    const start = match.index + rel;
    if (start > cursor) {
      tokens.push({ start, end: start, scopes: baseScopes, text: "" });
    }
    const end = start + sub.length;
    tokens.push({
      start,
      end,
      scopes: cfg.name ? [...baseScopes, cfg.name] : baseScopes,
      text: sub,
    });
    cursor = end;
  }
  if (cursor < fullEnd) {
    tokens.push({ start: cursor, end: fullEnd, scopes: baseScopes, text: fullText.slice(cursor - match.index) });
  }
}

test("fln.tmLanguage.json compiles and scopes comments, strings, suffixes, records, and constructors", () => {
  const flnPath = path.join(root, "syntaxes", "fln.tmLanguage.json");
  const fln = JSON.parse(readFileSync(flnPath, "utf8"));
  assert.equal(fln.scopeName, "source.fln");
  assert.deepEqual(fln.fileTypes, ["fln"]);
  compilePatterns(fln);

  const rules = expandIncludes(fln.patterns, fln.repository, fln.patterns);
  const tokenizeFln = (line) => {
    const tokens = [];
    let pos = 0;
    while (pos < line.length) {
      let best = null;
      for (const rule of rules) {
        const pattern = rule.match ?? rule.begin;
        if (!pattern) continue;
        const m = execAt(pattern, line, pos);
        if (!m) continue;
        if (best === null || m.index < best.m.index) {
          best = { rule, m };
          if (m.index === pos) break;
        }
      }
      if (!best) break;
      const { rule, m } = best;
      const baseScopes = rule.name ? [rule.name] : [];
      if (rule.begin) {
        const endRe = new RegExp(rule.end.replace(/\\\d+/g, ""), "g");
        endRe.lastIndex = m.index + m[0].length;
        const endM = endRe.exec(line);
        const endPos = endM ? endM.index + endM[0].length : line.length;
        tokens.push({ start: m.index, end: endPos, scopes: baseScopes, text: line.slice(m.index, endPos) });
        pos = endPos;
      } else {
        if (rule.captures) {
          applyCaptures(m, rule.captures, baseScopes, tokens);
        } else {
          tokens.push({ start: m.index, end: m.index + m[0].length, scopes: baseScopes, text: m[0] });
        }
        pos = Math.max(m.index + m[0].length, pos + 1);
      }
    }
    return tokens;
  };
  const flnScopes = (line, word) => {
    const at = line.indexOf(word);
    assert.ok(at >= 0, `${word} in ${line}`);
    const tok = tokenizeFln(line).find((t) => t.start <= at && at < t.end);
    assert.ok(tok, `no token for ${word} in ${line}`);
    return tok.scopes;
  };

  assert.ok(flnScopes("// comment", "comment").includes("comment.line.double-slash.fln"));
  assert.ok(flnScopes("/// doc", "doc").includes("comment.line.documentation.fln"));
  assert.ok(flnScopes('r#"raw"#', "raw").includes("string.quoted.other.raw.fln"));
  assert.ok(flnScopes("App.Config { host = \"localhost\"; port = 8080u16; ratio = 1.5f32; big = 10n }", "App").includes("entity.name.type.fln"));
  assert.ok(flnScopes("App.Config { host = \"localhost\"; port = 8080u16; ratio = 1.5f32; big = 10n }", "Config").includes("entity.name.type.fln"));
  assert.ok(flnScopes("App.Config { host = \"localhost\"; port = 8080u16; ratio = 1.5f32; big = 10n }", "host").includes("variable.other.member.fln"));
  assert.ok(flnScopes("App.Config { host = \"localhost\"; port = 8080u16; ratio = 1.5f32; big = 10n }", "8080u16").includes("constant.numeric.fln"));
  assert.ok(flnScopes("App.Config { host = \"localhost\"; port = 8080u16; ratio = 1.5f32; big = 10n }", "1.5f32").includes("constant.numeric.fln"));
  assert.ok(flnScopes("App.Config { host = \"localhost\"; port = 8080u16; ratio = 1.5f32; big = 10n }", "10n").includes("constant.numeric.fln"));
  assert.ok(flnScopes("{ enabled = true; unit = () }", "true").includes("constant.language.boolean.fln"));
  assert.ok(flnScopes("{ enabled = true; unit = () }", "()").includes("constant.language.unit.fln"));
  assert.ok(!fln.repository.strings.patterns.some((p) => (p.patterns ?? []).some((q) => q.include === "#interpolation")));
});
