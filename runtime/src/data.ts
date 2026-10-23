// The data ABI of compiled Flick code: Option and Result are native Effect
// values, an ADT value is `{ _tag, _0, _1, … }` with a qualified tag, a
// record is a plain object, a tuple or a list a dense array, Unit
// `undefined`. Equality is canonical: structural, SameValueZero on numbers.
import { Equal, Hash, Option, Result } from "effect";

import { item } from "./internal.ts";

/** A value of a variant type: its qualified constructor id and positional fields. */
export interface Data {
  readonly _tag: string;
  readonly [field: `_${number}`]: unknown;
}

/** A value of a variant type with one field `_0`. */
export interface Tagged<T extends string, A> {
  readonly _tag: T;
  readonly _0: A;
}

/** `Prelude.Ordering`. */
export type Ordering = Readonly<{ _tag: `Prelude.Ordering.${"LT" | "EQ" | "GT"}` }>;

/** Dictionaries of the built-in classes, curried as generated code calls them. */
export interface EqDict<A> {
  readonly eq: (a: A) => (b: A) => boolean;
}
export interface OrdDict<A> {
  readonly compare: (a: A) => (b: A) => Ordering;
  readonly $Eq: EqDict<A>;
}
export interface HashDict<A> {
  readonly hash: (a: A) => number;
  readonly $Eq: EqDict<A>;
}
export interface ShowDict<A> {
  readonly show: (a: A) => string;
}

export const some = <A>(value: A): Option.Option<A> => Option.some(value);
export const none: Option.Option<never> = Option.none();
export const ok = <A>(value: A): Result.Result<A> => Result.succeed(value);
export const err = <E>(error: E): Result.Result<never, E> => Result.fail(error);

const ordering = (name: "LT" | "EQ" | "GT"): Ordering => Object.freeze({ _tag: `Prelude.Ordering.${name}` as const });
export const LT = ordering("LT");
export const EQ = ordering("EQ");
export const GT = ordering("GT");

/** `==`: structural equality with SameValueZero on numbers. */
export const equals = (a: unknown, b: unknown): boolean => Equal.equals(a, b);

const rank = (n: number): number => (Number.isNaN(n) ? 1 : 0);

/**
 * The total order of `Ord`: on numbers −Infinity < finite < +Infinity < NaN,
 * signed zeros and all NaNs equal; strings by UTF-16 code units.
 */
export const compare = <A extends number | string | bigint | boolean>(a: A, b: A): Ordering => {
  if (typeof a === "number" && typeof b === "number" && (Number.isNaN(a) || Number.isNaN(b))) {
    const d = rank(a) - rank(b);
    return d < 0 ? LT : d > 0 ? GT : EQ;
  }
  return a < b ? LT : a > b ? GT : EQ;
};

export const hash = (a: unknown): number => Hash.hash(a);

const tagOf = (a: object): string | undefined => {
  const tag: unknown = Reflect.get(a, "_tag");
  return typeof tag === "string" ? tag : undefined;
};

/** `Show` of a primitive. */
export const show = (a: unknown): string => {
  switch (typeof a) {
    case "string":
      return JSON.stringify(a);
    case "bigint":
      return `${a}n`;
    case "object": {
      if (a === null) return "null";
      const tag = tagOf(a);
      return tag === undefined ? Object.prototype.toString.call(a) : tag.slice(tag.lastIndexOf(".") + 1);
    }
    default:
      return Object.is(a, -0) ? "-0" : String(a);
  }
};

export const listEquals = <A>(eq: (a: A, b: A) => boolean, xs: ReadonlyArray<A>, ys: ReadonlyArray<A>): boolean =>
  xs.length === ys.length && xs.every((x, i) => eq(x, item(ys, i)));

export const listCompare = <A>(
  cmp: (a: A, b: A) => Ordering,
  xs: ReadonlyArray<A>,
  ys: ReadonlyArray<A>,
): Ordering => {
  const n = Math.min(xs.length, ys.length);
  for (let i = 0; i < n; i++) {
    const o = cmp(item(xs, i), item(ys, i));
    if (o._tag !== EQ._tag) return o;
  }
  return compare(xs.length, ys.length);
};

const combine = (h: number, x: number): number => (Math.imul(h, 31) + x) | 0;

export const listHash = <A>(h: (a: A) => number, xs: ReadonlyArray<A>): number =>
  xs.reduce((acc, x) => combine(acc, h(x)), 7);

export const listShow = <A>(s: (a: A) => string, xs: ReadonlyArray<A>): string => `[${xs.map((x) => s(x)).join(", ")}]`;

