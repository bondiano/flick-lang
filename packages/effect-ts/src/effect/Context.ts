// Effect's `Context` for the bindings of the standard library, and the
// adapters for references.
import { Context, Effect } from "effect";

export * from "effect/Context";

/** `Context.merge_all`: the first context and the rest. */
export const flickMergeAll = <S>(first: Context.Context<S>, rest: ReadonlyArray<Context.Context<S>>): Context.Context<S> =>
  Context.mergeAll(first, ...rest);

/** `Context.reference`. */
export const flickReference = <A>(key: string, defaultValue: () => A): Context.Reference<A> =>
  Context.Reference<A>(key, { defaultValue });

/** `Context.read`. */
export const flickRead = <A>(ref: Context.Reference<A>): Effect.Effect<A> => Effect.service(ref);

/** `Context.with_reference`. */
export const flickWithReference =
  <A>(ref: Context.Reference<A>, value: A) =>
  <B, E, R>(self: Effect.Effect<B, E, R>): Effect.Effect<B, E, R> =>
    Effect.provideService(self, ref, value);
