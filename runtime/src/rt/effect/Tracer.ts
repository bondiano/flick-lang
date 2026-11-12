// Spans live in the kernel's evidence, so they follow the task across async
// suspension and child calls. A failed task keeps its frame on each reason.
import { none, type Option, some } from "../../core/data.ts";
import { tag } from "../../tags.ts";
import { type Exit, fromReasons, raise, type Reason } from "../cause.ts";
import { asExit, caught, currentCtx, extend, lookup, type Run, Under } from "../fiber.ts";

interface SpanLink {
  readonly span: Span;
  readonly attributes: ReadonlyArray<readonly [string, string]>;
}

interface SpanOptions {
  readonly attributes: ReadonlyArray<readonly [string, string]>;
  readonly links: ReadonlyArray<SpanLink>;
  readonly parent: Option<Span>;
  readonly root: boolean;
  readonly kind_: string;
  readonly sampled: Option<boolean>;
  readonly level: Option<string>;
  readonly captureStackTrace: boolean;
  readonly disablePropagation: boolean;
}

export interface Span {
  readonly name: string | undefined;
  readonly spanId: string;
  readonly traceId: string;
  readonly sampled: boolean;
  readonly parent: Span | undefined;
  readonly kind: string | undefined;
  readonly attributes: ReadonlyArray<readonly [string, string]>;
  readonly links: ReadonlyArray<SpanLink>;
  readonly started: bigint;
  ended: bigint | undefined;
}

interface Frame {
  readonly name: string;
  readonly stack: () => string | undefined;
  readonly parent: Frame | undefined;
}

const asFrame = (value: unknown): Frame | undefined =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "name") === "string" &&
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- checked frame fields at the runtime boundary
    typeof Reflect.get(value, "stack") === "function" ? value as Frame : undefined;

const appendParent = (inner: Frame | undefined, parent: Frame): Frame =>
  inner === undefined ? parent : { ...inner, parent: appendParent(inner.parent, parent) };

const CURRENT: object = /*#__PURE__*/ Object.freeze({ key: "rt/tracer/current" });
/** What watches spans: a client of the DevTools sees each start and end. */
export const tracerHooks: {
  parent?: () => unknown;
  started?: (span: Span) => void;
  ended?: (span: Span, exit: Exit) => void;
} = {};
const PROPAGATING: object = /*#__PURE__*/ Object.freeze({ key: "rt/tracer/propagating" });
const MINIMUM: object = /*#__PURE__*/ Object.freeze({ key: "rt/tracer/minimum" });
const LEVEL: object = /*#__PURE__*/ Object.freeze({ key: "rt/tracer/level" });
let nextId = 0;

const option = <A>(v: A | undefined): Option<A> => v === undefined ? none : some(v);
const value = <A>(v: Option<A>): A | undefined => v._tag === "Some" ? v.value : undefined;
const isSpan = (v: unknown): v is Span =>
  typeof v === "object" && v !== null && typeof Reflect.get(v, "spanId") === "string";
const asSpan = (v: unknown): Span | undefined => isSpan(v) ? v : undefined;
const asString = (v: unknown): string | undefined => typeof v === "string" ? v : undefined;
const levelRank = (name: string): number => {
  switch (name) {
    case "Trace": return 0;
    case "Debug": return 1;
    case "Info": return 2;
    case "Warn": return 3;
    case "Error": return 4;
    case "Fatal": return 5;
    default: return 2;
  }
};

export const flickExternalSpan = (options: { readonly spanId: string; readonly traceId: string; readonly sampled: boolean }): Span => ({
  name: undefined,
  spanId: options.spanId,
  traceId: options.traceId,
  sampled: options.sampled,
  parent: undefined,
  kind: undefined,
  attributes: [],
  links: [],
  started: 0n,
  ended: undefined,
});

export const flickWithSpan =
  (name: string, options: SpanOptions) =>
  (task: () => unknown): Run =>
    (function* (): Run {
      const parent = value(options.parent) ?? (options.root ? undefined : asSpan(lookup(PROPAGATING)));
      const currentLevel = value(options.level) ?? asString(lookup(LEVEL)) ?? "Info";
      const minimum = asString(lookup(MINIMUM)) ?? "Trace";
      const sampled = value(options.sampled) ?? (levelRank(currentLevel) >= levelRank(minimum));
      const id = String(++nextId);
      const span: Span = {
        name, spanId: id, traceId: parent?.traceId ?? id, sampled, parent,
        kind: options.kind_, attributes: options.attributes, links: options.links,
        started: BigInt(Date.now()), ended: undefined,
      };
      tracerHooks.started?.(span);
      const capturedStack = options.captureStackTrace ? new Error().stack : undefined;
      const frame: Frame = {
        name, parent: undefined,
        stack: () => capturedStack,
      };
      const ctx = extend(extend(currentCtx(), CURRENT, span), PROPAGATING,
        options.disablePropagation ? parent : span);
      let exit;
      try {
        exit = asExit(yield caught(() => new Under(ctx, task)));
      } finally {
        span.ended = BigInt(Date.now());
      }
      tracerHooks.ended?.(span, exit);
      if (exit.cause === undefined) return exit.value;
      const reasons: Array<Reason> = [];
      for (const reason of exit.cause.reasons) reasons.push(Object.assign({}, reason, {
        trace: appendParent(asFrame(Reflect.get(reason, "trace")), frame),
      }));
      return raise(fromReasons(reasons));
    })();

export const flickParentSpan = (): Option<Span> => option(asSpan(tracerHooks.parent?.() ?? lookup(CURRENT)));
export const nativeParentSpan = (): Span | undefined => asSpan(lookup(PROPAGATING));
export const flickSpanId = (span: Span): string => span.spanId;
export const flickTraceId = (span: Span): string => span.traceId;
export const flickSampled = (span: Span): boolean => span.sampled;
export const flickSpanName = (span: Span): Option<string> => option(span.name);
export const flickSpanKind = (span: Span): Option<string> => option(span.kind);
export const flickParent = (span: Span): Option<Span> => option(span.parent);
export const flickLinks = (span: Span): ReadonlyArray<SpanLink> => span.links;
export const flickAttributes = (span: Span): ReadonlyArray<readonly [string, string]> => span.attributes;

const STARTED = /*#__PURE__*/ tag("Tracer.SpanStatus.Started");
const ENDED = /*#__PURE__*/ tag("Tracer.SpanStatus.Ended");
export const flickStatus = (span: Span): { readonly _tag: string; readonly _0: bigint; readonly _1?: bigint } =>
  span.ended === undefined
    ? { _tag: STARTED, _0: span.started }
    : { _tag: ENDED, _0: span.started, _1: span.ended };

export const nativeTracer: object = /*#__PURE__*/ Object.freeze({ kind: "rt/tracer" });
export const flickWithTracer = (_tracer: object) => (task: () => unknown): unknown => task();
export const flickWithTraceLevel = (level: string) => (task: () => unknown): Under =>
  new Under(extend(currentCtx(), LEVEL, level), task);
export const flickWithMinimumTraceLevel = (level: string) => (task: () => unknown): Under =>
  new Under(extend(currentCtx(), MINIMUM, level), task);
