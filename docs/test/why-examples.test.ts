import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { repo, verifyExample } from "./helpers/verify-example.ts";

const blocks = (locale: string): string[] => {
  const markdown = readFileSync(path.join(repo, "docs/src/content/docs", locale, "start/why.md"), "utf8");
  return [...markdown.matchAll(/```flick\n([\s\S]*?)\n```/g)].map(match => match[1]!);
};

test("Why has identical checked examples in English and Russian", () => {
  assert.deepEqual(blocks("ru"), blocks(""));
});

const cases = [
  {
    signature: "parse_number :",
    body: '  Platform.stdout_line (sum_text "1" "2.5")\n  Platform.stdout_line (sum_text "one" "2")\n  Platform.stdout_line (sum_text "1" "two")',
    output: "3.5\nNot a number: one\nNot a number: two\n",
  },
  {
    signature: "greeting :",
    body: '  Platform.stdout_line (show (check_greeting ()))\n  Platform.stdout_line (provide (Layer.succeed Clock { hour = fun () -> 15 }) { greeting () })',
    output: "true\nGood afternoon\n",
  },
  {
    signature: "digits :",
    body: "  Platform.stdout_line (show (digits 0))\n  Platform.stdout_line (show (digits 7))\n  Platform.stdout_line (show (digits 2026))",
    output: "1\n1\n4\n",
  },
];

for (const [index, example] of cases.entries()) {
  test(`Why ${example.signature} runs as documented in both profiles`, () => {
    const source = blocks("").find(block => block.includes(example.signature));
    assert.ok(source, example.signature);
    const parent = path.join(repo, "target/tmp");
    mkdirSync(parent, { recursive: true });
    const dir = mkdtempSync(path.join(parent, "docs-why-"));
    const name = `${path.basename(dir)}-run-${index}`;
    try {
      const file = path.join(dir, "Main.fl");
      const stdout = path.join(dir, "Main.stdout");
      writeFileSync(file, `module Main exposing (main)\n\n${source}\n\nmain : Unit ->{Host.Console} Unit\nlet main () =\n${example.body}\n`);
      writeFileSync(stdout, example.output);
      verifyExample(name, file, "run", stdout);
    } finally {
      rmSync(dir, { recursive: true, force: true });
      rmSync(path.join(parent, name), { recursive: true, force: true });
    }
  });
}
