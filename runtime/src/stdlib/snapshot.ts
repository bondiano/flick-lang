// Effect adapter for the shared, pure snapshot copier.
import { Effect } from "effect";

import { snapshotCopy, snapshotReason } from "../core/snapshot.ts";
import type { Tagged } from "../data.ts";
import { tag } from "../tags.ts";

const SNAPSHOT = /*#__PURE__*/ tag("Js.SnapshotError.SnapshotError");

/** `Js.SnapshotError`. */
export type SnapshotError = Tagged<typeof SNAPSHOT, string>;

/** `Js.snapshot_with depth limit value`. */
export const snapshot = (depth: number, limit: number, value: unknown): Effect.Effect<unknown, SnapshotError> =>
  Effect.suspend(() => {
    try {
      return Effect.succeed(snapshotCopy(value, depth, limit));
    } catch (error) {
      return Effect.fail({ _tag: SNAPSHOT, _0: snapshotReason(error) });
    }
  });

export const snapshotDefault = (value: unknown): Effect.Effect<unknown, SnapshotError> => snapshot(64, 100_000, value);
