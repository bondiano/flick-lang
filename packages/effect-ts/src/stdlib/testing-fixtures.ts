// The fixtures of the tests on Effect: each made by its initializer the
// first time code of a test uses it, for that test or, shared, once for the
// run of its suite, in a scope of their own.
import { Cause, Context, Effect, Exit, Scope } from "effect";

import type * as Core from "@flick-lang/runtime/core/testing";

import { LazyCell, lazyForce } from "../effect/Lazy.ts";
import { flickAddFinalizer } from "../effect/Scope.ts";
import { Owner, closeOwn, inside } from "../owner.ts";
import { withOwnerEvidence } from "../owner_evidence.ts";

type Task = Effect.Effect<unknown, unknown>;

/** A suite in a run, as its fixtures see it: the shared fixtures it owns, and their cells. */
interface SharedRun {
  readonly suite: Readonly<{ shares?: ReadonlySet<Fixture> }>;
  readonly shared: Cells;
}

type Fixture = Core.Fixture<Task>;
type Cell = LazyCell<unknown, unknown, unknown>;

/**
 * The fixtures of a test or a suite's run, and the owner they are made in:
 * a scope of their own, which the scope of the test or the suite closes
 * after the hooks that run when it closes, as those may use them.
 */
export type Cells = Readonly<{ cells: Map<Fixture, Cell>; owner: Owner; broken: { first?: [Fixture, Cause.Cause<unknown>] } }>;

export const cellsIn = (outer: Owner): Cells => ({ cells: new Map(), owner: new Owner(Effect.runSync(Scope.make()), outer.defects), broken: {} });

/** `task`, the first failure of `fixture`'s initializer noted in `broken`: what makes its case a `setup` failure. */
const noting = (broken: Cells["broken"], fixture: Fixture, task: Effect.Effect<unknown, unknown, unknown>): Effect.Effect<unknown, unknown, unknown> =>
  Effect.onExit(task, (exit) =>
    Effect.sync(() => {
      if (Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)) broken.first ??= [fixture, exit.cause];
    }),
  );

/** The finalizer that closes `cells` with `outcome`: it fails as their finalizers do. */
export const closing = ({ owner }: Cells, outcome: () => Exit.Exit<unknown, unknown> = () => Exit.void): Effect.Effect<void, never, Scope.Scope> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a failed finalizer fails the closing of its scope
  flickAddFinalizer(Effect.flatMap(Effect.suspend(() => closeOwn(owner, outcome())), (ended) => ended) as Effect.Effect<void>);

/**
 * What code finds its fixtures in: those of the test it runs for, if any,
 * and the suites around it, outermost first, whose shared fixtures it sees.
 */
export class Fixtures {
  readonly test: Cells | undefined;
  readonly suites: ReadonlyArray<SharedRun>;
  constructor(test: Fixtures["test"], suites: ReadonlyArray<SharedRun>) {
    this.test = test;
    this.suites = suites;
  }
}

export const FixturesRef = /*#__PURE__*/ Context.Reference<Fixtures | undefined>("@flick-lang/runtime/Test.fixtures", {
  defaultValue: () => undefined,
});

/** `task`, with `fixtures` for the fixtures it uses. */
export const withFixtures = (fixtures: Fixtures, task: Task): Task => Effect.provideService(task, FixturesRef, fixtures);

/** The cell of `fixture`, made the first time: `init`, by default its initializer in the scope of the cells. */
const cellOf = (
  { cells, owner, broken }: Cells,
  fixture: Fixture,
  init: Effect.Effect<unknown, unknown, unknown> = inside(owner, withOwnerEvidence(owner, () => fixture.init)),
): Cell => {
  const found = cells.get(fixture);
  if (found !== undefined) return found;
  const cell: Cell = new LazyCell(noting(broken, fixture, init));
  cells.set(fixture, cell);
  return cell;
};

/**
 * `Test.use`: the `Lazy` of `fixture` for the test this code runs for, or
 * for the run of the innermost suite around it that shares it.
 */
export const testUse = (fixture: Fixture): Effect.Effect<Cell> =>
  Effect.flatMap(Effect.service(FixturesRef), (fixtures) => {
    if (fixtures === undefined) return Effect.die(new Error(`the fixture ${fixture.key} runs only in the code of a test or a suite that \`flick test\` runs`));
    if (fixture.shared) {
      const run = fixtures.suites.findLast((r) => r.suite.shares?.has(fixture) === true);
      if (run === undefined) return Effect.die(new Error(`the shared fixture ${fixture.key}, used by code that no suite sharing it runs`));
      // each test sees the shared cell through one of its own, which notes a failure for it
      return Effect.sync(() => {
        const shared = cellOf(run.shared, fixture);
        return fixtures.test === undefined ? shared : cellOf(fixtures.test, fixture, lazyForce(shared));
      });
    }
    const { test } = fixtures;
    if (test === undefined) return Effect.die(new Error(`the fixture ${fixture.key} of a test, used by code that runs for no test`));
    return Effect.sync(() => cellOf(test, fixture));
  });