/** Wraps a shown argument of a constructor in parentheses when it has spaces. */
const atom = (text: string): string => (/^[[({"]/.test(text) || !text.includes(" ") ? text : `(${text})`);

/** A record or a tuple, as `structural` sees it. */
type Structure = Readonly<Record<string, unknown>> | ReadonlyArray<unknown>;

/** The dictionaries of the fields of a structure, of every class at once. */
interface FieldDict {
  readonly eq: (a: unknown) => (b: unknown) => boolean;
  readonly compare: (a: unknown) => (b: unknown) => Ordering;
  readonly hash: (a: unknown) => number;
  readonly show: (a: unknown) => string;
  readonly $Eq: FieldDict;
}

type Dict = Partial<Record<"eq" | "compare" | "hash" | "show" | "$Eq", unknown>>;

const isTuple = (value: Structure): value is ReadonlyArray<unknown> => Array.isArray(value);

const fieldsOf = (value: Structure): ReadonlyArray<unknown> =>
  isTuple(value) ? value : Object.keys(value).toSorted().map((k) => value[k]);

const labelsOf = (value: Structure): ReadonlyArray<string> | null =>
  isTuple(value) ? null : Object.keys(value).toSorted();

const at = item;

/**
 * The compiler-owned instance of class `name` for a record or a tuple:
 * `dicts` are those of its fields, by label order or by position.
 */
export const structural = (name: string, dicts: ReadonlyArray<FieldDict>): Dict => {
  const eq = (a: Structure) => (b: Structure) =>
    fieldsOf(a).every((x, i) => at(dicts, i).eq(x)(at(fieldsOf(b), i)));
  const cmp = (a: Structure) => (b: Structure) => {
    const xs = fieldsOf(a);
    const ys = fieldsOf(b);
    for (let i = 0; i < xs.length; i++) {
      const o = at(dicts, i).compare(xs[i])(ys[i]);
      if (o._tag !== EQ._tag) return o;
    }
    return EQ;
  };
  const supers = (): Dict => structural("Eq", dicts.map((d) => d.$Eq));
  switch (name) {
    case "Eq":
      return { eq };
    case "Ord":
      return { compare: cmp, $Eq: supers() };
    case "Hash":
      return {
        hash: (a: Structure) => fieldsOf(a).reduce<number>((h, x, i) => combine(h, at(dicts, i).hash(x)), 11),
        $Eq: supers(),
      };
    default:
      return {
        show: (a: Structure) => {
          const shown = fieldsOf(a).map((x, i) => at(dicts, i).show(x));
          const labels = labelsOf(a);
          return labels
            ? `{ ${labels.map((l, i) => `${l} = ${at(shown, i)}`).join(", ")} }`
            : `(${shown.join(", ")})`;
        },
      };
  }
};

/** A constructor of a derived type: its qualified id, its name and arity. */
type ConstructorInfo = readonly [tag: string, name: string, arity: number];

const field = (value: Data, k: number): unknown => value[`_${k}`];

const argsOf = (value: Data, arity: number): Array<unknown> => Array.from({ length: arity }, (_, k) => field(value, k));

/**
 * A derived instance of class `name`. `fields` gives the dictionaries of the
 * fields of all constructors in order, lazily (a recursive type needs its own
 * instance); `constructors` lists `[tag, name, arity]`, or is `null` for a
 * type without variants whose one field is its representation.
 */
export const derive = (
  name: string,
  fields: () => ReadonlyArray<FieldDict>,
  constructors: ReadonlyArray<ConstructorInfo> | null,
  supers: object | null,
): Dict => {
  let cache: ReadonlyArray<FieldDict> | undefined;
  const dicts = (): ReadonlyArray<FieldDict> => (cache ??= fields());
  const dict = (k: number): FieldDict => at(dicts(), k);
  const variant = (all: ReadonlyArray<ConstructorInfo>, value: Data) => {
    const i = all.findIndex(([tag]) => tag === value._tag);
    const offset = all.slice(0, i).reduce((n, c) => n + c[2], 0);
    return { i, offset, arity: at(all, i)[2] };
  };
  const args = argsOf;
  const self: Dict = {};
  // the instance's superclass dictionaries are its prototype, as generated code reads them
  Object.setPrototypeOf(self, supers);
  switch (name) {
    case "Eq":
      self.eq = (a: Data) => (b: Data) => {
        if (constructors === null) return dict(0).eq(a)(b);
        if (a._tag !== b._tag) return false;
        const { offset, arity } = variant(constructors, a);
        return args(a, arity).every((x, k) => dict(offset + k).eq(x)(field(b, k)));
      };
      break;
    case "Ord":
      self.compare = (a: Data) => (b: Data) => {
        if (constructors === null) return dict(0).compare(a)(b);
        const va = variant(constructors, a);
        const vb = variant(constructors, b);
        if (va.i !== vb.i) return va.i < vb.i ? LT : GT;
        for (let k = 0; k < va.arity; k++) {
          const o = dict(va.offset + k).compare(field(a, k))(field(b, k));
          if (o._tag !== EQ._tag) return o;
        }
        return EQ;
      };
      break;
    case "Hash":
      self.hash = (a: Data) => {
        if (constructors === null) return dict(0).hash(a);
        const { i, offset, arity } = variant(constructors, a);
        return args(a, arity).reduce<number>((h, x, k) => combine(h, dict(offset + k).hash(x)), i);
      };
      break;
    default:
      self.show = (a: Data) => {
        if (constructors === null) return dict(0).show(a);
        const { i, offset, arity } = variant(constructors, a);
        const shown = args(a, arity).map((x, k) => atom(dict(offset + k).show(x)));
        return [at(constructors, i)[1], ...shown].join(" ");
      };
  }
  return self;
};

/** `nullable`: `null` and `undefined` are `None`. */
export const fromNullable = <A>(value: A | null | undefined): Option.Option<A> =>
  value === null || value === undefined ? none : some(value);

/** `optional i`: `None` is passed as `undefined`. */
export const optionalArg = <A>(option: Option.Option<A>): A | undefined =>
  Option.isSome(option) ? option.value : undefined;
