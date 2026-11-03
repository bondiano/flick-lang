// The Effect release the adapter's contracts were verified against, and the
// operations it relies on. Another release is refused rather than trusted:
// Cause, scope, fiber and layer semantics may change between releases.
import * as effect from "effect";
import effectPackage from "effect/package.json" with { type: "json" };

export const EFFECT_VERSION = "4.1.2";

/** Operations of the capability matrix, by Effect module. */
const MATRIX: Readonly<Record<string, ReadonlyArray<string>>> = {
  Effect: [
    "gen",
    "sync",
    "promise",
    "suspend",
    "callback",
    "exit",
    "failCause",
    "catchCauseFilter",
    "forkDetach",
    "uninterruptible",
    "uninterruptibleMask",
    "provide",
    "provideService",
    "provideContext",
    "forEach",
    "all",
    "race",
    "raceFirst",
    "timeout",
    "timeoutOption",
    "acquireRelease",
    "acquireUseRelease",
    "runFork",
    "reduce",
    "tryPromise",
    "map",
    "flatMap",
    "as",
    "succeed",
  ],
  Chunk: ["empty", "fromIterable", "toArray", "append", "appendAll", "size", "getUnsafe", "map"],
  BigDecimal: ["make", "fromString", "fromNumber", "format", "normalize", "sum", "subtract", "Order"],
  Duration: ["millis", "nanos", "zero", "infinity", "isFinite", "toMillis", "sum", "Order", "format"],
  HashSet: ["empty", "fromIterable", "add", "remove", "has", "size", "union"],
  HashMap: ["empty", "fromIterable", "set", "remove", "get", "size", "union"],
  Redacted: ["make", "value"],
  Equal: ["symbol"],
  Hash: ["symbol"],
  Scope: ["make", "close", "addFinalizer", "Scope"],
  Fiber: ["await", "awaitAll", "join", "interrupt", "interruptAll"],
  Layer: ["buildWithScope", "succeed", "merge"],
  Cause: ["fromReasons", "combine", "hasDies", "hasFails", "hasInterruptsOnly", "pretty"],
  Context: ["Reference", "Service"],
};

/** The modules of an Effect installation, by name. */
export type Modules = Readonly<Record<string, unknown>>;

const has = (module: unknown, name: string): boolean =>
  typeof module === "object" && module !== null && Reflect.get(module, name) !== undefined;

/**
 * The problem of an installed Effect of another version, if any. The entry
 * runner checks only this: the matrix needs every Effect module at hand,
 * which would keep all of Effect in a browser bundle.
 */
export const versionProblems = (version: string = effectPackage.version): Array<string> =>
  version === EFFECT_VERSION ? [] : [`the Flick runtime requires effect ${EFFECT_VERSION}, found ${version}`];

/**
 * Problems that make the installed Effect unusable: another version, or an
 * operation of the matrix that is missing. The runtime's tests and
 * `just effect-bump` run it.
 */
export const capabilities = (version: string = effectPackage.version, modules: Modules = effect): Array<string> => {
  const problems = versionProblems(version);
  for (const [module, names] of Object.entries(MATRIX)) {
    for (const name of names) {
      if (!has(modules[module], name)) {
        problems.push(`effect is missing ${module}.${name}`);
      }
    }
  }
  return problems;
};
