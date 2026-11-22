// The data ABI of compiled Flick code on Effect: Option and Result are
// native Effect values; everything else is the data of `core/data.ts`,
// which compiled code without Effect uses alone.
import { Equal, Hash, Option, Result } from "effect";

import { derive as deriveData, gadtMarker, runtimeEquals, runtimeHash } from "@flick-lang/runtime/core/data";

export {
  type Data,
  type EqDict,
  EQ,
  GT,
  type HashDict,
  LT,
  type OrdDict,
  type Ordering,
  type ShowDict,
  type Tagged,
  compare,
  listCompare,
  listEquals,
  listHash,
  listShow,
  show,
  structural,
} from "@flick-lang/runtime/core/data";

export const some = <A>(value: A): Option.Option<A> => Option.some(value);
export const none: Option.Option<never> = /*#__PURE__*/ Option.none();
export const ok = <A>(value: A): Result.Result<A> => Result.succeed(value);
export const err = <E>(error: E): Result.Result<never, E> => Result.fail(error);

const held = (k: string): boolean => k.charCodeAt(0) === 36;
const visibleKeys = (o: Readonly<Record<string, unknown>>): Array<string> =>
  Object.keys(o).filter((k) => !held(k));
const isPlainOrGadt = (v: unknown): v is Readonly<Record<string, unknown>> => {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return false;
  const proto: unknown = Object.getPrototypeOf(v);
  return proto === Object.prototype || proto === null || gadtMarker in v;
};
const hashObject = (o: Readonly<Record<string, unknown>>): number => {
  let h = 12289;
  for (const k of visibleKeys(o)) h ^= Hash.combine(Hash.string(k), hash(o[k]));
  return Hash.optimize(h);
};
const hashBox = (value: unknown): Hash.Hash => ({ [Hash.symbol]: () => hash(value) });

const eqValues = (a: unknown, b: unknown, strictZero: boolean): boolean => {
  // oxlint-disable-next-line no-self-compare -- SameValueZero: NaN equals NaN
  if (strictZero ? Object.is(a, b) : a === b || (a !== a && b !== b)) return true;
  if (typeof a !== "object" || a === null || typeof b !== "object" || b === null) return false;
  if (Array.isArray(a)) {
    return Array.isArray(b) && a.length === b.length && a.every((x, i) => eqValues(x, b[i], strictZero));
  }
  if (Array.isArray(b)) return false;
  if (Option.isOption(a) && Option.isOption(b)) {
    return Option.isNone(a) ? Option.isNone(b) : Option.isSome(b) && eqValues(a.value, b.value, strictZero);
  }
  if (Result.isResult(a) && Result.isResult(b)) {
    return Result.isSuccess(a)
      ? Result.isSuccess(b) && eqValues(a.success, b.success, strictZero)
      : Result.isFailure(b) && eqValues(a.failure, b.failure, strictZero);
  }
  const plainA = isPlainOrGadt(a);
  const plainB = isPlainOrGadt(b);
  if (plainA && plainB) {
    const ks = visibleKeys(a);
    return ks.length === visibleKeys(b).length && ks.every((k) => Object.hasOwn(b, k) && eqValues(a[k], b[k], strictZero));
  }
  if (plainA || plainB) return false;
  if (strictZero && (!Equal.isEqual(a) || !Equal.isEqual(b))) return false;
  return Equal.equals(a, b);
};

/** `==`: structural equality with SameValueZero on numbers, ignoring GADT `$`-prefixed dictionaries. */
export const equals = (a: unknown, b: unknown): boolean => eqValues(a, b, false);

/** Structural SameValue equivalence: distinguishes `+0` and `-0`, ignores GADT `$`-prefixed dictionaries. */
export const sameValue = (a: unknown, b: unknown): boolean => eqValues(a, b, true);

export const hash = (a: unknown): number => {
  if (typeof a !== "object" || a === null) return Hash.hash(a);
  if (Array.isArray(a)) {
    let h = 6151;
    for (const x of a) h = Hash.combine(h, hash(x));
    return Hash.optimize(h);
  }
  if (isPlainOrGadt(a)) return hashObject(a);
  if (Option.isOption(a)) return Option.isSome(a) ? Hash.hash(Option.some(hashBox(a.value))) : Hash.hash(a);
  if (Result.isResult(a)) {
    return Result.isSuccess(a)
      ? Hash.hash(Result.succeed(hashBox(a.success)))
      : Hash.hash(Result.fail(hashBox(a.failure)));
  }
  return Hash.hash(a);
};

