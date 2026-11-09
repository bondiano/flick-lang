// The bridge between the kernel and Effect. Effect's Context travels on
// the kernel's evidence (`Ctx.host`); the kernel's evidence travels in Effect's
// Context under `KERNEL`. A crossing that ends at once takes no suspension.
import {
  Cause as ECause,
  Context,
  DateTime as EDateTime,
  Duration as EDuration,
  Effect,
  Exit as EExit,
  Fiber as EFiber,
  Metric,
  Option as EOption,
  Schedule as ESchedule,
  Scheduler,
  Scope as EScope,
  Tracer as ETracer,
} from "effect";
import type { StackFrame } from "effect/References";

import { FlickFiber as HostFlickFiber } from "./owner.ts";
import { syncContext, syncIn } from "./sync.ts";
import { type Cause, type Exit, Raise, dieReason, failReason, failed, fromReasons, interruptReason, raise, succeeded } from "@flick-lang/runtime/rt/cause";
import { type Ctx, type Fiber, type Run, Suspend, awaitFiber, currentCtx, currentFiber, fiberHooks, forkNow, hosted, inCtx, owned, asExit, exitOf, raiseOf, runIn } from "@flick-lang/runtime/rt/fiber";
import { Duration } from "@flick-lang/runtime/rt/duration";

import { FlickFiber as NativeFlickFiber, Owner, close } from "@flick-lang/runtime/rt/owner";

import { durationHooks } from "@flick-lang/runtime/rt/duration";
import { Schedule as NativeSchedule, outOf } from "@flick-lang/runtime/rt/effect/Schedule";
import { nativeParentSpan, tracerHooks } from "@flick-lang/runtime/rt/effect/Tracer";
import { causeHooks } from "@flick-lang/runtime/rt/effect/Cause";
import { dateTimeHooks } from "@flick-lang/runtime/rt/effect/DateTime";
import { inheritContext, rememberContext } from "./bridge_context.ts";

import { dataFromEffect, dataToEffect, installDataBridge } from "./bridge_data.ts";
import { installLayerBridge } from "./bridge_layer.ts";
import { installSchemaBridge } from "./bridge_schema.ts";
import { bridgeKernel } from "./bridge_runtime.ts";
import { KERNEL, hostOf, settled } from "./bridge_host.ts";


type EffectFiber = EFiber.Fiber<unknown, unknown>;
const nativeIdentities = /*#__PURE__*/ new WeakMap<Fiber, number>();
const effectIdentities = /*#__PURE__*/ new WeakMap<EffectFiber, number>();
/** Fiber-local library state keeps its identity across interpreter calls. */
export const fiberIdentity: Effect.Effect<number> = Effect.withFiber((fiber) =>
  Effect.succeed(effectIdentities.get(fiber) ?? fiber.id));

export const durationToEffect = (value: unknown): unknown =>
  value instanceof Duration
    ? value.nanos === undefined ? EDuration.millis(value.millis) : EDuration.nanos(value.nanos)
    : value;

export const durationFromEffect = (value: unknown): unknown => {
  if (!EDuration.isDuration(value)) return value;
  const nanos = EDuration.toNanos(value);
  return EOption.isSome(nanos) ? new Duration(EDuration.toMillis(value), nanos.value) : new Duration(Infinity);
};

/** A native schedule's steps as Effect pulls, preserving delays and completion. */
export const scheduleToEffect = (value: unknown): unknown => {
  if (!(value instanceof NativeSchedule)) return value;
  return ESchedule.fromStep(Effect.sync(() => {
    const step = value.start();
    return (now: number, input: unknown) => Effect.flatMap(callEffect(() => step(now, input)), (out) => {
      const [output, delay, done] = outOf(out);
      if (done) return ECause.done(durationToEffect(output));
      const duration = delay.nanos === undefined ? EDuration.millis(delay.millis) : EDuration.nanos(delay.nanos);
      const result: [unknown, EDuration.Duration] = [durationToEffect(output), duration];
      return Effect.succeed(result);
    });
  }));
};

const isScheduleStep = (value: unknown): value is (now: number, input: unknown) => Effect.Effect<unknown, unknown> =>
  typeof value === "function";

