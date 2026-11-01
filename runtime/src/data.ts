// The data ABI of compiled Flick code on Effect: Option and Result are
// native Effect values; everything else is the data of `core/data.ts`,
// which compiled code without Effect uses alone.
import { Effect, Equal, Hash, Option, Result } from "effect";

import { type Dict, type FieldDict, copyThrough, derive as deriveData } from "./core/data.ts";
import { item } from "./internal.ts";

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
} from "./core/data.ts";

export const some = <A>(value: A): Option.Option<A> => Option.some(value);
export const none: Option.Option<never> = /*#__PURE__*/ Option.none();
export const ok = <A>(value: A): Result.Result<A> => Result.succeed(value);
export const err = <E>(error: E): Result.Result<never, E> => Result.fail(error);

/** `==`: structural equality with SameValueZero on numbers. */
export const equals = (a: unknown, b: unknown): boolean => Equal.equals(a, b);

export const hash = (a: unknown): number => Hash.hash(a);

/** `GreenLight` on the command line: `green-light`. */
const kebab = (name: string): string => name.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();

type ConstructorInfo = readonly [tag: string, name: string, arity: number, rep?: string];

/** The parser of a field of `Cli.Value`. */
const parseOf = (d: FieldDict) => (text: string) =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Cli.Value` dictionaries are made on Effect
  d.parse(text) as Effect.Effect<Result.Result<unknown, string>>;

/**
 * A derived instance of class `name`, as `core/data.ts` derives it; `Cli.Value`
 * of a refined type also gets its predicate and its name.
 */
export const derive = (
  name: string,
  fields: () => ReadonlyArray<FieldDict>,
  constructors: ReadonlyArray<ConstructorInfo> | null,
  supers: object | null,
  check?: (value: unknown) => boolean,
  typeName?: string,
): Dict => {
  if (name !== "Value") return deriveData(name, fields, constructors, supers);
  let cache: ReadonlyArray<FieldDict> | undefined;
  const dict = (k: number): FieldDict => item((cache ??= fields()), k);
  const self: Dict = {};
  Object.setPrototypeOf(self, supers);
  self.parse = (text: string): Effect.Effect<Result.Result<unknown, string>> => {
    if (constructors === null) {
      return Effect.map(parseOf(dict(0))(text), (parsed) =>
        Result.isSuccess(parsed) && check !== undefined && !check(parsed.success)
          ? Result.fail(`not a valid ${typeName ?? "value"}: ${text}`)
          : parsed,
      );
    }
    const choice = constructors.find((c) => kebab(c[1]) === text);
    const names = constructors.map((c) => kebab(c[1])).join(", ");
    return Effect.succeed(
      choice === undefined ? Result.fail(`expected one of ${names}, got ${text}`) : Result.succeed({ _tag: choice[0] }),
    );
  };
  return self;
};

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
