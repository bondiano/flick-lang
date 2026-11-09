import assert from "node:assert/strict";
import { after, test } from "node:test";

import type { Effect as EffectModule } from "effect";
import type { Atom } from "effect/reactivity/Atom";
import { JSDOM } from "jsdom";

// React's streamed markup finishes a boundary with inline scripts, so this
// document runs them; React is imported only after it is the global window.
const dom = new JSDOM("<!doctype html><html><body></body></html>", { runScripts: "dangerously", pretendToBeVisual: true });
Object.assign(globalThis, { window: dom.window, document: dom.window.document });
after(() => dom.window.close());

const { Effect, Fiber, Schema, Stream } = await import("effect");
const { RegistryService, flickLayer, flickSerializable } = await import("../src/effect/reactivity/Atom.ts");
const { ReadService, atomPending, atomSerializableTask, atomState, atomTask, syncOperation } = await import("../src/index.ts");
const { constructorName } = await import("@flick-lang/runtime/tags");
const { uiMount } = await import("../src/ui/index.ts");
const { uiFragment, uiRender, uiSuspended, uiText, uiView } = await import("../src/ui/react/index.ts");
const { uiServerRender, uiServerStream } = await import("../src/ui/server.ts");

type Eff<A, E = never> = EffectModule.Effect<A, E>;

const nothing = (): void => undefined;
const tick = (ms = 5): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));
const fields = (value: unknown): object => (typeof value === "object" && value !== null ? value : {});
/** A read in a runtime render (under `Ui.suspended`). */
const read = <A>(atom: Atom<A>) => Effect.map(Effect.service(ReadService), (reader) => reader.get(atom));
/** `Atom.get` as a Sync render calls it. */
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the operation takes the atom
const get = syncOperation(ReadService, "get", 1) as (atom: unknown) => unknown;

/** `Atom.ready atom` as the Flick module has it, for a task that does not fail. */
const ready = (atom: Atom<object>) =>
  Effect.flatMap(read(atom), (value): Eff<unknown> =>
    constructorName(String(Reflect.get(fields(value), "_tag"))) === "Loaded"
      ? Effect.succeed(Reflect.get(fields(value), "_0"))
      : atomPending(),
  );

/** A page whose user loads until `release`; the number of runs of its task is counted. */
const page = () => {
  let release = nothing;
  const runs = { count: 0 };
  const task = Effect.flatMap(
    Effect.callback<void>((resume) => {
      runs.count += 1;
      release = () => resume(Effect.void);
    }),
    () => Effect.succeed("ada"),
  );
  const count = flickSerializable("App.count", Schema.Number, atomState(0));
  const user = atomSerializableTask("App.user", Schema.String, atomTask(task));
  const counter = uiView(() => uiText(`count ${String(get(count))}`));
  const greeting = uiSuspended(
    () => uiText(" failed"),
    uiText(" waiting"),
    () => Effect.map(ready(user), (name) => uiText(` hello ${String(name)}`)),
  );
  const ui = uiFragment([uiRender(counter, undefined), uiRender(greeting, undefined)]);
  return { count, ui, runs, release: () => release() };
};

/**
 * Appends `html` to `container` as a parser would, running its scripts;
 * like a browser's parser, it runs microtasks before each script.
 */
const append = async (container: Element, html: string): Promise<void> => {
  const template = dom.window.document.createElement("template");
  template.innerHTML = html;
  for (const node of Array.from(template.content.childNodes)) {
    if (node instanceof dom.window.HTMLScriptElement) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- each script waits for the microtasks before it, in order
      await Promise.resolve();
      const script = dom.window.document.createElement("script");
      for (const { name, value } of Array.from(node.attributes)) script.setAttribute(name, value);
      script.text = node.text;
      container.append(script);
    } else container.append(node);
  }
};