/** An Effect schedule as native steps, with a fresh stepper for each run. */
export const scheduleFromEffect = (value: unknown): unknown => {
  if (!ESchedule.isSchedule(value)) return value;
  return new NativeSchedule(() => {
    let step: unknown;
    return (now, input) => (function* (): Run {
      if (!isScheduleStep(step)) step = yield fromEffect(ESchedule.toStep(value));
      if (!isScheduleStep(step)) throw new TypeError("an Effect schedule did not provide a step");
      const outcome: unknown = yield fromEffect(Effect.exit(step(now, input)));
      if (!EExit.isExit(outcome)) throw new TypeError("an Effect schedule did not provide an exit");
      if (EExit.isSuccess(outcome)) {
        const pair: unknown = outcome.value;
        if (!Array.isArray(pair) || pair.length !== 2 || !EDuration.isDuration(pair[1]))
          throw new TypeError("an Effect schedule did not provide an output and duration");
        const delay = durationFromEffect(pair[1]);
        if (!(delay instanceof Duration)) throw new TypeError("an Effect schedule gave an invalid duration");
        const out: [unknown, Duration, boolean] = [durationFromEffect(pair[0]), delay, false];
        return out;
      }
      const failure = ECause.findErrorOption(outcome.cause);
      if (EOption.isSome(failure) && ECause.isDone(failure.value)) {
        const out: [unknown, Duration, boolean] = [durationFromEffect(failure.value.value), new Duration(0), true];
        return out;
      }
      return raise(causeIn(outcome.cause));
    })();
  });
};

const nativeFibers = /*#__PURE__*/ new WeakMap<EffectFiber, NativeFlickFiber>();
const hostFibers = /*#__PURE__*/ new WeakMap<NativeFlickFiber, HostFlickFiber<unknown, unknown>>();

/** An Effect child as a native Fiber; native interruption waits for the Effect child to settle. */
export const fiberFromEffect = (value: unknown): unknown => {
  if (!(value instanceof HostFlickFiber)) return value;
  const ef: EffectFiber = value.fiber;
  const found = nativeFibers.get(ef);
  if (found !== undefined) return found;
  const native = forkNow(() => fromEffect(EFiber.join(ef)), currentCtx());
  native.interrupt = (by: number): void => { ef.interruptUnsafe(by); };
  const wrapped = new NativeFlickFiber(native);
  nativeFibers.set(ef, wrapped);
  hostFibers.set(wrapped, value);
  return wrapped;
};

function* joinNative(fiber: NativeFlickFiber): Run {
  return raiseOf(asExit(yield awaitFiber(fiber.fiber)));
}

/** A native child as an Effect Fiber for collections that own or interrupt it. */
export const fiberToEffect = (value: unknown): unknown => {
  if (!(value instanceof NativeFlickFiber)) return value;
  const found = hostFibers.get(value);
  if (found !== undefined) return found;
  const ctx = currentCtx();
  const by = currentFiber().id;
  const services = Context.add(hostOf(ctx), KERNEL, ctx);
  const ef = Effect.runForkWith(services)(Effect.onInterrupt(
    callEffect(() => joinNative(value)),
    () => Effect.sync(() => value.fiber.interrupt(by)),
  ));
  const wrapped = new HostFlickFiber(ef);
  hostFibers.set(value, wrapped);
  nativeFibers.set(ef, value);
  return wrapped;
};

/** An Effect a binding gave: what it requires is in the services its crossing provides. */
const isEffect = (u: unknown): u is Effect.Effect<unknown, unknown> => Effect.isEffect(u);

const contextOf = (u: unknown): Context.Context<never> | undefined => (Context.isContext(u) ? u : undefined);

const spanFromEffect = (span: ETracer.AnySpan): unknown => {
  if (Reflect.has(span, "started") && Array.isArray(Reflect.get(span, "attributes"))) return span;
  if (span._tag === "ExternalSpan") return {
    name: undefined, spanId: span.spanId, traceId: span.traceId, sampled: span.sampled,
    parent: undefined, kind: undefined, attributes: [], links: [], started: 0n, ended: undefined,
  };
  const parent = EOption.getOrUndefined(span.parent);
  return {
    name: span.name, spanId: span.spanId, traceId: span.traceId, sampled: span.sampled,
    parent: parent === undefined ? undefined : spanFromEffect(parent), kind: span.kind,
    attributes: Array.from(span.attributes, ([key, value]) => [key, String(value)] as const),
    links: span.links.map((link) => ({ span: spanFromEffect(link.span), attributes: Object.entries(link.attributes).map(([key, value]) => [key, String(value)] as const) })),
    started: span.status.startTime,
    ended: span.status._tag === "Ended" ? span.status.endTime : undefined,
  };
};
tracerHooks.parent = () => {
  const host = currentCtx().host;
  const span = Context.isContext(host) ? Context.getOrUndefined(host, ETracer.ParentSpan) : undefined;
  return span === undefined ? undefined : spanFromEffect(span);
};
export const inNativeRuntime: Effect.Effect<boolean> = Effect.withFiber((fiber) =>
  Effect.succeed(Context.getOrUndefined(fiber.context, KERNEL) !== undefined));

