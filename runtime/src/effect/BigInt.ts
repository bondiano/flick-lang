// Effect's `BigInt` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { BigInt as BI } from "effect";

import type { Ordering } from "../data.ts";
import { toOrdering } from "./ordering.ts";

export * from "effect/BigInt";

/** `BigInt.sign`: as Flick's `Ordering`. */
export const flickSign = (n: bigint): Ordering => toOrdering(BI.sign(n));
