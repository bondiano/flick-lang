// The Effect release the adapter's contracts were verified against, and the
// operations it relies on. Another release is refused rather than trusted:
// Cause, scope, fiber and layer semantics may change between releases.
import * as effect from "effect";
import effectPackage from "effect/package.json" with { type: "json" };

export const EFFECT_VERSION = "4.0.5";

/** Operations of the capability matrix, by Effect module. */
const MATRIX = {
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
  Fiber: ["await", "join", "interrupt", "interruptAll"],
  Layer: ["buildWithScope", "succeed", "merge"],
  Cause: ["fromReasons", "combine", "hasDies", "hasFails", "hasInterruptsOnly", "pretty"],
  Context: ["Reference", "Service"],
};

/**
 * Problems that make the installed Effect unusable: another version, or an
 * operation of the matrix that is missing.
 */
export const capabilities = (version = effectPackage.version, modules = effect) => {
  const problems = [];
  if (version !== EFFECT_VERSION) {
    problems.push(`the Flick runtime requires effect ${EFFECT_VERSION}, found ${version}`);
  }
  for (const [module, names] of Object.entries(MATRIX)) {
    for (const name of names) {
      if (modules[module]?.[name] === undefined) {
        problems.push(`effect is missing ${module}.${name}`);
      }
    }
  }
  return problems;
};
