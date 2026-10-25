// Lists, non-empty lists, mutable JS arrays and chunks. A callback comes
// adapted to its JS arity, `f(a, b)`; a runtime callback returns an `Effect`.
import { Chunk, Effect, type Option } from "effect";

import { type Tagged, none, some } from "../data.ts";
import { item } from "../internal.ts";
import { tag } from "../tags.ts";

type List<A> = ReadonlyArray<A>;

export {
  identity,
  listMap,
  listMapIndexed,
  listFold,
  listFoldRight,
  listFilter,
  listFilterMap,
  listFlatMap,
  listAny,
  listAll,
  listPartition,
  flickOrder,
  listSortBy,
  listLength,
  listSet,
  listSingleton,
  listRepeat,
  listRange,
  listInitialize,
  listReverse,
  listConcat,
  listTake,
  listDrop,
  listSlice,
  listZip,
  listUnzip,
  nonEmptyCons,
  nonEmptyHead,
  jsArrayMake,
  jsArrayFromList,
  jsArrayPush,
  jsArraySet,
} from "../core/list.ts";

export const listMapEffect = <A, B, E, R>(f: (a: A) => Effect.Effect<B, E, R>, xs: List<A>): Effect.Effect<Array<B>, E, R> =>
  Effect.forEach(xs, (x) => f(x));

export const listEachEffect = <A, E, R>(f: (a: A) => Effect.Effect<unknown, E, R>, xs: List<A>): Effect.Effect<void, E, R> =>
  Effect.forEach(xs, (x) => f(x), { discard: true });

export const listFoldEffect = <A, B, E, R>(
  f: (acc: B, a: A) => Effect.Effect<B, E, R>,
  init: B,
  xs: List<A>,
): Effect.Effect<B, E, R> => Effect.reduce(xs, () => init, (acc, x) => f(acc, x));

// Option-making helpers: Effect's `Option` here, plain data in `core/list.ts`
export const listFind = <A>(p: (a: A) => boolean, xs: List<A>): Option.Option<A> => {
  for (const x of xs) if (p(x)) return some(x);
  return none;
};

export const listFindIndex = <A>(p: (a: A) => boolean, xs: List<A>): Option.Option<number> => {
  const i = xs.findIndex((x) => p(x));
  return i < 0 ? none : some(i);
};

const index = (i: number, n: number): boolean => Number.isInteger(i) && i >= 0 && i < n;

export const listGet = <A>(i: number, xs: List<A>): Option.Option<A> => (index(i, xs.length) ? some(item(xs, i)) : none);

export const nonEmptyFromList = <A>(xs: List<A>): Option.Option<List<A>> => (xs.length > 0 ? some(xs) : none);

export const chunkEmpty: Chunk.Chunk<never> = /*#__PURE__*/ Chunk.empty();
export const chunkFromList = <A>(xs: List<A>): Chunk.Chunk<A> => Chunk.fromIterable(xs);
export const chunkToList = <A>(c: Chunk.Chunk<A>): Array<A> => Chunk.toArray(c);
export const chunkAppend = <A>(x: A, c: Chunk.Chunk<A>): Chunk.Chunk<A> => Chunk.append(c, x);
export const chunkConcat = <A>(a: Chunk.Chunk<A>, b: Chunk.Chunk<A>): Chunk.Chunk<A> => Chunk.appendAll(a, b);
export const chunkSize = <A>(c: Chunk.Chunk<A>): number => Chunk.size(c);
export const chunkGet = <A>(i: number, c: Chunk.Chunk<A>): Option.Option<A> =>
  index(i, Chunk.size(c)) ? some(Chunk.getUnsafe(c, i)) : none;
export const chunkMap = <A, B>(f: (a: A) => B, c: Chunk.Chunk<A>): Chunk.Chunk<B> => Chunk.map(c, (x) => f(x));

/** `Monad (Eff r e)` and `bind_eff`. */
export const succeed = <A>(x: A): Effect.Effect<A> => Effect.succeed(x);
export const flatMapEff = <A, B, E, R, E2, R2>(
  f: (a: A) => Effect.Effect<B, E2, R2>,
  eff: Effect.Effect<A, E, R>,
): Effect.Effect<B, E | E2, R | R2> => Effect.flatMap(eff, (x) => f(x));
export const mapEff = <A, B, E, R>(f: (a: A) => B, eff: Effect.Effect<A, E, R>): Effect.Effect<B, E, R> =>
  Effect.map(eff, (x) => f(x));
export const bindEff = <A, B, E, R, E2, R2>(
  eff: Effect.Effect<A, E, R>,
  f: (a: A) => Effect.Effect<B, E2, R2>,
): Effect.Effect<B, E | E2, R | R2> => Effect.flatMap(eff, (x) => f(x));

const CONTINUE = /*#__PURE__*/ tag("Loop.Step.Continue");

/** `Loop.Step s a`: the tags alone tell `Continue` from `Done`. */
export type Step<S, A> = (Tagged<string, S> & { readonly continue?: true }) | (Tagged<string, A> & { readonly continue?: false });

const isContinue = <S, A>(step: Step<S, A>): step is Tagged<string, S> & { readonly continue?: true } =>
  step._tag === CONTINUE;

/** `Loop.loop step state`: runs until `Done`, without growing the stack. */
export const loop = <S, A>(step: (s: S) => Step<S, A>, state: S): A => {
  let s = state;
  for (;;) {
    const next = step(s);
    if (!isContinue(next)) return next._0;
    s = next._0;
  }
};

/** `Trampoline a`. */
export type Trampoline<A> =
  | { readonly done: true; readonly value: A }
  | { readonly done: false; readonly thunk: (unit: undefined) => Trampoline<A> };

export const trampolineDone = <A>(value: A): Trampoline<A> => ({ done: true, value });
export const trampolineSuspend = <A>(thunk: (unit: undefined) => Trampoline<A>): Trampoline<A> => ({ done: false, thunk });
export const trampolineRun = <A>(t: Trampoline<A>): A => {
  let current = t;
  while (!current.done) current = current.thunk(undefined);
  return current.value;
};
