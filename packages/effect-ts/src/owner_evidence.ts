// Internal lexical owner channel. Transport is independent of resource ownership.
import { Context, Effect } from "effect";

/** Opaque, nonowning identity; crossing runtimes must not materialize it. */
export type OwnerEvidence = object | undefined;
export const OwnerEvidenceContext = /*#__PURE__*/ Context.Reference<OwnerEvidence>("@flick-lang/runtime/OwnerEvidence", {
  defaultValue: () => undefined,
});
export const readOwnerEvidence = (): Effect.Effect<OwnerEvidence> => Effect.service(OwnerEvidenceContext);
export const withOwnerEvidence = <A, E, R>(owner: OwnerEvidence, thunk: () => Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
  Effect.provideService(Effect.suspend(thunk), OwnerEvidenceContext, owner);

