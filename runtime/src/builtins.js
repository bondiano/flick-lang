// Implementations of the `foreign` declarations of the built-in modules.
import { Effect } from "effect";

import { none } from "./data.js";

const writeLine = (stream, text) => void stream.write(`${text}\n`);

export const stdoutLine = (text) => writeLine(process.stdout, text);

export const stderrLine = (text) => writeLine(process.stderr, text);

export const APP_ERROR = "Prelude.AppError.AppError";

/** `App.fail`: the typed failure the entry runner reports. */
export const appFail = (message) => Effect.fail({ _tag: APP_ERROR, _0: { message, details: none } });

/** `from_result`: an `Err` becomes the typed failure. */
export const fromResult = (result) =>
  result._tag === "Success" ? Effect.succeed(result.success) : Effect.fail(result.failure);

class Cell {
  constructor(value) {
    this.value = value;
  }
}

export const refMake = (value) => new Cell(value);
export const refGet = (cell) => cell.value;
export const refSame = (a, b) => a === b;
export const refSet = (cell, value) => {
  cell.value = value;
};
/** `Ref.modify cell f`: `f` gives the result and the new contents. */
export const refModify = (cell, f) => {
  const [result, next] = f(cell.value);
  cell.value = next;
  return result;
};

export const stringTrim = (s) => s.trim();
export const stringLength = (s) => s.length;

export const recordLabel = undefined;
export const recordGet = (_has, label, record) => record[label];
export const recordExtend = (_lacks, _union, label, value, record) => ({ ...record, [label]: value });
export const recordRemove = (_remove, label, record) => {
  const { [label]: _, ...rest } = record;
  return rest;
};
/** `Record.project`: the dictionary lists the labels kept. */
export const recordProject = (labels, record) =>
  Object.fromEntries(labels.map((l) => [l, record[l]]));
export const recordMergeDisjoint = (_union, a, b) => ({ ...a, ...b });
export const recordLabels = (_labels, record) => Object.keys(record).sort();

export const numberStrictEqual = (a, b) => a === b;
export const numberSameValue = (a, b) => Object.is(a, b);