test("a streamed page sends a loading task's fallback, then its markup after its value, and hydrates without running it again", async () => {
  const { count, ui, runs, release } = page();
  const server = Effect.gen(function* () {
    const registry = yield* Effect.service(RegistryService);
    yield* registry.set(count)(5);
    const chunks: Array<string> = [];
    // the task ends once the shell, with its fallback, is out
    yield* Stream.runForEach(uiServerStream("app", ui), (chunk) =>
      Effect.sync(() => {
        chunks.push(chunk);
        if (chunk.includes(" waiting")) release();
      }),
    );
    return chunks;
  });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the request's one service
  const chunks = Array.from(await Effect.runPromise(Effect.provide(server, flickLayer(RegistryService)) as Eff<Array<string>>));
  const at = (pattern: RegExp): number => chunks.findIndex((chunk) => pattern.test(chunk));
  assert.equal(chunks[0], '<script type="application/json" data-flick-state="app">[{"key":"App.count","value":5}]</script>');
  assert.match(chunks[1] ?? "", /count 5.*<!--\$\?-->.* waiting/);
  const value = at(/^<script type="application\/json" data-flick-state="app">\[\{"key":"App.user","value":"ada"\}\]<\/script>$/);
  const markup = at(/ hello ada/);
  assert.ok(value > 1 && markup > value, `the value comes before the markup that shows it: ${JSON.stringify(chunks)}`);
  assert.equal(runs.count, 1);

  // the client mounts on the shell; the rest arrives after
  const errors: Array<string> = [];
  const complain = console.error;
  // the server's and the client's React share this process and its contexts, which a page never does
  const shared = /multiple renderers concurrently rendering the same context provider/;
  console.error = (...args: Array<unknown>) => {
    const message = args.map(String).join(" ");
    if (!shared.test(message)) errors.push(message);
  };
  // a document is still loading while its markup streams in; React client-renders a boundary still pending once it is not
  let loading = true;
  Object.defineProperty(dom.window.document, "readyState", { configurable: true, get: () => (loading ? "loading" : "complete") });
  const body = dom.window.document.body;
  await append(body, chunks.slice(0, 2).join(""));
  const container = dom.window.document.getElementById("app");
  assert.ok(container !== null, "the shell renders the container");
  const mounted = Effect.provide(uiMount(container, () => Effect.void, ui), flickLayer(RegistryService));
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the root's one service
  const fiber = Effect.runFork(mounted as Eff<never, unknown>);
  try {
    await tick(20);
    assert.equal(container.textContent, "count 5 waiting");
    await append(body, chunks.slice(2).join(""));
    // React reveals a boundary at most every 300 ms
    await tick(700);
    assert.equal(body.querySelector("[data-flick-state]"), null);
    assert.match(container.textContent ?? "", /^count 5 hello ada/);
  } finally {
    await Effect.runPromise(Fiber.interrupt(fiber));
    console.error = complain;
    loading = false;
  }
  assert.equal(runs.count, 1, "the client starts from the server's value");
  assert.deepEqual(errors, []);
});

test("a page rendered at once shows a loading task's fallback inside the boundary a streamed one has", async () => {
  const { ui } = page();
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the request's one service
  const markup = await Effect.runPromise(Effect.provide(uiServerRender(ui), flickLayer(RegistryService)) as Eff<string>);
  assert.equal(markup, "count 0<!--$--> waiting<!--/$-->");
});

test("a task read outside a suspended view streams as loading and is not waited for", async () => {
  // a task that never ends: the stream must end without it
  const user = atomTask(Effect.never);
  const status = uiView(() => uiText(constructorName(String(Reflect.get(fields(get(user)), "_tag")))));
  const server = Stream.runCollect(uiServerStream("app", uiRender(status, undefined)));
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the request's one service
  const chunks = Array.from(await Effect.runPromise(Effect.provide(server, flickLayer(RegistryService)) as Eff<Array<string>>));
  assert.equal(chunks.join(""), '<div id="app" data-flick-ssr="">Loading</div>');
});
