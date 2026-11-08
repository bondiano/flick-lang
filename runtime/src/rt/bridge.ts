// spike: the bridge between the kernel and Effect. Effect's Context travels on
// the kernel's evidence (`Ctx.host`); the kernel's evidence travels in Effect's
// Context under `KERNEL`. A crossing that ends at once takes no suspension.
import { Cause as ECause, Context, Duration as EDuration, Effect, Exit as EExit, Layer as ELayer, Option as EOption, Result as EResult, Scheduler, Scope as EScope } from "effect";

import { Failure as HostFailure } from "../cause.ts";
import { failureCause, flickExit, nativeExit } from "../stdlib/outcome.ts";
import { syncContext, syncIn } from "../sync.ts";
import { type Cause, type Exit, Raise, failed, fromReasons, raise, succeeded } from "./cause.ts";
import { type Ctx, type Fiber, type Run, Suspend, currentCtx, currentFiber, forkNow, hosted, inCtx, asExit, exitOf, lookup, raiseOf, runIn } from "./fiber.ts";
import { Duration } from "./duration.ts";
import { Failure } from "./outcome.ts";
import { bridgeHooks } from "./handle.ts";
import { HOST, Layer, MEMO as MEMO_KEY, MemoMap, layerHooks, memoizable } from "./layer.ts";
import { Owner, addTo, close, inside } from "./owner.ts";
import { none, some, useHostData, useHostEquality } from "../core/data.ts";
import { equals as hostEquals, hash as hostHash } from "../data.ts";
import { CurrentOwner, Owner as HostOwner, close as closeHost } from "../owner.ts";
import { durationHooks } from "./duration.ts";

interface EffectFiber {
  readonly id: number;
  readonly context: Context.Context<never>;
  readonly interruptible: boolean;
  readonly _exit: EExit.Exit<unknown, unknown> | undefined;
  addObserver(f: (exit: EExit.Exit<unknown, unknown>) => void): () => void;
  interruptUnsafe(by?: number): void;
}

const KERNEL = /*#__PURE__*/ Context.Service<Ctx>("flick/bridge/KernelCtx");
const EMPTY = /*#__PURE__*/ Context.empty();

/** Effect's cause as the kernel's: the reasons have the same fields. */
const causeIn = (cause: ECause.Cause<unknown>): Cause => fromReasons(cause.reasons as never);

/** The kernel's cause as Effect's. */
const causeOut = (cause: Cause): ECause.Cause<unknown> =>
  ECause.fromReasons(
    cause.reasons.map((r) =>
      r._tag === "Fail" ? ECause.makeFailReason(r.error) : r._tag === "Die" ? ECause.makeDieReason(r.defect) : ECause.makeInterruptReason(r.fiberId),
    ),
  );

const exitValue = (exit: EExit.Exit<unknown, unknown>): unknown => {
  if (exit._tag === "Success") return exit.value;
  return raise(causeIn(exit.cause));
};

/**
 * The kernel's services as Effect's code would call them: every call an Effect.
 * shortcut: from the outside, by what an op gives; the compiler knows each op's stage, and would derive it.
 */
const opOut = (f: unknown): unknown =>
  typeof f !== "function"
    ? f
    : (...args: ReadonlyArray<unknown>): unknown => {
        const out: unknown = Reflect.apply(f, undefined, args);
        if (typeof out === "function") return opOut(out);
        return callEffect(() => out);
      };

const implOut = (impl: unknown): unknown =>
  new Proxy(Object(impl) as object, {
    get: (target, name) => opOut(Reflect.get(target, name)),
  });

/** The Context of the evidence `ctx`: Effect's own, with the kernel's services in, computed once per node. */
const hostOf = (ctx: Ctx | null): Context.Context<never> => {
  if (ctx === null) return EMPTY as Context.Context<never>;
  if (ctx.bridged !== undefined) return ctx.bridged as Context.Context<never>;
  const below = ctx.next;
  let out: Context.Context<never>;
  if (ctx.host !== below?.host) out = ctx.host as Context.Context<never>;
  else if (ctx.owner !== below?.owner && ctx.owner !== undefined) out = Context.add(hostOf(below), CurrentOwner, hostOwnerOf(ctx.owner) as never);
  else if (ctx.tag !== null && Object.isFrozen(ctx.tag) && ctx.tag !== MEMO_TAG(ctx.tag)) {
    out = Context.add(hostOf(below), ctx.tag as never, implOut(ctx.impl) as never);
  } else out = hostOf(below);
  ctx.bridged = out;
  return out;
};
const MEMO_TAG = (tag: object): object | undefined => (Reflect.get(tag, "key") === "effect/Layer/CurrentMemoMap" ? tag : undefined);

