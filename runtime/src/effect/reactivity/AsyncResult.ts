// Effect's `AsyncResult`, the value of an atom computed by an effect, for
// the bindings of the standard library: a failure crosses as a Flick
// `Failure e`.
import * as AsyncResult from "effect/reactivity/AsyncResult";

import { Failure } from "../../cause.ts";

export * from "effect/reactivity/AsyncResult";

/** `Reactivity.AsyncResult.fold`: the case of `result`, a failure as a `Failure e`. */
export const flickFold = <A, E, B>(
  initial: B,
  success: (a: A) => B,
  failure: (f: Failure) => B,
  result: AsyncResult.AsyncResult<A, E>,
): B =>
  AsyncResult.match(result, {
    onInitial: () => initial,
    onSuccess: (s) => success(s.value),
    onFailure: (f) => failure(new Failure(f.cause)),
  });
