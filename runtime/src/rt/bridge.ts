// spike: the bridge between the kernel and Effect. Effect's Context travels on
// the kernel's evidence (`Ctx.host`); the kernel's evidence travels in Effect's
// Context under `KERNEL`. A crossing that ends at once takes no suspension.
import { Cause as ECause, Context, Effect, Exit as EExit, Layer as ELayer, Option as EOption, Result as EResult, Scope as EScope } from "effect";

import { type Cause, type Exit, Raise, failed, fromReasons, raise, succeeded } from "./cause.ts";
import { type Ctx, type Fiber, type Run, Suspend, currentCtx, currentFiber, forkNow, hosted, asExit, exitOf, raiseOf } from "./fiber.ts";
import { Failure } from "./outcome.ts";
import { bridgeHooks } from "./handle.ts";
import { HOST, Layer, MemoMap, layerHooks, memoizable } from "./layer.ts";
import { Owner, addTo, close, inside } from "./owner.ts";
import { none, some, useHostData } from "../core/data.ts";

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

/** An Effect as a call of the kernel: runs at once, on this stack, as far as it goes. */
export const fromEffect = (effect: unknown): unknown => {
  // spike: what is not an Effect is already a value or a kernel call
  if (!Effect.isEffect(effect)) return effect;
  if (FAST) {
    const tag = (effect as { readonly _tag?: string })._tag;
    if (tag === "Success") return (effect as unknown as { readonly value: unknown }).value;
    if (tag === "Failure") return raise(causeIn((effect as unknown as { readonly cause: ECause.Cause<unknown> }).cause));
  }
  const ctx = currentCtx();
  const scope = EScope.makeUnsafe();
  const services = Context.add(Context.add((ctx.host ?? EMPTY) as Context.Context<never>, KERNEL, ctx), EScope.Scope, scope);
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
export const callEffect = (call: () => unknown): Effect.Effect<unknown, unknown> =>
  Effect.withFiber((ef) => {
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

export const optionMapped = (value: { readonly _tag: string; readonly value?: unknown }, f: (a: unknown) => unknown): unknown =>
  value._tag === "Some" ? some(f(value.value)) : none;

// `Exit`, `Failure` at the boundary: the kernel's and Effect's are different classes.
export const exitToEffect = (exit: Exit): unknown =>
  exit.cause === undefined ? EExit.succeed(exit.value) : EExit.failCause(causeOut(exit.cause));
export const exitFromEffect = (exit: EExit.Exit<unknown, unknown>): Exit =>
  exit._tag === "Success" ? succeeded(exit.value) : failed(causeIn(exit.cause));
export const failureToEffect = (failure: Failure): unknown => causeOut(failure.cause);
export const failureFromEffect = (cause: ECause.Cause<unknown>): Failure => new Failure(causeIn(cause));

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
        EScope.addFinalizerExit(scope, (exit) =>
          callEffect(() => close(owner, exit._tag === "Success" ? succeeded(undefined) : failed(causeIn(exit.cause)))),
        ).pipe(Effect.runSync);
        const services = (yield inside(owner, () => layer.build(new MemoMap(undefined), owner))) as ReadonlyArray<readonly [{ readonly key: string }, unknown]>;
        const map = new Map<string, unknown>();
        for (const [tag, impl] of services) map.set(tag.key, impl);
        return Context.makeUnsafe(map);
      }),
    ),
  );
};

if (!process.env["FLICK_BRIDGE_PLAIN_DATA"]) {
  useHostData(EOption.some as never, EOption.none() as never, EResult.succeed as never, EResult.fail as never);
}

bridgeHooks.operation = (tag) => {
  const host = currentCtx().host as Context.Context<never> | undefined;
  return host === undefined ? undefined : host.mapUnsafe.get(String(Reflect.get(tag, "key")));
};
bridgeHooks.ran = fromEffect;
layerHooks.adopt = adopt;
layerHooks.merge = (a, b) => (a === undefined ? b : Context.merge(a as Context.Context<never>, b as Context.Context<never>));

export { Raise, asExit, exitOf, raiseOf };
