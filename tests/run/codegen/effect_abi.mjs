import { Effect } from "@flick-lang/effect-ts";

export const isEffect = (value) => Effect.isEffect(value);

export const runSync = (effect) => Effect.runSync(effect);

/** A two-argument callback as JS sees it: its arity and one call. */
export const stages = (f) => `${f.length} ${f(1, 2)}`;

export const bits = (n) => (Object.is(n, -0) ? "-0" : String(n));
