// The model of Arbitrary: a generated value with the lazy tree of its
// shrinks. A pull is a cursor over candidates, each a sample, a discard
// (`null`) or the end (`undefined`); a pull is synchronous, as the
// functions of generators are pure.
import { item } from "../../internal.ts";
import { type GenState, randomIndex } from "./random.ts";

/** A generated value and its shrinks. */
export interface Sample { readonly value: unknown; readonly shrinks: Pull | undefined }
/** The next candidate: a sample, a discard (`null`), or `undefined` when there are no more. */
export type Pull = () => Sample | null | undefined;

/** A generator, and the compiled node of a schema, whose cost and recursion the compiler settles. */
export interface Gen {
  minCost: number;
  recursive: boolean;
  mayRecurse: boolean;
  deps: ReadonlyArray<Gen>;
  computeMinCost: () => number;
  generate: (st: GenState) => Sample | null;
}

export const makeGen = (minCost: number, generate: Gen["generate"]): Gen =>
  ({ minCost, recursive: false, mayRecurse: false, deps: [], computeMinCost: () => Infinity, generate });

/** A compiled node: its cost is settled by the compiler. */
export const compiledGen = (deps: ReadonlyArray<Gen>, computeMinCost: () => number, generate: Gen["generate"]): Gen =>
  ({ minCost: Infinity, recursive: false, mayRecurse: false, deps, computeMinCost, generate });

export const sampleOf = (value: unknown, shrinks?: Pull): Sample => ({ value, shrinks });

const done: Pull = () => undefined;

/** `f` of each of `xs` in order, made when pulled. */
export const pullEach = <T>(xs: ReadonlyArray<T>, f: (x: T) => Sample | null): Pull => {
  let i = 0;
  return () => (i < xs.length ? f(item(xs, i++)) : undefined);
};

export const concatPulls = (pulls: ReadonlyArray<Pull>): Pull => {
  let i = 0;
  return () => {
    for (; i < pulls.length; i++) {
      const s = item(pulls, i)();
      if (s !== undefined) return s;
    }
    return undefined;
  };
};

/** The shrinks made when first pulled. */
export const lazyShrinks = (value: unknown, make: () => Pull | undefined): Sample => {
  let pull: Pull | undefined;
  return sampleOf(value, () => (pull ??= make() ?? done)());
};

/** A sample whose shrinks each caller walks with its own cursor over one shared, lazily filled history. */
export interface Retained { readonly value: unknown; readonly shrinks: (() => RetainedPull) | undefined }
type RetainedPull = () => Retained | null | undefined;

export const retain = (s: Sample): Retained => {
  const source = s.shrinks;
  if (source === undefined) return { value: s.value, shrinks: undefined };
  const history: Array<Retained | null> = [];
  let ended = false;
  return {
    value: s.value,
    shrinks: () => {
      let i = 0;
      return () => {
        if (i < history.length) return item(history, i++);
        if (ended) return undefined;
        const a = source();
        if (a === undefined) {
          ended = true;
          return undefined;
        }
        const kept = a === null ? null : retain(a);
        history.push(kept);
        i++;
        return kept;
      };
    },
  };
};

/** The samples `f` makes of a cursor's candidates. */
export const mapRetained = (next: RetainedPull, f: (r: Retained) => Sample): Pull => () => {
  const r = next();
  return r === undefined || r === null ? r : f(r);
};

export const fromRetained = (r: Retained): Sample =>
  sampleOf(r.value, r.shrinks === undefined ? undefined : mapRetained(r.shrinks(), fromRetained));

/** The samples of a pull mapped, discards as they are. */
export const mapPull = (p: Pull, f: (s: Sample) => Sample): Pull => () => {
  const s = p();
  return s === undefined || s === null ? s : f(s);
};

export const mapSample = (s: Sample, f: (value: unknown) => unknown): Sample =>
  sampleOf(f(s.value), s.shrinks === undefined ? undefined : mapPull(s.shrinks, (c) => mapSample(c, f)));

export const replaceAt = <T>(xs: ReadonlyArray<T>, i: number, x: T): Array<T> => {
  const out = [...xs];
  out[i] = x;
  return out;
};

const valuesOf = (xs: ReadonlyArray<{ readonly value: unknown }>): Array<unknown> => xs.map((x) => x.value);

/** Shrinks each child in turn, never an earlier one again. */
export const productSample = (children: ReadonlyArray<Sample>, make: (values: Array<unknown>) => unknown): Sample => {
  const value = make(valuesOf(children));
  if (!children.some((c) => c.shrinks !== undefined)) return sampleOf(value);
  const rebuild = (retained: ReadonlyArray<Retained>, start: number): Sample => {
    const pulls: Array<Pull> = [];
    for (let i = start; i < retained.length; i++) {
      const shrinks = item(retained, i).shrinks;
      if (shrinks !== undefined) pulls.push(mapRetained(shrinks(), (r) => rebuild(replaceAt(retained, i, r), i)));
    }
    return sampleOf(make(valuesOf(retained)), pulls.length > 0 ? concatPulls(pulls) : undefined);
  };
  return lazyShrinks(value, () => rebuild(children.map(retain), 0).shrinks);
};

/** The value `f` keeps of a candidate, or `NONE`. */
export const NONE: unique symbol = /*#__PURE__*/ Symbol("none");
type Kept = (value: unknown) => unknown;

/** The candidates `f` keeps; a rejected one is a discard whose own shrinks are tried next. */
const filterMapPull = (source: Pull, f: Kept): Pull => {
  const stack = [source];
  return () => {
    while (stack.length > 0) {
      const s = item(stack, stack.length - 1)();
      if (s === undefined) {
        stack.pop();
        continue;
      }
      if (s === null) return null;
      const kept = f(s.value);
      if (kept !== NONE) return sampleOf(kept, s.shrinks === undefined ? undefined : filterMapPull(s.shrinks, f));
      if (s.shrinks !== undefined) stack.push(s.shrinks);
      return null;
    }
    return undefined;
  };
};

