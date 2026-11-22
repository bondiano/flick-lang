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

const CHUNK_ID = "~effect/Chunk";
const isChunk = (v: object): v is Iterable<unknown> & { readonly length: number } => CHUNK_ID in v;
const keysOf = (o: Readonly<Record<string, unknown>>, skipHeld: boolean): Array<string> =>
  skipHeld ? Object.keys(o).filter((k) => k.charCodeAt(0) !== 36) : Object.keys(o);
const isGadt = (v: unknown): v is Readonly<Record<string, unknown>> =>
  typeof v === "object" && v !== null && !Array.isArray(v) && gadtMarker in v;
const isPlainRecord = (v: unknown): v is Readonly<Record<string, unknown>> => {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return false;
  const proto: unknown = Object.getPrototypeOf(v);
  return proto === Object.prototype || proto === null;
};

const eqRecords = (
  a: Readonly<Record<string, unknown>>,
  b: Readonly<Record<string, unknown>>,
  skipHeld: boolean,
  eq: (x: unknown, y: unknown) => boolean,
): boolean => {
  const ks = keysOf(a, skipHeld);
  return ks.length === keysOf(b, skipHeld).length && ks.every((k) => Object.hasOwn(b, k) && eq(a[k], b[k]));
};

const hashGadt = (o: Readonly<Record<string, unknown>>): number => {
  let h = 12289;
  for (const k of keysOf(o, true)) h ^= Hash.combine(Hash.string(k), Hash.hash(o[k]));
  return Hash.optimize(h);
};

/** `==`: Effect's structural equality; `GADT_PROTO[Equal.symbol]` skips held `$`-prefixed dictionaries. */
export const equals = Equal.equals;

/** Structural hash; `GADT_PROTO[Hash.symbol]` skips held `$`-prefixed dictionaries. */
export const hash = Hash.hash;

/**
 * Structural SameValue equivalence: distinguishes `+0` and `-0`, preserves
 * identity for mutable cells and foreign values, keeps `$`-prefixed keys on
 * plain objects (`Json` / `ReadonlyRecord`), and skips held dictionaries on
 * `gadtMarker`-tagged GADT values.
 */
export const sameValue = (a: unknown, b: unknown): boolean => {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || a === null || typeof b !== "object" || b === null) return false;
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => sameValue(x, b[i]));
  if (Array.isArray(b)) return false;
  const chunkA = isChunk(a);
  const chunkB = isChunk(b);
  if (chunkA || chunkB) {
    if (!chunkA || !chunkB || a.length !== b.length) return false;
    const right = b[Symbol.iterator]();
    for (const x of a) if (!sameValue(x, right.next().value)) return false;
    return true;
  }
  if (Option.isOption(a) && Option.isOption(b)) {
    return Option.isNone(a) ? Option.isNone(b) : Option.isSome(b) && sameValue(a.value, b.value);
  }
  if (Result.isResult(a) && Result.isResult(b)) {
    return Result.isSuccess(a)
      ? Result.isSuccess(b) && sameValue(a.success, b.success)
      : Result.isFailure(b) && sameValue(a.failure, b.failure);
  }
  const gadtA = isGadt(a);
  const gadtB = isGadt(b);
  if (gadtA || gadtB) return gadtA && gadtB && eqRecords(a, b, true, sameValue);
  const plainA = isPlainRecord(a);
  const plainB = isPlainRecord(b);
  if (plainA || plainB) return plainA && plainB && eqRecords(a, b, false, sameValue);
  return Equal.isEqual(a) && Equal.isEqual(b) && Equal.equals(a, b);
};

const GADT_PROTO: Record<PropertyKey, unknown> = /*#__PURE__*/ Object.freeze({
  [gadtMarker]: true,
  [Equal.symbol](this: Readonly<Record<string, unknown>>, that: unknown): boolean {
    return isGadt(that) && eqRecords(this, that, true, Equal.equals);
  },
  [Hash.symbol](this: Readonly<Record<string, unknown>>): number {
    return hashGadt(this);
  },
  [runtimeEquals](this: Readonly<Record<string, unknown>>, that: unknown, strictZero = false): boolean {
    return isGadt(that) && eqRecords(this, that, true, strictZero ? sameValue : Equal.equals);
  },
  [runtimeHash](this: Readonly<Record<string, unknown>>): number {
    return hashGadt(this);
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
    const gadtObj = !array && gadtMarker in input;
    const plain = !array && (proto === Object.prototype || proto === null || gadtObj) && !Object.isFrozen(input);
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
      if (gadtObj && typeof proto === "object" && proto !== null) Object.setPrototypeOf(copy, proto);
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
