// Bindings of the built-in modules that need neither Effect nor a fiber.
import { writeStderr } from "./host.ts";

/** What a binding of a module this runtime does not have yet is, until it is called. */
export const absent =
  (module: string, name: string) =>
  (): never => {
    throw new Error(`\`${module}\` \`${name}\` is not on Flick's own runtime yet`);
  };

/** `todo`: code not written yet fails when it runs. */
export const flickTodo = (message: string): never => {
  throw new Error(message);
};

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
