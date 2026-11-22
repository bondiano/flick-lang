// Task atoms and `Reactivity.AsyncResult` on Flick's own runtime.
import type { Cause, Exit } from "../cause.ts";
import { forkNow } from "../fiber.ts";
import { withOwner } from "../owner.ts";
import { type Atom, type Reading, patched, readable } from "./registry.ts";

/** `Reactivity.AsyncResult`: a task's result as an atom holds it. */
export type AsyncResult =
  | { readonly _tag: "Initial"; readonly waiting: boolean }
  | { readonly _tag: "Success"; readonly value: unknown; readonly waiting: boolean }
  | {
      readonly _tag: "Failure";
      readonly cause: Cause;
      readonly previous: { readonly value: unknown } | undefined;
      readonly waiting: boolean;
    };

const isResult = (value: unknown): value is AsyncResult =>
  typeof value === "object" && value !== null && ["Initial", "Success", "Failure"].includes(String(Reflect.get(value, "_tag")));

/** The result of a run that has started again after `previous`. */
export const waitingFrom = (previous: { readonly value: unknown } | undefined): AsyncResult =>
  previous !== undefined && isResult(previous.value) ? { ...previous.value, waiting: true } : { _tag: "Initial", waiting: true };

export const lastSuccess = (previous: { readonly value: unknown } | undefined): { readonly value: unknown } | undefined => {
  const result = previous?.value;
  if (!isResult(result) || result._tag === "Initial") return undefined;
  return result._tag === "Success" ? { value: result.value } : result.previous;
};

const resultOf = (exit: Exit, previous: { readonly value: unknown } | undefined): AsyncResult =>
  exit.cause === undefined
    ? { _tag: "Success", value: exit.value, waiting: false }
    : { _tag: "Failure", cause: exit.cause, previous: lastSuccess(previous), waiting: false };

/**
 * The atom of the task `make` gives: run in a scope of its own, with the
 * evidence of the registry, by the registry that reads it; interrupted when
 * the atom computes again or is dropped.
 */
export const taskAtom = (make: (reading: Reading) => () => unknown): Atom<AsyncResult> =>
  patched(
    readable((reading) => {
      const task = make(reading);
      const previous = reading.self();
      const fiber = forkNow(() => withOwner(() => task()), reading.registry.ctx);
      if (fiber.exit !== undefined) return resultOf(fiber.exit, previous);
      fiber.observe((exit) => reading.setSelf(resultOf(exit, previous)));
      reading.addFinalizer(() => fiber.interrupt(-1));
      return waitingFrom(previous);
    }),
    { equals: Object.is },
  );
