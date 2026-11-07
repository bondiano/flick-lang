import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import type { LanguageRegistration } from "shiki/core";

const grammar = JSON.parse(readFileSync(new URL("../../editors/vscode/syntaxes/flick.tmLanguage.json", import.meta.url), "utf8")) as LanguageRegistration;

test("CLI clauses and parameter names are scoped without leaking through nested braces", async () => {
  const highlighter = await createHighlighterCore({
    engine: createJavaScriptRegexEngine(),
    themes: [],
    langs: [{ ...grammar, name: "flick" }],
  });
  try {
    const language = highlighter.getLanguage("flick");
    let result = language.tokenizeLine('let command = cli "plan" {', null);
    const scopes = (line: string, word: string): readonly string[] => {
      result = language.tokenizeLine(line, result.ruleStack);
      const at = line.indexOf(word);
      assert.ok(at >= 0, word);
      const token = result.tokens.find((token) => token.startIndex <= at && at < token.endIndex);
      assert.ok(token, `${word} in ${line}`);
      return token.scopes;
    };
    assert.ok(scopes('  description "Plan tasks"', "description").includes("keyword.other.flick"));
    assert.ok(scopes('  flag overhead "o" : Option Number', "overhead").includes("variable.parameter.flick"));
    assert.ok(scopes("  arg tasks : List String", "tasks").includes("variable.parameter.flick"));
    assert.ok(scopes("  handle fun { overhead; tasks } ->", "handle").includes("keyword.other.flick"));
    assert.ok(scopes("    let options = { flag = overhead }", "flag").includes("variable.other.flick"));
    assert.ok(scopes("    ignore (options, tasks)", "tasks").includes("variable.other.flick"));
    scopes("}", "}");
    assert.ok(scopes("let value = description", "description").includes("variable.other.flick"));
    assert.ok(scopes('let group = cli "tool" { subcommand command }', "subcommand").includes("keyword.other.flick"));
    assert.ok(scopes("let member = Cli.flag", "flag").includes("variable.other.member.flick"));
  } finally {
    highlighter.dispose();
  }
});

test("plugin heads in imports and server functions are scoped, `cli` elsewhere is a name", async () => {
  const highlighter = await createHighlighterCore({
    engine: createJavaScriptRegexEngine(),
    themes: [],
    langs: [{ ...grammar, name: "flick" }],
  });
  try {
    const language = highlighter.getLanguage("flick");
    const scopes = (line: string, word: string): readonly string[] => {
      const result = language.tokenizeLine(line, null);
      const at = line.indexOf(word);
      assert.ok(at >= 0, word);
      const token = result.tokens.find((token) => token.startIndex <= at && at < token.endIndex);
      assert.ok(token, `${word} in ${line}`);
      return token.scopes;
    };
    const imported = "import Pairs exposing (Pair, plugin pairs)";
    assert.ok(scopes(imported, "plugin").includes("keyword.other.flick"));
    assert.ok(scopes(imported, "pairs").includes("entity.name.function.macro.flick"));
    assert.ok(scopes("server fn ping n = n", "fn").includes("keyword.other.flick"));
    assert.ok(!scopes("let tool = cli", "cli").includes("keyword.other.flick"));
  } finally {
    highlighter.dispose();
  }
});

test("the two parts of an arrow's row have their own scopes, and names in them keep theirs", async () => {
  const highlighter = await createHighlighterCore({
    engine: createJavaScriptRegexEngine(),
    themes: [],
    langs: [{ ...grammar, name: "flick" }],
  });
  try {
    const language = highlighter.getLanguage("flick");
    const line = "load : UserId ->{Users, Log {a}; NotFound} (Unit ->{; Other} View)";
    const { tokens } = language.tokenizeLine(line, null);
    const scopes = (word: string): readonly string[] => {
      const start = line.indexOf(word);
      return tokens.find((token) => token.startIndex <= start && start < token.endIndex)?.scopes ?? [];
    };
    assert.ok(scopes("Users").includes("meta.effect-row.needs.flick"));
    assert.ok(scopes("Users").includes("entity.name.type.flick"));
    assert.ok(scopes("Log").includes("meta.effect-row.needs.flick"));
    assert.ok(scopes("NotFound").includes("meta.effect-row.fails.flick"));
    assert.ok(!scopes("NotFound").includes("meta.effect-row.needs.flick") || scopes("NotFound").indexOf("meta.effect-row.fails.flick") > scopes("NotFound").indexOf("meta.effect-row.needs.flick"));
    assert.ok(scopes("Other").includes("meta.effect-row.fails.flick"));
    assert.ok(!scopes("UserId").some((scope) => scope.startsWith("meta.effect-row")));
    assert.ok(!scopes("View").some((scope) => scope.startsWith("meta.effect-row")));
  } finally {
    highlighter.dispose();
  }
});
