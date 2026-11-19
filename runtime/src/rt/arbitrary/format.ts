// The report of a failed check, its values written as the JS target's
// formatter writes them, by two spaces: an array or an object of more than
// one entry over several lines. A variant is its object of `_tag` and
// fields; an Option, a Result and a Duration write themselves, without
// spaces, and a value with its own `toString` is what that gives.
//
// shortcut: Chunk and BigDecimal as their arrays and objects; write their own forms when a report shows one
import { Duration } from "../duration.ts";
import type { CheckResult } from "./runner.ts";

const durationText = (d: Duration): string =>
  d.nanos !== undefined ? `${d.nanos} nanos`
    : Number.isFinite(d.millis) ? `${d.millis} millis` : d.millis > 0 ? "Infinity" : "-Infinity";

const wrapped: Readonly<Record<string, readonly [string, string]>> = {
  Some: ["some", "value"], Success: ["success", "success"], Failure: ["failure", "failure"],
};

const format = (value: unknown, space: number): string => {
  const gap = " ".repeat(space);
  const entries = (open: string, close: string, keys: ReadonlyArray<string> | undefined,
    values: ReadonlyArray<unknown>, depth: number): string => {
    const multiline = gap !== "" && values.length > 1;
    const inner = gap.repeat(depth + 1);
    const parts = values.map((v, i) => {
      const key = keys === undefined ? "" : `${JSON.stringify(keys[i])}:${multiline ? " " : ""}`;
      return `${multiline ? inner : ""}${key}${recur(v, multiline ? depth + 1 : depth)}`;
    });
    return multiline ? `${open}\n${parts.join(",\n")}\n${gap.repeat(depth)}${close}` : `${open}${parts.join(",")}${close}`;
  };
  const recur = (v: unknown, depth: number): string => {
    if (typeof v === "string") return JSON.stringify(v);
    if (typeof v === "bigint") return `${v}n`;
    if (typeof v !== "object" || v === null) return String(v);
    if (Array.isArray(v)) return entries("[", "]", undefined, v, depth);
    if (v instanceof Duration) return durationText(v);
    const tag: unknown = Reflect.get(v, "_tag");
    if (tag === "None") return "none()";
    const form = typeof tag === "string" ? wrapped[tag] : undefined;
    if (form !== undefined) return `${form[0]}(${format(Reflect.get(v, form[1]), 0)})`;
    const own: unknown = Reflect.get(v, "toString");
    // oxlint-disable-next-line typescript/no-base-to-string -- a value of its own `toString`, as a Redacted one
    if (typeof own === "function" && own !== Object.prototype.toString) return String(v);
    const keys = Object.keys(v);
    return entries("{", "}", keys, keys.map((k) => Reflect.get(v, k)), depth);
  };
  return recur(value, 0);
};

/** The report of a check that did not pass. */
export const formatCheckFailure = (result: CheckResult): string | undefined => {
  switch (result._tag) {
    case "Passed": return undefined;
    case "Falsified":
      return [
        `Property falsified after ${result.runs} run(s) and ${result.shrinks} shrink(s)`,
        `Shrunk input: ${format(result.shrunkInput, 2)}`,
        result.failure._tag === "ReturnedFalse" ? "Failure: returned false" : `Failure: ${format(result.failure.error, 2)}`,
        `Replay: ${result.replay}`,
      ].join("\n");
    case "Exhausted":
      return `Property exhausted after ${result.runs} run(s) and ${result.discards} discard(s)\nSeed: ${JSON.stringify(String(result.seed))}`;
    case "ReplayMismatch": return `Property replay failed: ${result.reason}`;
  }
};
