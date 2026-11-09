// Adapters of `Tx`, the transactions of the standard library over Effect's
// `Effect.tx`, `Effect.txRetry` and `Effect.Transaction`.
import { Effect, type Option } from "effect";

import { none, some } from "../data.ts";
import { tag } from "../tags.ts";

/** `Tx.RetryReason`. */
export type RetryReason = Readonly<{ _tag: string }>;

const retry: RetryReason = /*#__PURE__*/ Object.freeze({ _tag: tag("Tx.RetryReason.Retry") });
const conflict: RetryReason = /*#__PURE__*/ Object.freeze({ _tag: tag("Tx.RetryReason.Conflict") });

/** `Tx.retry`: runs the transaction again once a value it read changes. */
export const flickRetry = (): Effect.Effect<never, never, Effect.Transaction> => Effect.txRetry;

/** `Tx.Meta`. */
export interface Meta {
  readonly attempt: number;
  readonly retryReason: Option.Option<RetryReason>;
  readonly start: number;
  readonly now: number;
  readonly elapsed: number;
  readonly elapsedSincePrevious: number;
}

/** `Tx.current`: the metadata of the current attempt, without its journal. */
export const flickCurrent = (): Effect.Effect<Meta, never, Effect.Transaction> =>
  Effect.map(Effect.service(Effect.Transaction), (meta) =>
    Object.freeze({
      attempt: meta.attempt,
      retryReason: meta.retryReason === undefined ? none : some(meta.retryReason === "retry" ? retry : conflict),
      start: meta.start,
      now: meta.now,
      elapsed: meta.elapsed,
      elapsedSincePrevious: meta.elapsedSincePrevious,
    }),
  );
