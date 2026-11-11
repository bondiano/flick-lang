/* oxlint-disable typescript/no-unsafe-type-assertion -- the values of atoms are what they were made with */
// `Ui.Server.stream` on Flick's own runtime, as the Effect adapter streams:
// the shell with a suspended view's fallback first, then the serializable
// task's value, then its markup; an error after the shell fails the stream;
// cancelling it releases what it watched, once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { uiText } from "../src/ui/react/nodes.ts";
import { atomPending, atomSerializableTask, atomTask } from "../src/rt/atoms/atom.ts";
import { type Atom, Registry } from "../src/rt/atoms/registry.ts";
import { READ, REGISTRY } from "../src/rt/atoms/services.ts";
import { extend, inCtx } from "../src/rt/fiber.ts";
import { implementation } from "../src/rt/handle.ts";
import { currentCtx } from "../src/rt/kernel.ts";
import * as Deferred from "../src/rt/effect/Deferred.ts";
import { schemaString } from "../src/rt/schema.ts";
import { tag } from "../src/tags.ts";
import { uiRender, uiSuspended } from "../src/rt/ui/views.ts";
import { uiServerStream } from "../src/rt/ui/server.ts";
import { streamHooks } from "../src/rt/ui/stream_hook.ts";

const LOADED = tag("Atom.Async.Loaded");

// the adapter's bridge would make a `Stream` of it; here the stream of text is enough
streamHooks.text = (start) => start();

/** `Atom.ready` in a view: the loaded value, or the pending read a suspended view shows its fallback for. */
const ready = (atom: Atom): unknown => {
  const value = (implementation(READ) as { get(a: Atom): unknown }).get(atom);
  return Reflect.get(Object(value), "_tag") === LOADED ? Reflect.get(Object(value), "_0") : atomPending();
};

/** The stream of `view` rendered with `registry`. */
const streamOf = (registry: Registry, view: Parameters<typeof uiRender>[0]): ReadableStream<string> =>
  inCtx(extend(currentCtx(), REGISTRY, { registry }), () => uiServerStream("app", uiRender(view, undefined))) as ReadableStream<string>;

const readAll = async (stream: ReadableStream<string>): Promise<Array<string>> => {
  const chunks: Array<string> = [];
  for await (const chunk of stream) chunks.push(chunk);
  return chunks;
};

test("the shell with the fallback comes first, then the task's state, then its markup", async () => {
  const gate = Deferred.make();
  const user = atomSerializableTask("App.user", schemaString, atomTask(() => Deferred.await(gate)) as Atom<object>);
  const view = uiSuspended(() => uiText("failed"), uiText("waiting"), () => uiText(`hello ${String(ready(user))}`));
  const registry = new Registry(currentCtx());
  const chunks = readAll(streamOf(registry, view));
  setTimeout(() => Deferred.succeed("ann")(gate), 20);
  const text = (await chunks).join("");
  const shell = text.indexOf("waiting");
  const state = text.indexOf('{"key":"App.user","value":"ann"}');
  const markup = text.indexOf("hello ann");
  assert.ok(text.startsWith('<div id="app" data-flick-ssr="">'), text);
  assert.ok(shell >= 0 && state > shell && markup > state, text);
});

test("an error in a view after the shell fails the stream", async () => {
  const gate = Deferred.make();
  const late = atomTask(() => Deferred.await(gate));
  const view = uiSuspended(() => uiText("failed"), uiText("waiting"), () => {
    ready(late);
    throw new Error("broken view");
  });
  const registry = new Registry(currentCtx());
  const chunks = readAll(streamOf(registry, view));
  setTimeout(() => Deferred.succeed("x")(gate), 20);
  await assert.rejects(chunks, /broken view/);
});

test("cancelling the stream releases the atoms it watched, once", async () => {
  const gate = Deferred.make();
  const late = atomTask(() => Deferred.await(gate));
  const view = uiSuspended(() => uiText("failed"), uiText("waiting"), () => uiText(String(ready(late))));
  const registry = new Registry(currentCtx());
  const reader = streamOf(registry, view).getReader();
  const first = await reader.read();
  assert.match(String(first.value), /waiting/);
  const watched = (): number => [...registry.all()].filter((n) => n.atom === late).reduce((sum, n) => sum + n.listeners.size, 0);
  assert.equal(watched(), 1);
  await reader.cancel();
  await reader.cancel();
  assert.equal(watched(), 0);
});
