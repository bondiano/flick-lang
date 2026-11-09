// The data ABI of compiled Flick code on Effect: Option and Result are
// native Effect values; everything else is the data of `core/data.ts`,
// which compiled code without Effect uses alone.
import { Equal, Hash, Option, Result } from "effect";

import { copyThrough, derive as deriveData } from "@flick-lang/runtime/core/data";

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

/** The `copyData` of `core/data.ts`, into Effect's `Option` and `Result` too. */
export const copyData = <A>(value: A): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a copy has the shape of what it copies
  copyThrough(value, (x) => {
    if (Option.isOption(x)) return Option.map(x, copyData);
    if (Result.isResult(x)) return Result.isSuccess(x) ? Result.succeed(copyData(x.success)) : Result.fail(copyData(x.failure));
    return x;
  }) as A;
