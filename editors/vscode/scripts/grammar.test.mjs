// Validates `editors/vscode/syntaxes/flick.tmLanguage.json`: JSON schema,
// regex compilation, keyword scopes (including contextual keywords), and word
// boundary isolation against partial identifiers and member accesses.
//
//   node --test editors/vscode/scripts/grammar.test.mjs

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const grammarPath = path.join(root, "syntaxes", "flick.tmLanguage.json");
const grammar = JSON.parse(readFileSync(grammarPath, "utf8"));

function compilePatterns(node, visited = new Set()) {
  if (!node || typeof node !== "object" || visited.has(node)) return;
  visited.add(node);
  for (const field of ["match", "begin", "end"]) {
    if (typeof node[field] === "string") {
      // Replace TextMate backreference `\1` in `end` when validating JS compilation.
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

/** Tokenizes a single line through the grammar's top-level rules. */
function tokenizeLine(line) {
  const rules = expandIncludes(grammar.patterns, grammar.repository, grammar.patterns);
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
    if (!best) {
      tokens.push({ start: pos, end: line.length, scopes: ["source.flick"], text: line.slice(pos) });
      break;
    }
    const { rule, m } = best;
    if (m.index > pos) {
      tokens.push({ start: pos, end: m.index, scopes: ["source.flick"], text: line.slice(pos, m.index) });
    }
    if (rule.match) {
      const base = rule.name ? ["source.flick", rule.name] : ["source.flick"];
      if (rule.captures) {
        applyCaptures(m, rule.captures, base, tokens);
      } else {
        tokens.push({ start: m.index, end: m.index + m[0].length, scopes: base, text: m[0] });
      }
      pos = Math.max(pos + 1, m.index + m[0].length);
    } else {
      // `begin`/`end` block on single line (e.g. strings, effect rows, cli).
      const endPattern = rule.end.replace(/\\\d+/g, "");
      const endMatch = execAt(endPattern, line, m.index + m[0].length);
      const endPos = endMatch ? endMatch.index + endMatch[0].length : line.length;
      const base = rule.name ? ["source.flick", rule.name] : ["source.flick"];
      tokens.push({ start: m.index, end: endPos, scopes: base, text: line.slice(m.index, endPos) });
      pos = Math.max(pos + 1, endPos);
    }
  }
  return tokens;
}

function scopesFor(line, word) {
  const at = line.indexOf(word);
  assert.ok(at >= 0, `word "${word}" not found in "${line}"`);
  const tokens = tokenizeLine(line);
  const hit = tokens.find((t) => t.start <= at && at < t.end);
  assert.ok(hit, `no token covering "${word}" in "${line}"`);
  return hit.scopes;
}

test("flick.tmLanguage.json is valid and every pattern compiles", () => {
  assert.equal(grammar.name, "Flick");
  assert.equal(grammar.scopeName, "source.flick");
  assert.deepEqual(grammar.fileTypes, ["fl"]);
  compilePatterns(grammar);
});

test("contextual effect, sql, and control keywords receive expected TextMate scopes", () => {
  assert.ok(scopesFor("effect Emit a =", "effect").includes("keyword.other.flick"));
  assert.ok(scopesFor("  effect Ask a =", "effect").includes("keyword.other.flick"));
  assert.ok(scopesFor("  tail ping : Unit ->{} Unit", "tail").includes("keyword.other.flick"));
  assert.ok(scopesFor("  abort throw : forall b. String ->{} b", "abort").includes("keyword.other.flick"));
  assert.ok(scopesFor("  control emit : a ->{} Unit", "control").includes("keyword.other.flick"));
  assert.ok(scopesFor("  | return () -> []", "return").includes("keyword.control.flick"));
  assert.ok(scopesFor("  | return x -> x + 1", "return").includes("keyword.control.flick"));
  assert.ok(scopesFor("let q = sql { from u in Users; select u }", "sql").includes("keyword.other.flick"));
  assert.ok(scopesFor("let q = sql params { from u in Users }", "sql").includes("keyword.other.flick"));
  assert.ok(scopesFor('sql table Users "users" {', "sql").includes("keyword.other.flick"));
  assert.ok(scopesFor('sql migration 1 "init" {', "sql").includes("keyword.other.flick"));
  assert.ok(scopesFor("let c = codec Profile.schema {", "codec").includes("keyword.other.flick"));
  assert.ok(scopesFor("type Box = exists A : Type. A", "exists").includes("keyword.other.flick"));
  assert.ok(scopesFor("let y = do { a <- x and b <- y; pure (a, b) }", "and").includes("keyword.other.flick"));
  assert.ok(scopesFor("module M : Sig with type M.T = Int and type M.U = String", "and").includes("keyword.other.flick"));
  assert.ok(scopesFor("type alias UserId = String", "alias").includes("keyword.other.flick"));
  assert.ok(scopesFor("unboxed type Token = Token String", "unboxed").includes("keyword.other.flick"));
  assert.ok(scopesFor("opaque untagged type Method = Get = \"GET\"", "untagged").includes("keyword.other.flick"));
  assert.ok(scopesFor("let mod_val = pack Mod as Sig", "pack").includes("keyword.other.flick"));
  assert.ok(scopesFor("let x = unpack boxed as M, v in v", "unpack").includes("keyword.other.flick"));
  assert.ok(scopesFor("  server fn ping x = x", "server").includes("keyword.other.flick"));
  assert.ok(scopesFor("  server fn ping x = x", "fn").includes("keyword.other.flick"));
  assert.ok(scopesFor("  component counter () =", "component").includes("keyword.other.flick"));
  assert.ok(scopesFor('suite "Users" {', "suite").includes("keyword.other.flick"));
  assert.ok(scopesFor("proof same where same_type a a = refl", "proof").includes("keyword.other.flick"));
  assert.ok(scopesFor("proof same where same_type a a = refl", "same").includes("entity.name.function.flick"));
  assert.ok(scopesFor("  law kept (v : f a) where equal v v", "law").includes("keyword.other.flick"));
  assert.ok(scopesFor("  require (x > 0)", "require").includes("keyword.other.flick"));
  assert.ok(scopesFor("let inc x where ensure (fun y -> y > x) = x + 1", "ensure").includes("keyword.other.flick"));
  assert.ok(scopesFor('  test.skip "trims" {', "test").includes("keyword.other.flick"));
  assert.ok(scopesFor('  test.skip "trims" {', "skip").includes("keyword.other.flick"));
  assert.ok(scopesFor('test.only "alone" {}', "only").includes("keyword.other.flick"));
  assert.ok(scopesFor('  test "slow" timeout 500 {', "timeout").includes("keyword.other.flick"));
  assert.ok(scopesFor('suite.skip "net" timeout 500 {', "skip").includes("keyword.other.flick"));
  assert.ok(scopesFor('  before_each { reset () }', "before_each").includes("keyword.other.flick"));
  assert.ok(scopesFor("  after_all {", "after_all").includes("keyword.other.flick"));
  assert.ok(scopesFor('suite parallel "pool" {', "parallel").includes("keyword.other.flick"));
  assert.ok(scopesFor('suite.skip parallel "pool" timeout 50 {', "timeout").includes("keyword.other.flick"));
  assert.ok(scopesFor("  fixture conn = connect ()", "fixture").includes("keyword.other.flick"));
  assert.ok(scopesFor('  test.each [(1, 2)] "adds {0}" (a, b) {', "each").includes("keyword.other.flick"));
  assert.ok(scopesFor("test.each rows \"is {0}\" (n) {}", "test").includes("keyword.other.flick"));
  assert.ok(scopesFor("  fixture shared concurrent pool = make 4", "shared").includes("keyword.other.flick"));
  assert.ok(scopesFor("  fixture shared concurrent pool = make 4", "concurrent").includes("keyword.other.flick"));
  assert.ok(scopesFor("for x <- xs do { break }", "for").includes("keyword.control.flick"));
  assert.ok(scopesFor("for x <- xs yield x", "yield").includes("keyword.control.flick"));
  assert.ok(scopesFor("'outer: loop { continue 'outer }", "continue").includes("keyword.control.flick"));
  assert.ok(scopesFor("let shared = lazy { load id }", "lazy").includes("keyword.control.flick"));
});

test("word boundaries prevent spurious keyword matches on partial identifiers and member accesses", () => {
  const nonKeywordCases = [
    ["let sql_query = 1", "sql_query"],
    ["let abort_flag = true", "abort_flag"],
    ["let returning_value = 42", "returning_value"],
    ["let tail_call : Int = 1", "tail_call"],
    ["let control_flow : Int = 1", "control_flow"],
    ["let effect_id = 1", "effect_id"],
    ["let my_sql = sql_query { 1 }", "sql_query"],
    ["let x = row.sql", "sql"],
    ["let test = suite", "test"],
    ["let x = timeout 500 task", "timeout"],
    ["let before_each = 1", "before_each"],
    ["  fixture shared = 1", "shared"],
    ["let x = rows.each", "each"],
    ["let fixture = shared parallel", "fixture"],
    ["let x = parallel concurrent", "concurrent"],
    ["let x = Test.test \"a\" check", "test"],
    ["let x = item.effect", "effect"],
    ["let x = state.abort", "abort"],
    ["let x = state.control", "control"],
    ["let x = list.tail", "tail"],
    ["let x = res.return", "return"],
    ["let sql = { a = 1 }", "sql"],
    ["let effect = 1", "effect"],
    ["let tail = 1", "tail"],
    ["let abort = 1", "abort"],
    ["let control = 1", "control"],
    ["let return = 1", "return"],
    ["  | return -> 0", "return"],
    ["  | returning_value -> 0", "returning_value"],
    ["let ok = File.exists path", "exists"],
    ["let exists_flag = false", "exists_flag"],
    ["let x = and_then f y", "and_then"],
    ["let and = 1", "and"],
    ["let codec_json = 1", "codec_json"],
    ["let pack_bytes = 1", "pack_bytes"],
    ["let unpack_value = 1", "unpack_value"],
    ["let alias_name = 1", "alias_name"],
    ["let unboxed_val = 1", "unboxed_val"],
    ["let proof = 1", "proof"],
    ["let p = trans proof h", "proof"],
    ["let law = 1", "law"],
    ["let require = 1", "require"],
    ["let x = ensure y", "ensure"],
  ];

  for (const [line, word] of nonKeywordCases) {
    const scopes = scopesFor(line, word);
    assert.ok(
      !scopes.some((s) => s.startsWith("keyword.")),
      `expected "${word}" in "${line}" NOT to match keyword scope, got ${JSON.stringify(scopes)}`,
    );
  }
});

test("an effect row begins only where `{` touches `->`, as the compiler reads it", () => {
  const begin = new RegExp(grammar.repository["effect-row"].begin);
  assert.ok(begin.test("f : A ->{Log; E} B"));
  assert.ok(begin.test("f : A ->{} B"));
  assert.ok(!begin.test("f : A -> { x : Int }"));
  assert.ok(!begin.test("let g = fun x -> { a = 1 }"));
});
