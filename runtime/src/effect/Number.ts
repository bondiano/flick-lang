// Effect's `Number` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Number as Num } from "effect";

import type { Ordering } from "../data.ts";
import { toOrdering } from "./ordering.ts";

export * from "effect/Number";

/** `Number.sign`: as Flick's `Ordering`. */
export const flickSign = (n: number): Ordering => toOrdering(Num.sign(n));