export const filterMapSample = (s: Sample, f: Kept): Sample | null => {
  const kept = f(s.value);
  if (kept === NONE) return null;
  return sampleOf(kept, s.shrinks === undefined ? undefined : filterMapPull(s.shrinks, f));
};

export const filterSample = (s: Sample, keep: (value: unknown) => boolean): Sample | null => {
  if (!keep(s.value)) return null;
  if (s.shrinks === undefined) return s;
  return sampleOf(s.value, filterMapPull(s.shrinks, (v) => (keep(v) ? v : NONE)));
};

/** The candidates of `shrink` for each value. */
export const shrinkSample = <T>(value: T, shrink: (value: T) => ReadonlyArray<T>): Sample => {
  const candidates = shrink(value);
  return sampleOf(value, candidates.length === 0 ? undefined : pullEach(candidates, (c) => shrinkSample(c, shrink)));
};

/** `child` generated with `reserved` of the budget kept for the generators after it. */
export const reserving = (child: Gen, st: GenState, reserved: number): Sample | null => {
  if (child.minCost + reserved > st.budget.remaining) return null;
  if (reserved === 0) return child.generate(st);
  st.budget.remaining -= reserved;
  const s = child.generate(st);
  st.budget.remaining += reserved;
  return s;
};

/** Each child generated in `order`; `undefined` when one is discarded. */
export const generateProduct = (children: ReadonlyArray<Gen>, st: GenState, order: ReadonlyArray<number> | undefined,
  reserved: number): Array<Sample> | undefined => {
  if (reserved > st.budget.remaining) return undefined;
  const out = new Array<Sample>(children.length);
  for (let i = 0; i < children.length; i++) {
    const at = order === undefined ? i : item(order, i);
    const child = item(children, at);
    reserved -= child.minCost;
    const s = reserving(child, st, reserved);
    if (s === null) return undefined;
    out[at] = s;
  }
  return out;
};

/** An affordable member; a value of a costlier one first shrinks to a value of the cheapest member. */
export const generateUnion = (members: ReadonlyArray<Gen>, st: GenState): Sample | null => {
  const eligible = members.filter((m) => m.minCost <= st.budget.remaining);
  if (eligible.length === 0) return null;
  const selected = item(eligible, randomIndex(st, eligible.length));
  const s = selected.generate(st);
  if (!st.shrinks || s === null) return s;
  let fallback = item(members, 0);
  for (const m of members) if (m.minCost < fallback.minCost) fallback = m;
  if (fallback.minCost >= selected.minCost) return s;
  const random = st.random.clone();
  let pulled = false;
  const fallbackPull: Pull = () => {
    if (pulled) return undefined;
    pulled = true;
    return fallback.generate({ ...st, random, budget: { remaining: fallback.minCost } });
  };
  return sampleOf(s.value, s.shrinks === undefined ? fallbackPull : concatPulls([fallbackPull, s.shrinks]));
};

/** Where the parts of a generated array are. */
export interface ArrayShape {
  readonly fixed: number;
  readonly optional: number;
  readonly repeat: number;
  readonly tail: number;
  readonly minimum: number;
}

/** Removes optional elements, then runs of repeated ones, then shrinks each element from `start` on. */
const rebuildArray = (children: ReadonlyArray<Retained>, shape: ArrayShape, start: number): Sample => {
  let index = 0;
  let removable: number | undefined;
  let items: RetainedPull | undefined;
  let removableOptional = shape.repeat === 0 && shape.tail === 0 ? Math.min(shape.optional, children.length - shape.minimum) : 0;
  const repeatEnd = shape.fixed + shape.repeat;
  const next: Pull = () => {
    if (removableOptional > 0) {
      const count = removableOptional;
      removableOptional = Math.floor(removableOptional / 2);
      return rebuildArray(children.slice(0, -count), { ...shape, fixed: shape.fixed - count, optional: shape.optional - count }, 0);
    }
    while (index < children.length) {
      removable ??= index >= shape.fixed && index < repeatEnd ? Math.min(repeatEnd - index, children.length - shape.minimum) : 0;
      if (removable > 0) {
        const count = removable;
        removable = Math.floor(removable / 2);
        return rebuildArray([...children.slice(0, index), ...children.slice(index + count)],
          { ...shape, repeat: shape.repeat - count }, 0);
      }
      if (index >= start) items ??= item(children, index).shrinks?.();
      if (items !== undefined) {
        const r = items();
        if (r === undefined) {
          items = undefined;
          removable = undefined;
          index++;
          continue;
        }
        if (r === null) return null;
        return rebuildArray(replaceAt(children, index, r), shape, index);
      }
      index++;
      removable = undefined;
    }
    return undefined;
  };
  return sampleOf(valuesOf(children), next);
};

/** A list of the children's values. */
export const arraySample = (children: ReadonlyArray<Sample>, shape: ArrayShape, shrinks: boolean): Sample => {
  const value = valuesOf(children);
  if (!shrinks) return sampleOf(value);
  if (children.length <= shape.minimum || (shape.repeat === 0 && shape.optional === 0)) return productSample(children, (vs) => vs);
  return lazyShrinks(value, () => rebuildArray(children.map(retain), shape, 0).shrinks);
};

export const sumCosts = (costs: ReadonlyArray<number>): number => {
  let out = 0;
  for (const c of costs) {
    if (c === Infinity) return Infinity;
    out += c;
  }
  return out;
};
