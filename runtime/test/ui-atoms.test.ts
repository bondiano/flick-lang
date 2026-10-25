import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Fiber } from "effect";
import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { JSDOM } from "jsdom";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import { WriteService, atomLayer, nativeRegistry } from "../src/effect/reactivity/Atom.ts";
import { ReadService, atomAt, atomDerived, atomFamily, atomForget, atomState, label as compilerLabel } from "../src/index.ts";
import { type Renderer, type Ui, eventExhaust, uiFragment, uiNamed, uiRunWith, uiRender, uiText, uiView } from "../src/ui/react/index.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.assign(globalThis, { window: dom.window, document: dom.window.document });

const tick = (ms = 5): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** A synchronous React DOM root in a fresh element. */
const domRoot = (): { container: Element; renderer: Renderer } => {
  const container = dom.window.document.createElement("div");
  dom.window.document.body.append(container);
  let root: Root | undefined;
  return {
    container,
    renderer: () => ({
      render: (node) =>
        flushSync(() => {
          root ??= createRoot(container);
          root.render(node);
        }),
      unmount: () => root?.unmount(),
    }),
  };
};

/** `Atom.get atom` as a Flick render does it: through `Atom.Read`. */
const get = <A>(atom: Atom.Atom<A>): Effect.Effect<A> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the read gives the atom's own value
  Effect.flatMap(Effect.service(ReadService), (read) => read.get(atom)) as Effect.Effect<A>;

/** A view showing `atom`, counting its renders. */
const counting = (label: string, atom: Atom.Atom<number>, renders: Array<string>) =>
  uiNamed(
    label,
    uiView(() =>
      Effect.map(get(atom), (n) => {
        renders.push(label);
        return uiText(`${label}=${n}`);
      }),
    ),
  );

const mount = (ui: Ui, registry: AtomRegistry.AtomRegistry) => {
  const { container, renderer } = domRoot();
  const fiber = Effect.runFork(uiRunWith(renderer, () => Effect.void, ui, registry));
  return { container, stop: () => Effect.runPromise(Fiber.interrupt(fiber)) };
};

test("a view renders again only when an atom it read changes", async () => {
  const registry = AtomRegistry.make();
  const left = Atom.make(0);
  const right = Atom.make(10);
  const renders: Array<string> = [];
  const app = uiFragment([uiRender(counting("App.Left", left, renders), undefined), uiRender(counting("App.Right", right, renders), undefined)]);
  const { container, stop } = mount(app, registry);
  await tick();
  assert.equal(container.textContent, "App.Left=0App.Right=10");
  assert.deepEqual(renders, ["App.Left", "App.Right"]);
  flushSync(() => registry.set(left, 1));
  await tick();
  assert.equal(container.textContent, "App.Left=1App.Right=10");
  assert.deepEqual(renders, ["App.Left", "App.Right", "App.Left"]);
  await stop();
});

test("a derived atom read in a view follows the atoms it reads", async () => {
  const registry = AtomRegistry.make();
  const count = Atom.make(2);
  const doubled = atomDerived(() => Effect.map(get(count), (n) => n * 2));
  const renders: Array<string> = [];
  const { container, stop } = mount(uiRender(counting("App.Doubled", doubled, renders), undefined), registry);
  await tick();
  flushSync(() => registry.set(count, 5));
  await tick();
  assert.equal(container.textContent, "App.Doubled=10");
  assert.equal(renders.length, 2);
  await stop();
});

test("React's tools show a view by its label", async () => {
  const registry = AtomRegistry.make();
  const { container, stop } = mount(uiRender(counting("App.Counter.counter_view", Atom.make(0), []), undefined), registry);
  await tick();
  const key = Object.keys(container).find((k) => k.startsWith("__reactContainer"));
  assert.ok(key !== undefined, "React owns the container");
  const names: Array<string> = [];
  const walk = (fiber: unknown): void => {
    if (typeof fiber !== "object" || fiber === null) return;
    const type: unknown = Reflect.get(fiber, "elementType");
    if (typeof type === "object" && type !== null) {
      const name: unknown = Reflect.get(type, "displayName");
      if (typeof name === "string") names.push(name);
    }
    walk(Reflect.get(fiber, "child"));
    walk(Reflect.get(fiber, "sibling"));
  };
  // the container holds the first root fiber; its root points at the current tree
  const first: unknown = Reflect.get(container, key);
  const fiberRoot: unknown = typeof first === "object" && first !== null ? Reflect.get(first, "stateNode") : undefined;
  walk(typeof fiberRoot === "object" && fiberRoot !== null ? Reflect.get(fiberRoot, "current") : undefined);
  assert.ok(names.includes("App.Counter.counter_view"), names.join(", "));
  await stop();
});

