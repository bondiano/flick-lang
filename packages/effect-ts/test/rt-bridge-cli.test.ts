import assert from "node:assert/strict";
import { test } from "node:test";

import { Context, Effect, Layer, Option } from "effect";
import { err, ok } from "@flick-lang/runtime/core/data";
import { layerSucceed, none, runIn, service, some } from "@flick-lang/runtime/rt";
import { offer, unbounded } from "@flick-lang/runtime/rt/effect/Queue";
import { tag } from "@flick-lang/runtime/tags";
import { currentCtx, fork, Suspend } from "@flick-lang/runtime/rt/fiber";

import { cliRunVersion, multiRequired, promptCustomEvents, promptRun, promptText, specCommand, specFlag, specHandle } from "../src/cli/index.ts";
import { fromEffect, layerToEffect } from "../src/bridge.ts";
import type { TtyImpl } from "../src/cli/prompt.ts";

const run = (effect: Effect.Effect<unknown, unknown, unknown>): Promise<unknown> =>
  new Promise((resolve, reject) => {
    fork(() => fromEffect(effect), currentCtx()).observe((exit) => {
      if (exit.cause === undefined) resolve(exit.value);
      else reject(new Error(JSON.stringify(exit.cause)));
    });
  });

test("a native Cli.Value dictionary parses a flag for an Effect CLI command", async () => {
  let observed: unknown;
  const value = { parse: (text: string) => ok(text) };
  const spec = specFlag(value, undefined, undefined, "name", [], "", multiRequired, specCommand("tool"));
  const command = specHandle((options) => Effect.sync(() => { observed = options["name"]; }), spec);
  await run(cliRunVersion("1.0.0", command, ["--name", "Ada"]));
  assert.equal(observed, "Ada");
});

test("a native runtime Cli.Value parser is evaluated before its result is read", async () => {
  let observed: unknown;
  const value = { parse: function* (text: string) { yield undefined; return ok(text.toUpperCase()); } };
  const spec = specFlag(value, undefined, undefined, "name", [], "", multiRequired, specCommand("tool"));
  const command = specHandle((options) => Effect.sync(() => { observed = options["name"]; }), spec);
  await run(cliRunVersion("1.0.0", command, ["--name", "Ada"]));
  assert.equal(observed, "ADA");
});

test("a native runtime Cli.Value rejection retains its expected input message", async () => {
  const value = { parse: function* (_text: string) { yield undefined; return err("choose a digit"); } };
  const spec = specFlag(value, undefined, undefined, "digit", [], "", multiRequired, specCommand("tool"));
  const command = specHandle(() => Effect.die("an invalid parameter must not run the handler"), spec);
  await assert.rejects(run(cliRunVersion("1.0.0", command, ["--digit", "x"])), /choose a digit/);
});

test("a prompt reads a native Tty service through the bridge", async () => {
  const key = Context.Service<TtyImpl>("Test.NativeTty");
  const inputs = [
    { input: Option.some("A"), key: { name: "A", ctrl: false, meta: false, shift: false } },
    { input: Option.none(), key: { name: "return", ctrl: false, meta: false, shift: false } },
  ];
  const tty: TtyImpl = {
    read_key: () => new Suspend((resolve) => { queueMicrotask(() => resolve(inputs.shift())); }),
    display: () => undefined,
    columns: () => 80,
    rows: () => 24,
  };
  const prompt = promptText({ message: "Name?", default: Option.none(), validate: Option.none() });
  assert.equal(await run(Effect.provideService(promptRun(key, prompt), key, tty)), "A");
});

test("a prompt reads a native Tty service from a native layer", async () => {
  const inputs = [
    { input: some("A"), key: { name: "A", ctrl: false, meta: false, shift: false } },
    { input: none, key: { name: "return", ctrl: false, meta: false, shift: false } },
  ];
  const tty: TtyImpl = {
    read_key: () => new Suspend((resolve) => { queueMicrotask(() => resolve(inputs.shift())); }),
    display: () => undefined,
    columns: () => 80,
    rows: () => 24,
  };
  const prompt = promptText({ message: "Name?", default: Option.none(), validate: Option.none() });
  const layer = layerSucceed(service("Test.LayerTty"), tty);
  const host = layerToEffect(layer);
  assert.ok(Layer.isLayer(host));
  assert.equal(await run(Effect.provide(promptRun(service("Test.LayerTty"), prompt), host)), "A");
});

test("a prompt consumes an event from a native queue", async () => {
  const key = service("Test.EventTty");
  const queue = unbounded();
  runIn(offer("arrived")(queue));
  const tty: TtyImpl = {
    read_key: () => Effect.never,
    display: () => undefined,
    columns: () => 80,
    rows: () => 24,
  };
  const handlers = {
    render: () => () => "waiting",
    process: (input: { readonly _0?: unknown }) => () => ({ _tag: tag("Cli.Prompt.Action.Submit"), _0: input._0 }),
    clear: () => () => "",
  };
  const prompt = promptCustomEvents(0, queue, handlers);
  const host = layerToEffect(layerSucceed(key, tty));
  assert.ok(Layer.isLayer(host));
  assert.equal(await run(Effect.provide(promptRun(key, prompt), host)), "arrived");
});
