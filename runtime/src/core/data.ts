// The data ABI of compiled Flick code without Effect: an ADT value is
// `{ _tag, _0, _1, … }` with a qualified tag, a record is a plain object, a
// tuple or a list a dense array, Unit `undefined`. Option and Result have
// the fields of Effect's (`{ _tag: "Some", value }`, `{ _tag: "Success",
// success }`, …) as plain objects, so compiled matches read either.
// Equality is canonical: structural, SameValueZero on numbers.
import { item } from "../internal.ts";
import { constructorName, tag } from "../tags.ts";

export type Option<A> = Readonly<{ _tag: "None" }> | Readonly<{ _tag: "Some"; value: A }>;
export type Result<A, E = never> = Readonly<{ _tag: "Success"; success: A }> | Readonly<{ _tag: "Failure"; failure: E }>;

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
export type Ordering = Readonly<{ _tag: string }>;

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

export const some = <A>(value: A): Option<A> => ({ _tag: "Some", value });
export const none: Option<never> = /*#__PURE__*/ Object.freeze({ _tag: "None" });
export const ok = <A>(success: A): Result<A> => ({ _tag: "Success", success });
export const err = <E>(failure: E): Result<never, E> => ({ _tag: "Failure", failure });

const ordering = (name: "LT" | "EQ" | "GT"): Ordering => Object.freeze({ _tag: tag(`Prelude.Ordering.${name}`) });
export const LT = /*#__PURE__*/ ordering("LT");
export const EQ = /*#__PURE__*/ ordering("EQ");
export const GT = /*#__PURE__*/ ordering("GT");

const isObject = (a: unknown): a is Readonly<Record<string, unknown>> => typeof a === "object" && a !== null;

/** A key `$0`, `$1` …: a dictionary a value of a GADT holds, which `==` and `hash` leave out. */
const held = (k: string): boolean => k.charCodeAt(0) === 36;

/** `==`: structural equality with SameValueZero on numbers. */
export const equals = (a: unknown, b: unknown): boolean => {
  // oxlint-disable-next-line no-self-compare -- SameValueZero: NaN equals NaN
  if (a === b || (a !== a && b !== b)) return true;
  if (!isObject(a) || !isObject(b)) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && listEquals(equals, a, b);
  }
  // shortcut: plain objects and arrays only; a Map, Set or Date compares by identity until Flick has one without Effect
  if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length && keys.every((k) => held(k) || (Object.hasOwn(b, k) && equals(a[k], b[k])))
  );
};

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

const hashString = (s: string): number => {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = (Math.imul(h, 33) ^ s.charCodeAt(i)) | 0;
  return h;
};

/** A hash consistent with `equals`. */
export const hash = (a: unknown): number => {
  if (typeof a === "number") return hashString(Object.is(a, -0) ? "0" : String(a));
  if (!isObject(a)) return hashString(`${typeof a}:${String(a)}`);
  if (Array.isArray(a)) return listHash(hash, a);
  return Object.keys(a)
    .toSorted()
    .reduce((h, k) => (held(k) ? h : combine(combine(h, hashString(k)), hash(a[k]))), 13);
};

