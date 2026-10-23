// Effect's `Request` for the bindings of the standard library: a Flick
// request carries its payload in one field, so any Flick value can be one.
import { Effect, Request, type RequestResolver } from "effect";

import type { Failure, FlickExit } from "../cause.ts";
import { failureCause, nativeExit } from "../stdlib/outcome.ts";

export * from "effect/Request";

/** `Request p e a`. */
export interface FlickRequest<P, E, A> extends Request.Request<A, E> {
  readonly payload: P;
}

type Entry<P, E, A> = Request.Entry<FlickRequest<P, E, A>>;

/** `Request.of`. */
export const flickOf = <P, E, A>(payload: P): FlickRequest<P, E, A> => Request.of<FlickRequest<P, E, A>>()({ payload });

/** `Request.payload`. */
export const flickPayload = <P, E, A>(request: FlickRequest<P, E, A>): P => request.payload;

/** `Request.request`. */
export const flickRequest = <P, E, A>(entry: Entry<P, E, A>): P => entry.request.payload;

/** `Request.run`. */
export const flickRun =
  <P, E, A>(resolver: RequestResolver.RequestResolver<FlickRequest<P, E, A>>) =>
  (request: FlickRequest<P, E, A>): Effect.Effect<A, E> =>
    Effect.request(request, resolver);

/** `Request.complete`: the native exit of a Flick one. */
export const flickComplete =
  <A>(exit: FlickExit<A>) =>
  <P, E>(entry: Entry<P, E, A>): Effect.Effect<void> =>
    Request.complete(entry, nativeExit<A, E>(exit));

/** `Request.fail_cause`: the native cause of a `Failure`. */
export const flickFailCause =
  (failure: Failure) =>
  <P, E, A>(entry: Entry<P, E, A>): Effect.Effect<void> =>
    Request.failCause(entry, failureCause<E>(failure));