/** Effect's cause as the kernel's: the reasons have the same fields. */
const causeIn = (cause: ECause.Cause<unknown>): Cause =>
  fromReasons(
    cause.reasons.map((r) => Object.assign(
      ECause.isFailReason(r) ? failReason(dataFromEffect(r.error)) : ECause.isDieReason(r) ? dieReason(r.defect) : interruptReason(r.fiberId),
      { trace: Context.getOrUndefined(ECause.reasonAnnotations(r), ECause.StackTrace), annotations: ECause.reasonAnnotations(r) },
    )),
  );
export const causeFromEffect = causeIn;

/** The kernel's cause as Effect's. */
export const causeToEffect = (cause: Cause): ECause.Cause<unknown> =>
  ECause.fromReasons(
    cause.reasons.flatMap((r) => {
      const reason = r._tag === "Fail" ? ECause.makeFailReason(dataToEffect(r.error)) : r._tag === "Die" ? ECause.makeDieReason(r.defect) : ECause.makeInterruptReason(r.fiberId);
      let annotations = contextOf(Reflect.get(r, "annotations")) ?? Context.empty();
      const trace: unknown = Reflect.get(r, "trace");
      if (typeof trace === "object" && trace !== null && typeof Reflect.get(trace, "stack") === "function") {
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the native tracer owns this checked frame
        annotations = Context.add(annotations, ECause.StackTrace, trace as StackFrame);
      }
      return ECause.annotate(ECause.fromReasons([reason]), annotations).reasons;
    }),
  );

const prettyCause = (cause: Cause): ECause.Cause<unknown> => {
  const converted = causeToEffect(cause);
  const trace = cause.reasons.map((reason): unknown => Reflect.get(reason, "trace")).find((value) =>
    typeof value === "object" && value !== null && typeof Reflect.get(value, "name") === "string");
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- validated frame shape at the runtime boundary
  return trace === undefined ? converted : ECause.annotate(converted, Context.make(ECause.StackTrace, trace as StackFrame));
};
causeHooks.pretty = (failure) => ECause.pretty(prettyCause(failure.cause));
causeHooks.prettyErrors = (failure) => ECause.prettyErrors(prettyCause(failure.cause)).map((error) => `${error.name}: ${error.message}`);

fiberHooks.started = (ctx, fiber) => {
  inheritContext(currentFiber(), fiber);
  const host = contextOf(ctx.host);
  if (host === undefined) return;
  const metrics = Context.getOrUndefined(host, Metric.FiberRuntimeMetrics);
  if (metrics === undefined) return;
  metrics.recordFiberStart(host);
  fiber.observe((exit) => metrics.recordFiberEnd(host,
    exit.cause === undefined ? EExit.succeed(exit.value) : EExit.failCause(causeToEffect(exit.cause))));
};

const exitValue = (exit: EExit.Exit<unknown, unknown>): unknown => {
  if (exit._tag === "Success") return exit.value;
  return raise(causeIn(exit.cause));
};

/** Waits for an Effect fiber that did not end at once; an interruption is passed on and waits for it to end. */
function* awaiting(ef: EffectFiber, owner: Owner | undefined, scope: EScope.Closeable): Run {
  const fiber = currentFiber();
  try {
    yield new Suspend((resolve) => {
      ef.addObserver(resolve);
    });
  } catch (thrown) {
    fiber.mask++;
    try {
      ef.interruptUnsafe(fiber.id);
      if (ef.pollUnsafe() === undefined) {
        yield new Suspend((resolve) => {
          ef.addObserver(resolve);
        });
      }
      yield settled(owner, scope);
    } finally {
      fiber.mask--;
    }
    throw thrown;
  }
  yield settled(owner, scope);
  const exit = ef.pollUnsafe();
  if (exit === undefined) throw new Error("an Effect fiber woke the kernel before it ended");
  return exitValue(exit);
}

/** An Effect as a call of the kernel: runs at once, on this stack, as far as it goes. */
export const fromEffect = (effect: unknown): unknown => {
  // A value already on the kernel needs no conversion.
  if (!isEffect(effect)) return effect;
  if (EExit.isExit(effect)) return exitValue(effect);
  const ctx = currentCtx();
  const scope = EScope.makeUnsafe();
  let services = Context.add(Context.add(hostOf(ctx), KERNEL, ctx), EScope.Scope, scope);
  const parent = nativeParentSpan();
  if (parent !== undefined && Context.getOrUndefined(services, ETracer.ParentSpan) === undefined) {
    let span = effectSpans.get(parent);
    if (span === undefined) {
      span = ETracer.externalSpan({ spanId: parent.spanId, traceId: parent.traceId, sampled: parent.sampled });
      effectSpans.set(parent, span);
    }
    services = Context.add(services, ETracer.ParentSpan, span);
  }
  services = Context.add(services, Scheduler.PreventSchedulerYield, true);
  const fiber = currentFiber();
  const identity = nativeIdentities.get(fiber) ?? -fiber.id;
  const tracked = Effect.withFiber((ef) => {
    effectIdentities.set(ef, identity);
    return Effect.onExit(effect, () => Effect.sync(() => rememberContext(fiber, ctx, services, ef)));
  });
  const ef: EffectFiber = Effect.runForkWith(services)(tracked, { uninterruptible: fiber.mask > 0 });
  const exit = ef.pollUnsafe();
  if (exit === undefined) return awaiting(ef, ctx.owner, scope);
  if (scope.state._tag !== "Empty") {
    const owner = ctx.owner;
    return (function* (): Run {
      yield settled(owner, scope);
      return exitValue(exit);
    })();
  }
  return exitValue(exit);
};

