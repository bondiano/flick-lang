// The data ABI of compiled Flick code: Option and Result are native Effect
// values, an ADT value is `{ _tag, _0, _1, … }` with a qualified tag, a
// record is a plain object, a tuple or a list a dense array, Unit
// `undefined`. Equality is canonical: structural, SameValueZero on numbers.
import { Equal, Hash, Option, Result } from "effect";

export const some = (value) => Option.some(value);
export const none = Option.none();
export const ok = (value) => Result.succeed(value);
export const err = (error) => Result.fail(error);

const ordering = (name) => Object.freeze({ _tag: `Prelude.Ordering.${name}` });
export const LT = ordering("LT");
export const EQ = ordering("EQ");
export const GT = ordering("GT");

/** `==`: structural equality with SameValueZero on numbers. */
export const equals = (a, b) => Equal.equals(a, b);

const rank = (n) => (Number.isNaN(n) ? 1 : 0);

/**
 * The total order of `Ord`: on numbers −Infinity < finite < +Infinity < NaN,
 * signed zeros and all NaNs equal; strings by UTF-16 code units.
 */
export const compare = (a, b) => {
  if (typeof a === "number" && (Number.isNaN(a) || Number.isNaN(b))) {
    const d = rank(a) - rank(b);
    return d < 0 ? LT : d > 0 ? GT : EQ;
  }
  return a < b ? LT : a > b ? GT : EQ;
};

export const hash = (a) => Hash.hash(a);

/** `Show` of a primitive. */
export const show = (a) => {
  switch (typeof a) {
    case "string":
      return JSON.stringify(a);
    case "bigint":
      return `${a}n`;
    case "object":
      return a?._tag?.slice(a._tag.lastIndexOf(".") + 1) ?? String(a);
    default:
      return Object.is(a, -0) ? "-0" : String(a);
  }
};

export const listEquals = (eq, xs, ys) =>
  xs.length === ys.length && xs.every((x, i) => eq(x, ys[i]));

export const listCompare = (cmp, xs, ys) => {
  const n = Math.min(xs.length, ys.length);
  for (let i = 0; i < n; i++) {
    const o = cmp(xs[i], ys[i]);
    if (o._tag !== EQ._tag) return o;
  }
  return compare(xs.length, ys.length);
};

const combine = (h, x) => (Math.imul(h, 31) + x) | 0;

export const listHash = (h, xs) => xs.reduce((acc, x) => combine(acc, h(x)), 7);

export const listShow = (s, xs) => `[${xs.map((x) => s(x)).join(", ")}]`;

/** Wraps a shown argument of a constructor in parentheses when it has spaces. */
const atom = (text) => (/^[[({"]/.test(text) || !text.includes(" ") ? text : `(${text})`);

/**
 * The compiler-owned instance of class `name` for a record or a tuple:
 * `dicts` are those of its fields, by label order or by position.
 */
export const structural = (name, dicts) => {
  const parts = (value) =>
    Array.isArray(value) ? value : Object.keys(value).sort().map((k) => value[k]);
  const labels = (value) => (Array.isArray(value) ? null : Object.keys(value).sort());
  const eq = (a) => (b) => parts(a).every((x, i) => dicts[i].eq(x)(parts(b)[i]));
  const cmp = (a) => (b) => {
    const xs = parts(a);
    const ys = parts(b);
    for (let i = 0; i < xs.length; i++) {
      const o = dicts[i].compare(xs[i])(ys[i]);
      if (o._tag !== EQ._tag) return o;
    }
    return EQ;
  };
  const eqDict = { eq };
  switch (name) {
    case "Eq":
      return eqDict;
    case "Ord":
      return { compare: cmp, $Eq: structural("Eq", dicts.map((d) => d.$Eq)) };
    case "Hash":
      return {
        hash: (a) => parts(a).reduce((h, x, i) => combine(h, dicts[i].hash(x)), 11),
        $Eq: structural("Eq", dicts.map((d) => d.$Eq)),
      };
    default:
      return {
        show: (a) => {
          const shown = parts(a).map((x, i) => dicts[i].show(x));
          const ls = labels(a);
          return ls
            ? `{ ${ls.map((l, i) => `${l} = ${shown[i]}`).join(", ")} }`
            : `(${shown.join(", ")})`;
        },
      };
  }
};

/**
 * A derived instance of class `name`. `fields` gives the dictionaries of the
 * fields of all constructors in order, lazily (a recursive type needs its own
 * instance); `constructors` lists `[tag, name, arity]`, or is `null` for a
 * type without variants whose one field is its representation.
 */
export const derive = (name, fields, constructors, supers) => {
  let cache;
  const dicts = () => (cache ??= fields());
  const variant = (value) => {
    const i = constructors.findIndex(([tag]) => tag === value._tag);
    const offset = constructors.slice(0, i).reduce((n, c) => n + c[2], 0);
    return { i, offset, arity: constructors[i][2] };
  };
  const args = (value, arity) => Array.from({ length: arity }, (_, k) => value[`_${k}`]);
  const self = Object.create(supers);
  switch (name) {
    case "Eq":
      self.eq = (a) => (b) => {
        if (constructors === null) return dicts()[0].eq(a)(b);
        if (a._tag !== b._tag) return false;
        const { offset, arity } = variant(a);
        return args(a, arity).every((x, k) => dicts()[offset + k].eq(x)(b[`_${k}`]));
      };
      break;
    case "Ord":
      self.compare = (a) => (b) => {
        if (constructors === null) return dicts()[0].compare(a)(b);
        const va = variant(a);
        const vb = variant(b);
        if (va.i !== vb.i) return va.i < vb.i ? LT : GT;
        for (let k = 0; k < va.arity; k++) {
          const o = dicts()[va.offset + k].compare(a[`_${k}`])(b[`_${k}`]);
          if (o._tag !== EQ._tag) return o;
        }
        return EQ;
      };
      break;
    case "Hash":
      self.hash = (a) => {
        if (constructors === null) return dicts()[0].hash(a);
        const { i, offset, arity } = variant(a);
        return args(a, arity).reduce((h, x, k) => combine(h, dicts()[offset + k].hash(x)), i);
      };
      break;
    default:
      self.show = (a) => {
        if (constructors === null) return dicts()[0].show(a);
        const { i, offset, arity } = variant(a);
        const shown = args(a, arity).map((x, k) => atom(dicts()[offset + k].show(x)));
        return [constructors[i][1], ...shown].join(" ");
      };
  }
  return self;
};

/** `nullable`: `null` and `undefined` are `None`. */
export const fromNullable = (value) => (value === null || value === undefined ? none : some(value));

/** `optional i`: `None` is passed as `undefined`. */
export const optionalArg = (option) => (option._tag === "Some" ? option.value : undefined);
