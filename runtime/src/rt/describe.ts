// A defect as inert text: building it never runs code of the value.
import { constructorName } from "../tags.ts";
import { type Cause, isDie, isFail } from "./cause.ts";

const LIMIT = 200;

const text = (s: string): string => (s.length > LIMIT ? `${s.slice(0, LIMIT)}…` : s);

const own = (value: object, key: string): string | undefined => {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  return descriptor && "value" in descriptor && typeof descriptor.value === "string" ? text(descriptor.value) : undefined;
};

/** What can be said of a payload without running any code of it. */
interface Described {
  readonly value?: string;
  readonly error?: true;
  readonly message?: string;
  readonly tag?: string | undefined;
}

const described = (payload: unknown): Described => {
  switch (typeof payload) {
    case "string":
      return { value: text(payload) };
    case "number":
    case "boolean":
    case "bigint":
    case "undefined":
      return { value: String(payload) };
    case "symbol":
      return { value: "<symbol>" };
    case "function":
      return { value: "<function>" };
    default:
  }
  if (typeof payload !== "object" || payload === null) return { value: "null" };
  if (payload instanceof Error) return { error: true, message: own(payload, "message") ?? "<error>" };
  return { tag: own(payload, "_tag"), value: "<object>" };
};

/** The message of an error, a primitive, or the constructor of a tagged value. */
export const describeDefect = (defect: unknown): string => {
  const d = described(defect);
  if (d.error) return d.message ?? "<error>";
  return d.tag === undefined ? (d.value ?? "") : constructorName(d.tag);
};

/** One reason of a `CauseReport`. */
type ReasonReport = (Described & { readonly kind: "fail" | "die" }) | { readonly kind: "interrupt"; readonly fiber: number | null };

/** `CauseReport`: what a cause holds, as inert data. */
export interface CauseReport {
  readonly reasons: ReadonlyArray<ReasonReport>;
}

/** `fields` as an object without a prototype: nothing of it runs code when read. */
const inert = <T extends object>(fields: T): Readonly<T> => {
  const out: T = { ...fields };
  Object.setPrototypeOf(out, null);
  return Object.freeze(out);
};

/** A `CauseReport` of a cause: kinds, safe tags and texts. */
export const report = (cause: Cause): CauseReport =>
  inert({
    reasons: Object.freeze(
      cause.reasons.map((r): ReasonReport => {
        if (isFail(r)) return inert({ kind: "fail" as const, ...described(r.error) });
        if (isDie(r)) return inert({ kind: "die" as const, ...described(r.defect) });
        return inert({ kind: "interrupt" as const, fiber: r.fiberId ?? null });
      }),
    ),
  });

/** `RawCause.describe`: one line of the reasons, with what can be said of each. */
export const describeReport = (r: CauseReport): string =>
  r.reasons
    .map((reason) => {
      if (reason.kind === "interrupt") return "interrupt";
      const shown = reason.tag === undefined ? reason.value : constructorName(reason.tag);
      const what = reason.error ? `error ${reason.message ?? ""}` : (shown ?? "");
      return `${reason.kind} ${what}`;
    })
    .join("; ");
