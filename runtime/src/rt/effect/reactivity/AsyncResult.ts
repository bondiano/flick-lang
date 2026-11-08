// `Reactivity.AsyncResult` on Flick's own runtime: the value of a task atom,
// a failure crossing as a Flick `Failure e`.
import { type Option, none, some } from "../../../core/data.ts";
import type { AsyncResult } from "../../atoms/registry.ts";
import { Failure } from "../../outcome.ts";

/** `Reactivity.AsyncResult.fold`. */
export const flickFold = <B>(initial: B, success: (a: unknown) => B, failure: (f: Failure) => B, result: AsyncResult): B =>
  result._tag === "Initial" ? initial : result._tag === "Success" ? success(result.value) : failure(new Failure(result.cause));

/** `Reactivity.AsyncResult.value`: of a success, else the last one a failure kept. */
export const value = (result: AsyncResult): Option<unknown> => {
  if (result._tag === "Success") return some(result.value);
  return result._tag === "Failure" && result.previous !== undefined ? some(result.previous.value) : none;
};

/** `Reactivity.AsyncResult.is_waiting`. */
export const isWaiting = (result: AsyncResult): boolean => result.waiting;