const GADT_PROTO: Record<PropertyKey, unknown> = /*#__PURE__*/ Object.freeze({
  [gadtMarker]: true,
  [Equal.symbol](this: unknown, that: unknown): boolean {
    return eqValues(this, that, false);
  },
  [Hash.symbol](this: unknown): number {
    return hash(this);
  },
  [runtimeEquals](this: unknown, that: unknown, strictZero = false): boolean {
    return eqValues(this, that, strictZero);
  },
  [runtimeHash](this: unknown): number {
    return hash(this);
  },
});

/** Marks a GADT value holding `$0`, `$1`, … dictionaries so Effect `Equal`/`Hash` skip them. */
export const gadt = <A extends object>(value: A): A => {
  Object.setPrototypeOf(value, GADT_PROTO);
  return value;
};

/** A derived instance of class `name`, as `core/data.ts` derives it. */
export const derive = deriveData;

/** `nullable`: `null` and `undefined` are `None`. */
export const fromNullable = <A>(value: A | null | undefined): Option.Option<A> =>
  value === null || value === undefined ? none : some(value);

/** `optional i`: `None` is passed as `undefined`. */
export const optionalArg = <A>(option: Option.Option<A>): A | undefined =>
  Option.isSome(option) ? option.value : undefined;

const putOwn = (target: object, key: PropertyKey, value: unknown): void => {
  Object.defineProperty(target, key, { value, writable: true, enumerable: true, configurable: true });
};

/**
 * The `copyData` of `core/data.ts`, into Effect's `Option` and `Result` too.
 * The ABI preserves data shape, not foreign array subclasses/constructors: all
 * copied arrays are fresh plain arrays. Opaque handles retain their identity.
 * Active cycles fail as RangeError; completed aliases are copied independently.
 */
export const copyData = <A>(value: A): A => {
  const work: Array<() => void> = [];
  const active = new WeakSet<object>();
  let result: unknown;
  const visit = (input: unknown, put: (copy: unknown) => void): void => {
    if (typeof input !== "object" || input === null) { put(input); return; }
    const array = Array.isArray(input);
    const proto: unknown = array ? undefined : Object.getPrototypeOf(input);
    const isGadt = !array && gadtMarker in input;
    const plain = !array && (proto === Object.prototype || proto === null || isGadt) && !Object.isFrozen(input);
    const option = !array && !plain && Option.isOption(input) ? input : undefined;
    const nativeResult = !array && !plain && option === undefined && Result.isResult(input) ? input : undefined;
    if (!array && !plain && option === undefined && nativeResult === undefined) { put(input); return; }
    if (active.has(input)) throw new RangeError("Maximum call stack size exceeded");
    active.add(input);
    const finish = (copy: unknown): void => { active.delete(input); put(copy); };
    if (!array && !plain) {
      if (option !== undefined) {
        if (!Option.isNone(option)) {
          const child = option.value;
          work.push(() => { visit(child, (copy) => { work.push(() => { finish(Option.some(copy)); }); }); });
        } else finish(Option.none());
      } else if (nativeResult !== undefined) {
        if (Result.isSuccess(nativeResult)) {
          const child = nativeResult.success;
          work.push(() => { visit(child, (copy) => { work.push(() => { finish(Result.succeed(copy)); }); }); });
        } else {
          const child = nativeResult.failure;
          work.push(() => { visit(child, (copy) => { work.push(() => { finish(Result.fail(copy)); }); }); });
        }
      }
    } else if (Array.isArray(input)) {
      const length = input.length;
      const copy: unknown[] = [];
      copy.length = length;
      let index = 0;
      const next = (): void => {
        while (index < length) {
          const key = index++;
          if (!(key in input)) continue;
          const child: unknown = input[key];
          work.push(next);
          visit(child, (item) => putOwn(copy, key, item));
          return;
        }
        finish(copy);
      };
      work.push(next);
    } else {
      const entries: Array<[string, unknown]> = Object.entries(input);
      const copy: Record<string, unknown> = {};
      if (isGadt && typeof proto === "object" && proto !== null) Object.setPrototypeOf(copy, proto);
      let index = 0;
      const next = (): void => {
        const entry = entries[index++];
        if (entry === undefined) { finish(copy); return; }
        const [key, child] = entry;
        work.push(next);
        visit(child, (item) => putOwn(copy, key, item));
      };
      work.push(next);
    }
  };
  visit(value, (copy) => { result = copy; });
  while (work.length > 0) work.pop()?.();
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a copy has the shape of what it copies
  return result as A;
};
