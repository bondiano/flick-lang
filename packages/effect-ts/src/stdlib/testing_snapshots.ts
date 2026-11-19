// The snapshots of a test on Effect: `Test.snapshot` checks those of the
// test whose body runs it (see `@flick-lang/runtime/core/snapshots`).
import { Context, Effect, Option } from "effect";

import type { SnapshotCase } from "@flick-lang/runtime/core/snapshots";

const SnapshotsRef = /*#__PURE__*/ Context.Reference<SnapshotCase | undefined>("@flick-lang/runtime/Test.snapshots", {
  defaultValue: () => undefined,
});

/**
 * `Test.snapshot`: `None` when the snapshot `name` of the test this code runs
 * for holds `text`, or the run wrote it; `Some None` when there is none, and
 * `Some (Some stored)` when it holds another text.
 */
export const testSnapshot = (name: string, text: string): Effect.Effect<Option.Option<Option.Option<string>>> =>
  Effect.flatMap(Effect.service(SnapshotsRef), (check) => {
    if (check === undefined) return Effect.die(new Error("a snapshot is checked only in the body of a test that `flick test` runs"));
    return Effect.sync(() => {
      const stored = check(name, text);
      return stored.kind === "same" ? Option.none() : Option.some(stored.kind === "missing" ? Option.none() : Option.some(stored.text));
    });
  });

/** A test's body `task`, whose snapshots `snapshot` holds, if any. */
export const withSnapshot = <A, E>(task: Effect.Effect<A, E>, snapshot: SnapshotCase | undefined): Effect.Effect<A, E> =>
  snapshot === undefined ? task : Effect.provideService(task, SnapshotsRef, snapshot);
