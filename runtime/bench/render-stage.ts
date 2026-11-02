// What a render costs with atom reads as effects (`Effect.gen` run by
// `runSync`, as the compiler emits now) against plain calls (a `Sync`
// stage). Run: `node runtime/bench/render-stage.ts`.
import { Effect } from "effect";
import { createElement, type ReactNode } from "react";
import { renderToString } from "react-dom/server";

import { ReadService, runReading } from "../src/atoms.ts";
import { operation } from "../src/effects.ts";

type Atom = { readonly n: number };
const atoms: readonly [Atom, Atom, Atom] = [{ n: 0 }, { n: 1 }, { n: 2 }];
const values = new Map<unknown, number>(atoms.map((a) => [a, a.n * 10]));
const read = (atom: unknown): number => values.get(atom) ?? 0;

// as the compiler emits `Atom.get` and a render that reads three atoms
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the call `operation` builds
const readOp = operation(ReadService, "get", 1) as (atom: Atom) => Effect.Effect<number>;
const get = (atom: Atom) => Effect.gen(function* () { return yield* readOp(atom); });
const viaEffect = (i: number): Effect.Effect<ReactNode> => Effect.gen(function* () {
  const a = yield* get(atoms[0]);
  const b = yield* get(atoms[1]);
  const c = yield* get(atoms[2]);
  return createElement("li", { key: i }, `${a + b + c + i}`);
});
// the same render in a Sync stage: `Atom.get` is a direct call
const direct = (i: number): ReactNode => {
  const a = read(atoms[0]);
  const b = read(atoms[1]);
  const c = read(atoms[2]);
  return createElement("li", { key: i }, `${a + b + c + i}`);
};

const Before = ({ i }: { i: number }) => runReading(viaEffect(i), read);
const After = ({ i }: { i: number }) => direct(i);
const page = (row: typeof Before, n: number) => createElement("ul", null, Array.from({ length: n }, (_, i) => createElement(row, { key: i, i })));

const time = (label: string, run: () => unknown, rounds: number): number => {
  for (let i = 0; i < rounds / 10; i++) run();
  const start = performance.now();
  for (let i = 0; i < rounds; i++) run();
  const per = (performance.now() - start) / rounds;
  console.log(`${label.padEnd(28)} ${(per * 1000).toFixed(2).padStart(9)} µs`);
  return per;
};

const report = (what: string, before: number, after: number) =>
  console.log(`${what}: after is ${(100 * (1 - after / before)).toFixed(1)} % faster\n`);

const b1 = time("render body, effect", () => runReading(viaEffect(1), read), 200_000);
const a1 = time("render body, direct", () => direct(1), 200_000);
report("render body", b1, a1);
const b2 = time("SSR 500 views, effect", () => renderToString(page(Before, 500)), 300);
const a2 = time("SSR 500 views, direct", () => renderToString(page(After, 500)), 300);
report("SSR page", b2, a2);
