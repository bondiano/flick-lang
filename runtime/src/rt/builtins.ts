// Bindings of the built-in modules that need neither Effect nor a fiber.
import { writeStderr } from "./host.ts";
import { none } from "../core/data.ts";
import { tag } from "../tags.ts";
import { fail, raise } from "./cause.ts";

/** What a binding of a module this runtime does not have yet is, until it is called. */
export const absent = (module: string, name: string): (() => never) => {
  const missing = (): never => {
    throw new Error(`\`${module}\` \`${name}\` is not on Flick's own runtime yet`);
  };
  Object.defineProperty(missing, Symbol.for("flick/absent"), { value: true });
  return missing;
};

/** `todo`: code not written yet fails when it runs. */
export const flickTodo = (message: string): never => {
  throw new Error(message);
};

const APP_ERROR = /*#__PURE__*/ tag("Prelude.AppError.AppError");

/** The typed application error that the entry runner prints. */
export const appFail = (message: string): never =>
  raise(fail({ _tag: APP_ERROR, _0: { message, details: none } }));

const shown = (value: unknown): string => {
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
};

/** A failing `let assert`: its place and message, and the value it missed. */
export const flickAssert = (message: string, value: unknown): never => {
  throw new Error(`${message}: ${shown(value)}`);
};

/** `echo`: shows a value and where it was echoed, and returns it. */
export const flickEcho = <A>(location: string, value: A): A => {
  writeStderr(`${location} ${shown(value)}\n`);
  return value;
};

/** `Ref a`: a mutable cell. */
export class Cell {
  value: unknown;
  constructor(value: unknown) {
    this.value = value;
  }
}

export const refMake = (value: unknown): Cell => new Cell(value);
export const refGet = (cell: Cell): unknown => cell.value;
export const refSame = (a: Cell, b: Cell): boolean => a === b;
export const refSet = (cell: Cell, value: unknown): void => {
  cell.value = value;
};
/** `Ref.modify cell f`: `f` gives the result and the new contents, one step: it cannot suspend. */
export const refModify = (cell: Cell, f: (a: unknown) => readonly [unknown, unknown]): unknown => {
  const [result, next] = f(cell.value);
  cell.value = next;
  return result;
};

type Fields = Readonly<Record<string, unknown>>;
type Erased = unknown;
export const recordLabel: undefined = undefined;
export const recordGet = (_has: Erased, label: string, record: Fields): unknown => record[label];
export const recordExtend = (_lacks: Erased, _union: Erased, label: string, value: unknown, record: Fields): Fields => ({
  ...record, [label]: value,
});
export const recordRemove = (_remove: Erased, label: string, record: Fields): Fields => {
  const { [label]: _, ...rest } = record;
  return rest;
};
export const recordProject = (labels: ReadonlyArray<string>, record: Fields): Fields =>
  Object.fromEntries(labels.map((label) => [label, record[label]]));
export const recordMergeDisjoint = (_union: Erased, a: Fields, b: Fields): Fields => ({ ...a, ...b });
export const recordLabels = (_labels: Erased, record: Fields): Array<string> => Object.keys(record).toSorted();
