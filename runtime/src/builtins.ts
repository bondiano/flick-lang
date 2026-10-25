// Implementations of the `foreign` declarations of the built-in modules.
import { Effect, Formatter, type Option, Result } from "effect";

import { type Tagged, none } from "./data.ts";
import { writeStderr, writeStdout } from "./host.ts";
import { tag } from "./tags.ts";

export const stdoutLine = (text: string): void => writeStdout(`${text}\n`);

export const stderrLine = (text: string): void => writeStderr(`${text}\n`);

/** `todo`: code not written yet fails when it runs. */
export const flickTodo = (message: string): never => {
  throw new Error(message);
};

/** A failing `let assert`: its place and message, and the value it missed. */
export const flickAssert = (message: string, value: unknown): never => {
  throw new Error(`${message}: ${Formatter.format(value)}`);
};

/** `echo`: shows a value and where it was echoed, and returns it. */
export const flickEcho = <A>(location: string, value: A): A => {
  stderrLine(`${location} ${Formatter.format(value)}`);
  return value;
};

export const APP_ERROR = /*#__PURE__*/ tag("Prelude.AppError.AppError");

/** `Prelude.AppError`: what the entry runner reports when `main` fails. */
export type AppError = Tagged<typeof APP_ERROR, { readonly message: string; readonly details: Option.Option<string> }>;

/** `App.fail`: the typed failure the entry runner reports. */
export const appFail = (message: string): Effect.Effect<never, AppError> =>
  Effect.fail({ _tag: APP_ERROR, _0: { message, details: none } });

/** `from_result`: an `Err` becomes the typed failure. */
export const fromResult = <A, E>(result: Result.Result<A, E>): Effect.Effect<A, E> =>
  Result.isSuccess(result) ? Effect.succeed(result.success) : Effect.fail(result.failure);

/** `Ref a`: a mutable cell. */
export class Cell<A> {
  value: A;
  constructor(value: A) {
    this.value = value;
  }
}

export const refMake = <A>(value: A): Cell<A> => new Cell(value);
export const refGet = <A>(cell: Cell<A>): A => cell.value;
export const refSame = <A>(a: Cell<A>, b: Cell<A>): boolean => a === b;
export const refSet = <A>(cell: Cell<A>, value: A): void => {
  cell.value = value;
};
/** `Ref.modify cell f`: `f` gives the result and the new contents. */
export const refModify = <A, B>(cell: Cell<A>, f: (a: A) => readonly [B, A]): B => {
  const [result, next] = f(cell.value);
  cell.value = next;
  return result;
};

export const stringTrim = (s: string): string => s.trim();
export const stringLength = (s: string): number => s.length;

type Fields = Readonly<Record<string, unknown>>;

/** The dictionaries of record classes are erased: these take and ignore them. */
type Erased = unknown;

export const recordLabel: undefined = undefined;
export const recordGet = (_has: Erased, label: string, record: Fields): unknown => record[label];
export const recordExtend = (_lacks: Erased, _union: Erased, label: string, value: unknown, record: Fields): Fields => ({
  ...record,
  [label]: value,
});
export const recordRemove = (_remove: Erased, label: string, record: Fields): Fields => {
  const { [label]: _, ...rest } = record;
  return rest;
};
/** `Record.project`: the dictionary lists the labels kept. */
export const recordProject = (labels: ReadonlyArray<string>, record: Fields): Fields =>
  Object.fromEntries(labels.map((l) => [l, record[l]]));
export const recordMergeDisjoint = (_union: Erased, a: Fields, b: Fields): Fields => ({ ...a, ...b });
export const recordLabels = (_labels: Erased, record: Fields): Array<string> => Object.keys(record).toSorted();

export { numberSameValue, numberStrictEqual } from "./core/text.ts";
