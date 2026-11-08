import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as Effect from "@flick-lang/runtime/effect/Effect";
import * as Fiber from "@flick-lang/runtime/effect/Fiber";
import { type Ui, uiRenderStatic } from "@flick-lang/runtime/ui/react";
import { JSDOM } from "jsdom";
import { renderToString } from "react-dom/server";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(repo, "target"), "debug/flick");

test("the playground hydrates the markup its page renders on the server", async () => {
  const out = path.join(repo, "target/tmp/playground-hydration");
  rmSync(out, { recursive: true, force: true });
  execFileSync(flick, ["build", path.join(here, "../src/playground/Playground.fl"), "--out-dir", out, "--color", "never"]);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiled module exports what `Playground.fl` exposes
  const playground = (await import(pathToFileURL(path.join(out, "Playground.js")).href)) as {
    initial_view: (config: { mode: string; locale: string; examples: typeof examples }) => Effect.Effect<Ui>;
    main: (unit: undefined) => Effect.Effect<void, unknown>;
  };
  const examples = [
    { id: "hello", group: "Basics", group_ru: "Основы", name: "Hello", name_ru: "Привет", source: "module Main exposing (main)\n" },
    { id: "other", group: "Types", group_ru: "Типы", name: "Other", name_ru: "Другое", source: "module Other exposing (x)\n" },
  ];
  // as `Playground.astro` renders it
  const markup = renderToString(
    uiRenderStatic(await Effect.runPromise(playground.initial_view({ mode: "play", locale: "en", examples }))),
  );
  const dom = new JSDOM(
    `<!doctype html><html><body><div id="flick-playground" data-mode="play" data-locale="en" data-flick-ssr>${markup}</div>` +
      `<script type="application/json" id="flick-playground-examples">${JSON.stringify(examples)}</script></body></html>`,
  );
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  const complaints: Array<string> = [];
  const error = console.error;
  console.error = (...args: Array<unknown>) => complaints.push(args.map(String).join(" "));
  const fiber = Effect.runFork(playground.main(undefined));
  try {
    await new Promise((resolve) => setTimeout(resolve, 100));
    assert.deepEqual(
      complaints.filter((c) => /hydrat|did not match|didn't match/i.test(c)),
      [],
    );
    assert.ok(dom.window.document.querySelector("#flick-playground textarea"), "the editor is still rendered");
  } finally {
    await Effect.runPromise(Fiber.interrupt(fiber));
    console.error = error;
  }
});