const exitOut = (exit: Exit): Effect.Effect<unknown, unknown> => {
  return exit.cause === undefined ? Effect.succeed(exit.value) : Effect.failCause(causeToEffect(exit.cause));
};

const ROOT_CTX = /*#__PURE__*/ currentCtx();
const effectSpans = /*#__PURE__*/ new WeakMap<object, ETracer.AnySpan>();

/** An Effect acquisition scope owns the kernel resources acquired by its callbacks. */
const scopeOwners = /*#__PURE__*/ new WeakMap<EScope.Scope, Owner>();
const ownerOfScope = (scope: EScope.Scope, base: Ctx): Effect.Effect<Owner> => {
  const existing = scopeOwners.get(scope);
  if (existing !== undefined) return Effect.succeed(existing);
  const owner = new Owner(base.owner?.defects);
  scopeOwners.set(scope, owner);
  return Effect.as(EScope.addFinalizerExit(scope, (exit) =>
    Effect.orDie(callKernel(() => close(owner, exit._tag === "Success" ? succeeded(undefined) : failed(causeIn(exit.cause))), false)),
  ), owner);
};

/** A kernel call as an Effect: a fiber of the kernel per run, with the evidence of the Effect fiber that runs it. */
const callKernel = (call: () => unknown, scoped: boolean): Effect.Effect<unknown, unknown> => {
  return Effect.withFiber((ef) => {
    const context = ef.context;
    const base = Context.getOrUndefined(context, KERNEL) ?? ROOT_CTX;
    const scope = scoped ? Context.getOrUndefined(context, EScope.Scope) : undefined;
    return Effect.flatMap(scope === undefined ? Effect.succeed(base) : Effect.map(ownerOfScope(scope, base), (owner) => owned(base, owner)), (active) => {
      // `interruptible` is not in the public type of Effect's fiber
      const fiber: Fiber = forkNow(() => {
        nativeIdentities.set(currentFiber(), effectIdentities.get(ef) ?? ef.id);
        return call();
      }, hosted(active, context), Reflect.get(ef, "interruptible") === false ? 1 : 0);
      if (fiber.exit !== undefined) return exitOut(fiber.exit);
      return Effect.callback<unknown, unknown>((resume) => {
        fiber.observe((exit) => resume(exitOut(exit)));
        // the interruption is passed on, and waits for its finalizers
        return Effect.callback<void>((done) => {
          fiber.observe(() => done(Effect.void));
          fiber.interrupt(ef.id);
        });
      });
    });
  });
};
export const callEffect = (call: () => unknown): Effect.Effect<unknown, unknown> => callKernel(call, true);

/** A Sync stage of Flick that Effect's side calls and that gives its value: the evidence is the services the side set. */
export const callSync = (call: () => unknown): unknown => {
  const context = syncContext();
  const base = Context.getOrUndefined(context, KERNEL) ?? ROOT_CTX;
  return inCtx(hosted(base, context), () => runIn(call()));
};

/** A Sync stage of Effect's side that Flick calls: the services it reads are those of the evidence. */
export const inSyncContext = (call: () => unknown): unknown => {
  const context = hostOf(currentCtx());
  return syncIn(context, () => Effect.runSyncWith(context)(Effect.sync(call)));
};

durationHooks.millis = (d) => {
  if (d instanceof Duration) return d.millis;
  if (EDuration.isDuration(d)) return EDuration.toMillis(d);
  throw new TypeError("not a duration of this runtime or of Effect");
};
durationHooks.native = (d) => {
  const converted = durationFromEffect(d);
  if (converted instanceof Duration) return converted;
  throw new TypeError("not a duration of this runtime or of Effect");
};
dateTimeHooks.makeUtc = (millis) => EDateTime.makeUnsafe(millis);


export { Raise, asExit, exitOf, raiseOf };

let initialized = false;
export const initializeBridge = (): void => {
  if (initialized) return;
  initialized = true;
bridgeKernel.fromEffect = fromEffect;
bridgeKernel.callEffect = callEffect;
bridgeKernel.causeFromEffect = causeFromEffect;
bridgeKernel.causeToEffect = causeToEffect;
  installDataBridge();
  installLayerBridge();
  installSchemaBridge();
};
initializeBridge();