/**
 * The owner Effect's side of the runtime sees for a kernel's owner: one per owner, closed with it.
 * shortcut: its place on the owner's stack is that of the first crossing, not of each acquire; a crossing's own `Scope` keeps the order.
 */
const hostOwners = /*#__PURE__*/ new WeakMap<Owner, HostOwner>();
const hostOwnerOf = (owner: Owner): HostOwner => {
  const found = hostOwners.get(owner);
  if (found !== undefined) return found;
  const made = new HostOwner(EScope.makeUnsafe());
  hostOwners.set(owner, made);
  addTo(owner, (exit: Exit) => fromEffect(closeHost(made, exit.cause === undefined ? EExit.void : EExit.failCause(causeOut(exit.cause)))), true);
  return made;
};

/** What Effect's scope of one crossing holds goes on the owner's stack, at its place in the order. */
const settled = (owner: Owner | undefined, scope: EScope.Closeable): unknown => {
  if (owner === undefined || scope.state._tag === "Empty") return undefined;
  return addTo(
    owner,
    (exit: Exit) => fromEffect(EScope.close(scope, exit.cause === undefined ? EExit.void : EExit.failCause(causeOut(exit.cause)))),
    true,
  );
};

/** Waits for an Effect fiber that did not end at once; an interruption is passed on and waits for it to end. */
function* awaiting(ef: EffectFiber, owner: Owner | undefined, scope: EScope.Closeable): Run {
  const fiber = currentFiber();
  let exit: EExit.Exit<unknown, unknown>;
  try {
    exit = (yield new Suspend((resolve) => {
      ef.addObserver(resolve);
    })) as EExit.Exit<unknown, unknown>;
  } catch (thrown) {
    fiber.mask++;
    try {
      ef.interruptUnsafe(fiber.id);
      if (ef._exit === undefined) {
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
  return exitValue(exit);
}

const FAST = !process.env["FLICK_BRIDGE_NO_FAST"];

/**
 * spike: an Effect run from the kernel keeps its turn until it waits for something. Its fiber
 * gives its turn after 2048 operations otherwise, and a Sync stage that called it cannot wait
 * for that turn to come back. FLICK_BRIDGE_YIELD=1 lets it give its turn as before.
 */
const NO_YIELD = !process.env["FLICK_BRIDGE_YIELD"];

/** spike: crossings by kind, written to stderr at exit when FLICK_BRIDGE_COUNT is set. */
export const crossings = { fromEffect: 0, fromEffectFast: 0, fromEffectRun: 0, callEffect: 0, callSync: 0, hostOp: 0, adopt: 0 };
if (process.env["FLICK_BRIDGE_COUNT"]) process.on("exit", () => process.stderr.write(`[bridge] ${JSON.stringify(crossings)}\n`));
const COUNT = Boolean(process.env["FLICK_BRIDGE_COUNT"]);

/** spike: with FLICK_BRIDGE_COUNT=sites, full runs and kernel calls by the compiled frame that made them. */
const SITES = process.env["FLICK_BRIDGE_COUNT"] === "sites";
const sites = new Map<string, number>();
const site = (kind: string): void => {
  const limit = Error.stackTraceLimit;
  Error.stackTraceLimit = 40;
  const stack = new Error().stack ?? "";
  Error.stackTraceLimit = limit;
  const frame = stack
    .split("\n")
    .slice(1)
    .find((l) => !l.includes("/runtime/dist/") && !l.includes("/node_modules/"));
  const at = `${kind} ${frame?.trim().replace(/^at /, "").replace(/file:\/\/\S*\/(dist\/[^/]+\/)/, "$1") ?? "?"}`;
  sites.set(at, (sites.get(at) ?? 0) + 1);
};
if (SITES)
  process.on("exit", () => {
    for (const [at, n] of [...sites].toSorted((a, b) => b[1] - a[1])) process.stderr.write(`[bridge] ${n}\t${at}\n`);
  });

/** An Effect as a call of the kernel: runs at once, on this stack, as far as it goes. */
export const fromEffect = (effect: unknown): unknown => {
  // spike: what is not an Effect is already a value or a kernel call
  if (!Effect.isEffect(effect)) return effect;
  if (COUNT) crossings.fromEffect++;
  if (FAST) {
    if (COUNT && ((effect as { readonly _tag?: string })._tag === "Success" || (effect as { readonly _tag?: string })._tag === "Failure")) crossings.fromEffectFast++;
    const tag = (effect as { readonly _tag?: string })._tag;
    if (tag === "Success") return (effect as unknown as { readonly value: unknown }).value;
    if (tag === "Failure") return raise(causeIn((effect as unknown as { readonly cause: ECause.Cause<unknown> }).cause));
  }
  if (COUNT) crossings.fromEffectRun++;
  if (SITES) site("fromEffect");
  const ctx = currentCtx();
  const scope = EScope.makeUnsafe();
  let services = Context.add(Context.add(hostOf(ctx), KERNEL, ctx), EScope.Scope, scope);
  if (NO_YIELD) services = Context.add(services, Scheduler.PreventSchedulerYield, true);
  const fiber = currentFiber();
  const ef = Effect.runForkWith(services)(effect as Effect.Effect<unknown, unknown, never>, { uninterruptible: fiber.mask > 0 }) as unknown as EffectFiber;
  const exit = ef._exit;
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

const DEBUG = Boolean(process.env["FLICK_BRIDGE_DEBUG"]);
const exitOut = (exit: Exit): Effect.Effect<unknown, unknown> => {
  if (DEBUG && exit.cause !== undefined) console.error("[bridge] kernel exit", exit.cause.reasons);
  return exit.cause === undefined ? Effect.succeed(exit.value) : Effect.failCause(causeOut(exit.cause));
};

const ROOT_CTX = /*#__PURE__*/ currentCtx();

/** A kernel call as an Effect: a fiber of the kernel per run, with the evidence of the Effect fiber that runs it. */
export const callEffect = (call: () => unknown): Effect.Effect<unknown, unknown> => {
  if (SITES) site("callEffect");
  return Effect.withFiber((ef) => {
    if (COUNT) crossings.callEffect++;
    const context = ef.context as Context.Context<never>;
    const base = Context.getOrUndefined(context, KERNEL) ?? ROOT_CTX;
    const fiber: Fiber = forkNow(call, hosted(base, context), (ef as unknown as EffectFiber).interruptible ? 0 : 1);
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
};

/** A Sync stage of Flick that Effect's side calls and that gives its value: the evidence is the services the side set. */
export const callSync = (call: () => unknown): unknown => {
  if (COUNT) crossings.callSync++;
  const context = syncContext();
  const base = Context.getOrUndefined(context, KERNEL) ?? ROOT_CTX;
  return inCtx(hosted(base, context), () => runIn(call()));
};

/** A Sync stage of Effect's side that Flick calls: the services it reads are those of the evidence. */
export const inSyncContext = (call: () => unknown): unknown => syncIn(hostOf(currentCtx()), call);

/** A record of handlers the signature does not show: each function in it, as Effect's side calls one. */
export const handlersToEffect = (handlers: unknown): unknown => {
  if (typeof handlers === "function") return (x: unknown) => callEffect(() => (handlers as (x: unknown) => unknown)(x));
  if (typeof handlers !== "object" || handlers === null) return handlers;
  return Object.fromEntries(Object.entries(handlers).map(([k, v]) => [k, handlersToEffect(v)]));
};

export const optionMapped = (value: { readonly _tag: string; readonly value?: unknown }, f: (a: unknown) => unknown): unknown =>
  value._tag === "Some" ? some(f(value.value)) : none;

// `Exit`, `Failure` at the boundary: the kernel's and Effect's are different classes.
export const exitToEffect = (exit: Exit): unknown =>
  flickExit(exit.cause === undefined ? EExit.succeed(exit.value) : EExit.failCause(causeOut(exit.cause)));
export const exitFromEffect = (exit: never): Exit => {
  const native = nativeExit<unknown, unknown>(exit);
  return native._tag === "Success" ? succeeded(native.value) : failed(causeIn(native.cause));
};
export const failureToEffect = (failure: Failure): unknown => new HostFailure(causeOut(failure.cause));
export const failureFromEffect = (failure: HostFailure): Failure => new Failure(causeIn(failureCause(failure)));
export const resultMapped = (
  value: { readonly _tag: string; readonly success?: unknown; readonly failure?: unknown },
  f: (a: unknown) => unknown,
  g: (e: unknown) => unknown,
): unknown => (value._tag === "Success" ? EResult.succeed(f(value.success)) : EResult.fail(g(value.failure)));

/** One memo map of Effect per memo map of the kernel: an Effect layer shared inside Effect is built once. */
const memos = /*#__PURE__*/ new WeakMap<MemoMap, ELayer.MemoMap>();
const memoOf = (memo: MemoMap): ELayer.MemoMap => {
  const found = memos.get(memo);
  if (found !== undefined) return found;
  const made = memo.parent === undefined ? ELayer.makeMemoMapUnsafe() : ELayer.forkMemoMapUnsafe(memoOf(memo.parent));
  memos.set(memo, made);
  return made;
};

const adopted = /*#__PURE__*/ new WeakMap<object, Layer>();

/** An Effect layer as the kernel's: built into a scope of Effect that the owner closes, its services under `HOST`. */
const adopt = (layer: unknown): Layer => {
  if (layer instanceof Layer) return layer;
  const found = adopted.get(layer as object);
  if (found !== undefined) return found;
  const made = memoizable(function* (memo: MemoMap, into: Owner): Run {
    if (COUNT) crossings.adopt++;
    const scope = EScope.makeUnsafe();
    yield addTo(
      into,
      (exit: Exit) => fromEffect(EScope.close(scope, exit.cause === undefined ? EExit.void : EExit.failCause(causeOut(exit.cause)))),
      true,
    );
    const context: unknown = yield inside(into, () =>
      fromEffect(ELayer.buildWithMemoMap(layer as ELayer.Layer<never>, memoOf(memo), scope)),
    );
    return [[HOST, context]];
  });
  adopted.set(layer as object, made);
  return made;
};

/** A kernel layer as Effect's: built by a fiber of the kernel into an owner the Effect scope closes. */
export const layerToEffect = (layer: unknown): unknown => {
  if (!(layer instanceof Layer)) return layer;
  return (ELayer.effectContext as (e: unknown) => unknown)(
    Effect.flatMap(Effect.scope, (scope) =>
      callEffect(function* (): Run {
        const owner = new Owner();
        EScope.addFinalizerExit(
          scope,
          (exit) =>
            callEffect(() => close(owner, exit._tag === "Success" ? succeeded(undefined) : failed(causeIn(exit.cause)))) as Effect.Effect<unknown, never>,
        ).pipe(Effect.runSync);
        // the memo map of the `provide` around it: a layer shared with the kernel's side is one build
        const outer = lookup(MEMO_KEY);
        const memo = outer instanceof MemoMap ? outer : new MemoMap(undefined);
        const services = (yield inside(owner, () => layer.build(memo, owner))) as ReadonlyArray<readonly [{ readonly key: string }, unknown]>;
        const map = new Map<string, unknown>();
        for (const [tag, impl] of services) map.set(tag.key, implOut(impl));
        return Context.makeUnsafe(map);
      }),
    ),
  );
};

if (!process.env["FLICK_BRIDGE_PLAIN_DATA"]) {
  useHostData(EOption.some as never, EOption.none() as never, EResult.succeed as never, EResult.fail as never);
  useHostEquality(hostEquals, hostHash);
  durationHooks.millis = (d) => (d instanceof Duration ? d.millis : EDuration.toMillis(d as EDuration.Duration));
}

bridgeHooks.operation = (tag) => {
  const host = currentCtx().host as Context.Context<never> | undefined;
  if (COUNT) crossings.hostOp++;
  return host === undefined ? undefined : host.mapUnsafe.get(String(Reflect.get(tag, "key")));
};
bridgeHooks.ran = fromEffect;
layerHooks.adopt = adopt;
layerHooks.merge = (a, b) => (a === undefined ? b : Context.merge(a as Context.Context<never>, b as Context.Context<never>));

export { Raise, asExit, exitOf, raiseOf };
export { label } from "../atoms.ts";
export { refinedParse } from "../schema/index.ts";
