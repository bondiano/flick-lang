// The data ABI of compiled Flick code without Effect: an ADT value is
// `{ _tag, _0, _1, … }` with a qualified tag, a record is a plain object, a
// tuple or a list a dense array, Unit `undefined`. Option and Result have
// the same fields as those of the Effect library (`{ _tag: "Some", value }`,
// `{ _tag: "Success", success }`, …) as plain objects, so compiled matches
// read either.
// Equality is canonical: structural, SameValueZero on numbers.
import { item } from "../internal.ts";
import { constructorName } from "../tags.ts";
import {
  type Data,
  EQ,
  GT,
  LT,
  type Ordering,
  none,
  runtimeEquals,
  runtimeHash,
  runtimeSequence,
  some,
} from "./values.ts";

export * from "./values.ts";

const isObject = (a: unknown): a is Readonly<Record<PropertyKey, unknown>> => typeof a === "object" && a !== null;
const has = Object.hasOwn;
const keys = Object.keys;

/** A key `$0`, `$1` …: a dictionary a value of a GADT holds, which `==` and `hash` leave out. */
const ownKeys = (value: Readonly<Record<PropertyKey, unknown>>): Array<string> =>
  keys(value).filter((k) => k.charCodeAt(0) !== 36);

/**
 * Returns the payload field name of an Option or Result (or `""` for `None`),
 * handling both Flick's own-property `_tag` and Effect's prototype `_tag`.
 */
const builtinField = (value: Readonly<Record<string, unknown>>): string | undefined => {
  const v = value["_tag"];
  const k = v === "Some" ? "value" : v === "Success" ? "success" : v === "Failure" ? "failure" : v === "None" ? "" : undefined;
  if (k === undefined) return undefined;
  const n = keys(value).length - (has(value, "_tag") ? 1 : 0);
  return k ? (n === 1 && has(value, k) ? k : undefined) : n === 0 ? "" : undefined;
};

const eqValues = (a: unknown, b: unknown, strictZero: boolean): boolean => {
  // oxlint-disable-next-line no-self-compare -- SameValueZero: NaN equals NaN
  if (strictZero ? Object.is(a, b) : a === b || (a !== a && b !== b)) return true;
  if (!isObject(a) || !isObject(b)) return false;
  const custom = a[runtimeEquals];
  if (typeof custom === "function") return custom.call(a, b) === true;
  const aKey = builtinField(a);
  const bKey = builtinField(b);
  if (aKey !== undefined && bKey !== undefined) {
    return aKey === bKey && (!aKey || eqValues(a[aKey], b[aKey], strictZero));
  }
  const hostEqual = a["~effect/Equal"];
  if (typeof hostEqual === "function" && typeof b["~effect/Equal"] === "function") {
    return hostEqual.call(a, b) === true;
  }
  // A flat chunk can compare with a tree through the tree's equality hook.
  if (Array.isArray(a)) {
    return Array.isArray(b)
      ? a.length === b.length && a.every((x, i) => eqValues(x, b[i], strictZero))
      : runtimeSequence in b && eqValues(b, a, strictZero);
  }
  if (Array.isArray(b)) return false;
  // Class instances without a Flick equality hook compare by identity.
  const proto: unknown = Object.getPrototypeOf(a);
  if (proto !== Object.getPrototypeOf(b) || (proto !== Object.prototype && proto !== null)) return false;
  const ak = ownKeys(a);
  return ak.length === ownKeys(b).length && ak.every((k) => has(b, k) && eqValues(a[k], b[k], strictZero));
};

/** `==`: structural equality with SameValueZero on numbers. */
export const equals = (a: unknown, b: unknown): boolean => eqValues(a, b, false);

/**
 * Structural SameValue equivalence: distinguishes `+0` and `-0`, treats `NaN`
 * as equal to `NaN`, preserves identity for mutable cells, functions, and
 * foreign class instances, and compares plain records, tuples, lists, Option,
 * Result, and ADTs structurally without observing data-value identity or
 * GADT held dictionary keys.
 */
export const sameValue = (a: unknown, b: unknown): boolean => eqValues(a, b, true);

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

/** Stable reference-identity hashes for non-plain class instances (mutable cells, foreign handles). */
const identityHashes = /*#__PURE__*/ new WeakMap<object, number>();
let nextIdentityHash = 1;

const identityHash = (value: object): number => {
  let code = identityHashes.get(value);
  if (code === undefined) {
    code = Math.imul(nextIdentityHash++, 2654435761) | 0;
    identityHashes.set(value, code);
  }
  return code;
};

/** A hash consistent with `equals`. */
export const hash = (a: unknown): number => {
  if (typeof a === "number") return hashString(Object.is(a, -0) ? "0" : String(a));
  if (!isObject(a)) return hashString(`${typeof a}:${String(a)}`);
  const custom = a[runtimeHash];
  if (typeof custom === "function") {
    const value: unknown = custom.call(a);
    if (typeof value === "number") return value;
  }
  const key = builtinField(a);
  if (key !== undefined) {
    if (key === "" || key === "value") {
      if (!has(a, "_tag")) return hash(key ? some(a[key]) : none);
    } else {
      return combine(
        combine(13, hashString(key === "success" ? "Success" : "Failure")),
        hash(a[key]),
      );
    }
  }
  const hostHash = a["~effect/Hash"];
  if (typeof hostHash === "function") {
    const value: unknown = hostHash.call(a);
    if (typeof value === "number") return value;
  }
  if (Array.isArray(a)) return listHash(hash, a);
  const proto: unknown = Object.getPrototypeOf(a);
  if (proto !== Object.prototype && proto !== null) return identityHash(a);
  return ownKeys(a)
    .toSorted()
    .reduce((h, k) => combine(combine(h, hashString(k)), hash(a[k])), 13);
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

const listHashSeed = 7;

export const listHash = <A>(h: (a: A) => number, xs: ReadonlyArray<A>): number =>
  xs.reduce((acc, x) => combine(acc, h(x)), listHashSeed);

export const iterableHash = <A>(h: (a: A) => number, xs: Iterable<A>): number => {
  let result = listHashSeed;
  for (const x of xs) result = combine(result, h(x));
  return result;
};

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
      return {
        compare: cmp,
        get $Eq() {
          return supers();
        },
      };
    case "Hash":
      return {
        hash: (a: Structure) => fieldsOf(a).reduce<number>((h, x, i) => combine(h, at(dicts, i).hash(x)), 11),
        get $Eq() {
          return supers();
        },
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
