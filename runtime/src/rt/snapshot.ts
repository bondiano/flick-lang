import { snapshotCopy, snapshotReason } from "../core/snapshot.ts";
import { tag } from "../tags.ts";
import { fail, raise } from "./cause.ts";

const SNAPSHOT = /*#__PURE__*/ tag("Js.SnapshotError.SnapshotError");

export const snapshot = (depth: number, limit: number, value: unknown): unknown => {
  try {
    return snapshotCopy(value, depth, limit);
  } catch (error) {
    return raise(fail({ _tag: SNAPSHOT, _0: snapshotReason(error) }));
  }
};

export const snapshotDefault = (value: unknown): unknown => snapshot(64, 100_000, value);