test("a write between a render and its subscription renders again", async () => {
  const registry = AtomRegistry.make();
  const count = Atom.make(0);
  let wrote = false;
  const view = uiView(() =>
    Effect.map(get(count), (n) => {
      // the first render writes before the view has subscribed
      if (!wrote) {
        wrote = true;
        registry.set(count, 7);
      }
      return uiText(`n=${n}`);
    }),
  );
  const { container, stop } = mount(uiRender(view, undefined), registry);
  await tick();
  assert.equal(container.textContent, "n=7");
  await stop();
});

test("a view without a registry fails with what to do", async (t) => {
  // React reports the error it caught; the failure is what this test reads
  t.mock.method(console, "error", () => undefined);
  const { renderer } = domRoot();
  const failures: Array<string> = [];
  const view = uiView(() => Effect.succeed(uiText("x")));
  const fiber = Effect.runFork(uiRunWith(renderer, () => Effect.void, uiRender(view, undefined)));
  const exit = await Effect.runPromise(Fiber.await(fiber));
  if (exit._tag === "Failure") failures.push(String(exit.cause));
  assert.match(failures.join(""), /Ui\.Dom\.mount/);
});

test("a state keeps a value written before anything read it", async () => {
  const registry = AtomRegistry.make();
  const count = atomState(0);
  registry.set(count, 3);
  await tick();
  assert.equal(registry.get(count), 3);
});

type Id = { readonly id: number };
const byId = { hash: (p: Id) => p.id % 2, $Eq: { eq: (a: Id) => (b: Id) => a.id === b.id } };

test("a family gives the same atom for keys equal by Eq, and another for another key", () => {
  const family = atomFamily(byId, (p: Id) => atomState(p.id));
  const first = atomAt(family, { id: 1 });
  assert.equal(atomAt(family, { id: 1 }), first);
  assert.notEqual(atomAt(family, { id: 3 }), first);
});

test("a forgotten key gets a new atom, and the registry drops the old state once nothing reads it", async () => {
  const family = atomFamily(byId, (p: Id) => atomState(p.id));
  const [old, kept] = [atomAt(family, { id: 1 }), atomAt(family, { id: 2 })];
  const program = Effect.gen(function* () {
    const registry = nativeRegistry(yield* Effect.service(WriteService));
    assert.ok(registry !== undefined);
    registry.set(old, 10);
    registry.set(kept, 20);
    yield* atomForget(family, { id: 1 });
    yield* Effect.promise(() => tick());
    return [registry.getNodes().has(old), registry.getNodes().has(kept)];
  });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the program's one service
  assert.deepEqual(await Effect.runPromise(Effect.provide(program, atomLayer) as Effect.Effect<Array<boolean>>), [false, true]);
  assert.notEqual(atomAt(family, { id: 1 }), old);
  assert.equal(atomAt(family, { id: 2 }), kept);
});

test("a state forgotten while a view reads it goes when the view stops reading", async () => {
  const family = atomFamily(byId, (p: Id) => atomState(p.id));
  const old = atomAt(family, { id: 1 });
  const program = Effect.gen(function* () {
    const registry = nativeRegistry(yield* Effect.service(WriteService));
    assert.ok(registry !== undefined);
    const unsubscribe = registry.subscribe(old, () => undefined);
    yield* atomForget(family, { id: 1 });
    yield* Effect.promise(() => tick());
    const whileRead = registry.getNodes().has(old);
    unsubscribe();
    yield* Effect.promise(() => tick());
    return [whileRead, registry.getNodes().has(old)];
  });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the program's one service
  assert.deepEqual(await Effect.runPromise(Effect.provide(program, atomLayer) as Effect.Effect<Array<boolean>>), [true, false]);
});

test("the compiler's label names an atom, a view and an action, and keeps a name given by hand", () => {
  const atom = compilerLabel("App.Counter.count", atomState(0));
  assert.equal(atom.label?.[0], "App.Counter.count");
  assert.equal(atom.keepAlive, true);
  const view = uiView(() => Effect.succeed(uiText("")));
  assert.equal(compilerLabel("App.Counter.counter_view", view).label, "App.Counter.counter_view");
  assert.equal(compilerLabel("App.Counter.counter_view", uiNamed("Counter", view)).label, "Counter");
  const action = compilerLabel("App.Counter.reset", eventExhaust(() => Effect.void));
  assert.equal(action.label, "App.Counter.reset");
  assert.equal(action.policy.kind, "exhaust");
});
