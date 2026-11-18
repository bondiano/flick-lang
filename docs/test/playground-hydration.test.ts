import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import { runPromise } from "@flick-lang/runtime/rt";
import { hotDispose, run } from "@flick-lang/runtime/rt/runner/browser";
import { uiRenderStatic } from "@flick-lang/runtime/rt/ui/react";
import type { Ui } from "@flick-lang/runtime/ui/react/nodes";
import { JSDOM } from "jsdom";
import { renderToString } from "react-dom/server";

import { parseExerciseMetadata } from "../exercises/load.ts";
import { lessonConfiguration } from "../src/playground/lesson-config.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(repo, "target"), "debug/flick");

const examples = [
  { id: "hello", group: "Basics", group_ru: "Основы", name: "Hello", name_ru: "Привет", source: "module Main exposing (main)\n" },
  { id: "other", group: "Types", group_ru: "Типы", name: "Other", name_ru: "Другое", source: "module Other exposing (x)\n" },
];
interface Playground {
  initial_view: (config: { locale: string; examples: typeof examples }) => Ui;
  main: (unit: undefined) => unknown;
}
let compiled: Promise<Playground> | undefined;
const compile = (): Promise<Playground> => compiled ??= (async () => {
  const out = path.join(repo, "target/tmp/playground-hydration");
  rmSync(out, { recursive: true, force: true });
  execFileSync(flick, ["build", path.join(here, "../src/playground/Playground.fl"), "--project", path.join(here, "../src/playground"), "--out-dir", out, "--rt", "--color", "never"]);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiled module exports what `Playground.fl` exposes
  return (await import(pathToFileURL(path.join(out, "Playground.js")).href)) as Playground;
})();

test("the playground hydrates the markup its page renders on the server", async () => {
  const playground = await compile();
  // as `Playground.astro` renders it
  const markup = renderToString(
    uiRenderStatic(await runPromise<Ui>(() => playground.initial_view({ locale: "en", examples }))),
  );
  const dom = new JSDOM(
    `<!doctype html><html><body><div id="flick-playground" data-locale="en" data-flick-ssr>${markup}</div>` +
      `<script type="application/json" id="flick-playground-examples">${JSON.stringify(examples)}</script></body></html>`,
  );
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  const complaints: Array<string> = [];
  const error = console.error;
  console.error = (...args: Array<unknown>) => complaints.push(args.map(String).join(" "));
  const running = run(playground.main);
  try {
    await new Promise((resolve) => setTimeout(resolve, 100));
    assert.deepEqual(
      complaints.filter((c) => /hydrat|did not match|didn't match/i.test(c)),
      [],
    );
    assert.ok(dom.window.document.querySelector("#flick-playground textarea"), "the editor is still rendered");
  } finally {
    // stops the program, which unmounts its roots
    hotDispose();
    await running;
    console.error = error;
  }
});

test("an exercise's goal renders from its check, and a page without an exercise has none", async () => {
  const playground = await compile();
  const goal = async (config: Record<string, unknown>): Promise<Element | null> => {
    const markup = renderToString(uiRenderStatic(await runPromise<Ui>(() => playground.initial_view({ locale: "en", examples, ...config }))));
    return new JSDOM(markup).window.document.querySelector(".fp-goal");
  };
  const lesson = { lesson: true, solution: "", done: false, autorun: false };
  const exercise = lessonConfiguration(parseExerciseMetadata([
    'export = "total"', 'argument_types = ["string", "number"]', 'return_type = "number"', "runs = true", 'output = ""',
    'title = "Total"', 'title_ru = "Сумма"', 'task = "Sum."', 'task_ru = "Сложите."', 'hint = "Add."', 'hint_ru = "Сложите."',
  ].join("\n"), "test/01"));
  const rows = (element: Element | null): Array<string | null> => [...element?.querySelectorAll(".fp-goal-row") ?? []].map((row) => row.textContent);
  assert.deepEqual(rows(await goal({ ...lesson, ...exercise })), ["Export (.d.ts)total: (string) => (number) => Effect<number, never, never>", "Expected outputnothing"]);
  assert.deepEqual(rows(await goal({ ...lesson, ...exercise, locale: "ru" })), ["Экспорт (.d.ts)total: (string) => (number) => Effect<number, never, never>", "Ожидаемый выводничего"]);
  // a page whose playground holds no exercise has empty metadata
  assert.equal(await goal({ ...lesson, ...lessonConfiguration(undefined) }), null);
});
