import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Fiber } from "effect";
import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { JSDOM } from "jsdom";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import { ReadService, atomDerived, atomFamily, atomState } from "../src/index.ts";
import { type Renderer, type Ui, uiFragment, uiNamed, uiRunWith, uiRender, uiText, uiView } from "../src/ui/react/index.ts";

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

test("a family gives the same atom for keys equal by Eq, and another for another key", () => {
  const byId = { hash: (p: { readonly id: number }) => p.id % 2, $Eq: { eq: (a: { readonly id: number }) => (b: { readonly id: number }) => a.id === b.id } };
  let made = 0;
  const family = atomFamily(byId, (p: { readonly id: number }) => {
    made += 1;
    return atomState(p.id);
  });
  const first = family({ id: 1 });
  assert.equal(family({ id: 1 }), first);
  assert.notEqual(family({ id: 3 }), first);
  assert.equal(made, 2);
});
