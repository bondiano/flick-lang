// Lists, non-empty lists, mutable JS arrays and chunks. A Flick function is
// curried: a callback of two arguments is called `f(a)(b)`, and a runtime
// callback returns an `Effect`.
import { Chunk, Effect } from "effect";

import { EQ, LT, none, some } from "../data.js";

export const identity = (x) => x;

export const listMap = (f, xs) => xs.map((x) => f(x));
export const listMapIndexed = (f, xs) => xs.map((x, i) => f(i)(x));
export const listMapEffect = (f, xs) => Effect.forEach(xs, (x) => f(x));
export const listEachEffect = (f, xs) => Effect.forEach(xs, (x) => f(x), { discard: true });

export const listFold = (f, init, xs) => {
  let acc = init;
  for (const x of xs) acc = f(acc)(x);
  return acc;
};

export const listFoldRight = (f, init, xs) => {
  let acc = init;
  for (let i = xs.length - 1; i >= 0; i--) acc = f(xs[i])(acc);
  return acc;
};

export const listFoldEffect = (f, init, xs) => Effect.reduce(xs, init, (acc, x) => f(acc)(x));

export const listFilter = (p, xs) => xs.filter((x) => p(x));

export const listFilterMap = (f, xs) => {
  const out = [];
  for (const x of xs) {
    const o = f(x);
    if (o._tag === "Some") out.push(o.value);
  }
  return out;
};

export const listFlatMap = (f, xs) => xs.flatMap((x) => f(x));

export const listFind = (p, xs) => {
  for (const x of xs) if (p(x)) return some(x);
  return none;
};

export const listFindIndex = (p, xs) => {
  const i = xs.findIndex((x) => p(x));
  return i < 0 ? none : some(i);
};

export const listAny = (p, xs) => xs.some((x) => p(x));
export const listAll = (p, xs) => xs.every((x) => p(x));

export const listPartition = (p, xs) => {
  const yes = [];
  const no = [];
  for (const x of xs) (p(x) ? yes : no).push(x);
  return [yes, no];
};

const order = (o) => (o._tag === LT._tag ? -1 : o._tag === EQ._tag ? 0 : 1);

/** A stable sort: `Array.prototype.sort` is stable, on a copy. */
export const listSortBy = (cmp, xs) => [...xs].sort((a, b) => order(cmp(a)(b)));

export const listLength = (xs) => xs.length;

const index = (i, n) => Number.isInteger(i) && i >= 0 && i < n;

export const listGet = (i, xs) => (index(i, xs.length) ? some(xs[i]) : none);

export const listSet = (i, x, xs) => {
  if (!index(i, xs.length)) return xs;
  const out = [...xs];
  out[i] = x;
  return out;
};

export const listSingleton = (x) => [x];

export const listRepeat = (n, x) => Array.from({ length: Math.max(0, Math.trunc(n)) || 0 }, () => x);

export const listRange = (from, until) => {
  const out = [];
  for (let x = from; x < until; x++) out.push(x);
  return out;
};

export const listInitialize = (n, f) =>
  Array.from({ length: Math.max(0, Math.trunc(n)) || 0 }, (_, i) => f(i));

export const listReverse = (xs) => [...xs].reverse();
export const listConcat = (xss) => xss.flat(1);
export const listTake = (n, xs) => xs.slice(0, Math.max(0, n));
export const listDrop = (n, xs) => xs.slice(Math.max(0, n));
export const listSlice = (start, end, xs) => xs.slice(start, end);

export const listZip = (xs, ys) => {
  const n = Math.min(xs.length, ys.length);
  const out = new Array(n);
  for (let i = 0; i < n; i++) out[i] = [xs[i], ys[i]];
  return out;
};

export const listUnzip = (pairs) => [pairs.map((p) => p[0]), pairs.map((p) => p[1])];

export const nonEmptyFromList = (xs) => (xs.length > 0 ? some(xs) : none);
export const nonEmptyCons = (x, xs) => [x, ...xs];
export const nonEmptyHead = (xs) => xs[0];

export const jsArrayMake = () => [];
export const jsArrayFromList = (xs) => [...xs];
export const jsArrayPush = (array, x) => {
  array.push(x);
};
export const jsArraySet = (i, x, array) => {
  if (index(i, array.length + 1)) array[i] = x;
};

export const chunkEmpty = Chunk.empty();
export const chunkFromList = (xs) => Chunk.fromIterable(xs);
export const chunkToList = (c) => Chunk.toArray(c);
export const chunkAppend = (x, c) => Chunk.append(c, x);
export const chunkConcat = (a, b) => Chunk.appendAll(a, b);
export const chunkSize = (c) => Chunk.size(c);
export const chunkGet = (i, c) => (index(i, Chunk.size(c)) ? some(Chunk.getUnsafe(c, i)) : none);
export const chunkMap = (f, c) => Chunk.map(c, (x) => f(x));

/** `Monad (Eff r e)` and `bind_eff`. */
export const succeed = (x) => Effect.succeed(x);
export const flatMapEff = (f, eff) => Effect.flatMap(eff, (x) => f(x));
export const mapEff = (f, eff) => Effect.map(eff, (x) => f(x));
export const bindEff = (eff, f) => Effect.flatMap(eff, (x) => f(x));

const CONTINUE = "Loop.Step.Continue";

/** `Loop.loop step state`: runs until `Done`, without growing the stack. */
export const loop = (step, state) => {
  let s = state;
  for (;;) {
    const next = step(s);
    if (next._tag !== CONTINUE) return next._0;
    s = next._0;
  }
};

export const trampolineDone = (value) => ({ done: true, value });
export const trampolineSuspend = (thunk) => ({ done: false, thunk });
export const trampolineRun = (t) => {
  let current = t;
  while (!current.done) current = current.thunk(undefined);
  return current.value;
};