const tagOf = (a: object): string | undefined => {
  const t: unknown = Reflect.get(a, "_tag");
  return typeof t === "string" ? t : undefined;
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
      const t = tagOf(a);
      return t === undefined ? Object.prototype.toString.call(a) : constructorName(t);
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
export interface FieldDict {
  readonly eq: (a: unknown) => (b: unknown) => boolean;
  readonly compare: (a: unknown) => (b: unknown) => Ordering;
  readonly hash: (a: unknown) => number;
  readonly show: (a: unknown) => string;
  readonly parse: (text: string) => unknown;
  readonly $Eq: FieldDict;
}

export type Dict = Partial<Record<"eq" | "compare" | "hash" | "show" | "parse" | "$Eq", unknown>>;

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

/**
 * A constructor of a derived type: its qualified id, its name and arity.
 * `rep`, when present: `"unboxed"`, the value is the one field; the JS type
 * that tells an `untagged` constructor's field apart; or `"literal"`, the
 * value is the enum's `literal` and has no fields.
 */
type ConstructorInfo = readonly [
  tag: string,
  name: string,
  arity: number,
  rep?: string,
  literal?: string | number,
];

const field = (value: Data, k: number): unknown => value[`_${k}`];

const argsOf = (value: Data, arity: number): Array<unknown> => Array.from({ length: arity }, (_, k) => field(value, k));

/** The JS type a value of an `untagged` type is told apart by. */
const jsClass = (value: unknown): string => (Array.isArray(value) ? "array" : typeof value);

const isData = (value: unknown): value is Data => typeof value === "object" && value !== null && "_tag" in value;

/** Whether `value` is of constructor `c`. */
const isOf = (c: ConstructorInfo, value: unknown): boolean => {
  const rep = c[3];
  if (rep === undefined) return isData(value) && value._tag === c[0];
  if (rep === "literal") return value === c[4];
  return rep === "unboxed" || rep === jsClass(value);
};

/** The fields of `value`, a value of constructor `c`. */
const partsOf = (c: ConstructorInfo, value: unknown): Array<unknown> => {
  if (c[3] === "literal") return [];
  if (c[3] !== undefined) return [value];
  return isData(value) ? argsOf(value, c[2]) : [];
};

/**
 * A derived instance of class `name`. `fields` gives the dictionaries of the
 * fields of all constructors in order, lazily (a recursive type needs its own
 * instance); `constructors` lists `[tag, name, arity]`, or is `null` for a
 * type without variants whose one field is its representation. `Cli.Value`,
 * which parses with an effect, is derived by the Effect runtime.
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
  const variant = (all: ReadonlyArray<ConstructorInfo>, value: unknown) => {
    const i = all.findIndex((c) => isOf(c, value));
    const offset = all.slice(0, i).reduce((n, c) => n + c[2], 0);
    return { i, offset, arity: at(all, i)[2] };
  };
  const args = (value: unknown): Array<unknown> => {
    const all = constructors ?? [];
    return partsOf(at(all, variant(all, value).i), value);
  };
  const self: Dict = {};
  // the instance's superclass dictionaries are its prototype, as generated code reads them
  Object.setPrototypeOf(self, supers);
  switch (name) {
    case "Eq":
      self.eq = (a: unknown) => (b: unknown) => {
        if (constructors === null) return dict(0).eq(a)(b);
        const { i, offset } = variant(constructors, a);
        if (i !== variant(constructors, b).i) return false;
        const right = args(b);
        return args(a).every((x, k) => dict(offset + k).eq(x)(right[k]));
      };
      break;
    case "Ord":
      self.compare = (a: unknown) => (b: unknown) => {
        if (constructors === null) return dict(0).compare(a)(b);
        const va = variant(constructors, a);
        const vb = variant(constructors, b);
        if (va.i !== vb.i) return va.i < vb.i ? LT : GT;
        const [left, right] = [args(a), args(b)];
        for (let k = 0; k < va.arity; k++) {
          const o = dict(va.offset + k).compare(left[k])(right[k]);
          if (o._tag !== EQ._tag) return o;
        }
        return EQ;
      };
      break;
    case "Hash":
      self.hash = (a: unknown) => {
        if (constructors === null) return dict(0).hash(a);
        const { i, offset } = variant(constructors, a);
        return args(a).reduce<number>((h, x, k) => combine(h, dict(offset + k).hash(x)), i);
      };
      break;
    default:
      self.show = (a: unknown) => {
        if (constructors === null) return dict(0).show(a);
        const { i, offset } = variant(constructors, a);
        const shown = args(a).map((x, k) => atom(dict(offset + k).show(x)));
        return [at(constructors, i)[1], ...shown].join(" ");
      };
  }
  return self;
};

/** `nullable`: `null` and `undefined` are `None`. */
export const fromNullable = <A>(value: A | null | undefined): Option<A> =>
  value === null || value === undefined ? none : some(value);

/** `optional i`: `None` is passed as `undefined`. */
export const optionalArg = <A>(option: Option<A>): A | undefined => (option._tag === "Some" ? option.value : undefined);

/**
 * `value` as Flick data JS can no longer change: arrays and plain objects
 * are copied all the way down, and any other object goes through `other`
 * (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`). A frozen object is a constant such as
 * `none`, kept with its identity.
 * shortcut: a plain JS object typed `JsValue` inside data is copied too; type the copy if a binding needs its identity.
 */
export const copyThrough = (value: unknown, other: (value: object) => unknown): unknown => {
  if (typeof value !== "object" || value === null) return value;
  if (Array.isArray(value)) return value.map((x: unknown) => copyThrough(x, other));
  const proto: unknown = Object.getPrototypeOf(value);
  if ((proto !== Object.prototype && proto !== null) || Object.isFrozen(value)) return other(value);
  const copy: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(value)) copy[k] = copyThrough(v, other);
  return copy;
};

/** Flick data that crosses the boundary of an import outside the runtime. */
export const copyData = <A>(value: A): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a copy has the shape of what it copies
  copyThrough(value, (x) => x) as A;
