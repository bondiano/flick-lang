// The data ABI of compiled Flick code on Effect: Option and Result are
// native Effect values; everything else is the data of `core/data.ts`,
// which compiled code without Effect uses alone.
import { Equal, Hash, Option, Result } from "effect";

import { derive as deriveData } from "@flick-lang/runtime/core/data";

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

/** `==`: structural equality with SameValueZero on numbers. */
export const equals = (a: unknown, b: unknown): boolean => Equal.equals(a, b);

export const hash = (a: unknown): number => Hash.hash(a);

/** A derived instance of class `name`, as `core/data.ts` derives it. */
export const derive = deriveData;

/** `nullable`: `null` and `undefined` are `None`. */
export const fromNullable = <A>(value: A | null | undefined): Option.Option<A> =>
  value === null || value === undefined ? none : some(value);

/** `optional i`: `None` is passed as `undefined`. */
export const optionalArg = <A>(option: Option.Option<A>): A | undefined =>
  Option.isSome(option) ? option.value : undefined;

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
    const plain = !array && (proto === Object.prototype || proto === null) && !Object.isFrozen(input);
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
          visit(child, (item) => { Object.defineProperty(copy, key, { value: item, writable: true, enumerable: true, configurable: true }); });
          return;
        }
        finish(copy);
      };
      work.push(next);
    } else {
      const entries: Array<[string, unknown]> = Object.entries(input);
      const copy: Record<string, unknown> = {};
      let index = 0;
      const next = (): void => {
        const entry = entries[index++];
        if (entry === undefined) { finish(copy); return; }
        const [key, child] = entry;
        work.push(next);
        visit(child, (item) => { Object.defineProperty(copy, key, { value: item, writable: true, enumerable: true, configurable: true }); });
      };
      work.push(next);
    }
  };
  visit(value, (copy) => { result = copy; });
  while (work.length > 0) work.pop()?.();
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a copy has the shape of what it copies
  return result as A;
};
