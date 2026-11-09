// Effect contexts and scopes corresponding to native evidence and owners.
import { Context, Effect, Exit as EExit, Scope as EScope } from "effect";
import { type Ctx, Instruction, currentCtx, currentFiber, hosted, inCtx, isRun, inNativeFiber } from "@flick-lang/runtime/rt/fiber";
import { type Exit } from "@flick-lang/runtime/rt/cause";
import { HOST } from "@flick-lang/runtime/rt/layer";
import { type Owner, addTo } from "@flick-lang/runtime/rt/owner";
import { CurrentOwner, Owner as HostOwner, close as closeHost } from "./owner.ts";
import { syncContext } from "./sync.ts";
import { updatedContext } from "./bridge_context.ts";
import { dataFromEffect, dataToEffect } from "./bridge_data.ts";
import { fromEffect, callEffect, causeToEffect } from "./bridge_runtime.ts";

export const KERNEL = /*#__PURE__*/ Context.Service<Ctx>("flick/bridge/KernelCtx");
const EMPTY = /*#__PURE__*/ Context.empty();
const ROOT_CTX = /*#__PURE__*/ currentCtx();
const contextOf = (u: unknown): Context.Context<never> | undefined => Context.isContext(u) ? u : undefined;

/**
 * The kernel's services as Effect's code would call them: a pure result stays
 * a value, while a task is run on the kernel with the Effect fiber's context.
 */
const opOut = (f: unknown): unknown =>
  typeof f !== "function"
    ? f
    : (...args: ReadonlyArray<unknown>): unknown => {
        const context = syncContext();
        const base = Context.getOrUndefined(context, KERNEL) ?? ROOT_CTX;
        const out: unknown = inCtx(hosted(base, context), (): unknown => Reflect.apply(f, undefined, args.map(dataFromEffect)));
        if (typeof out === "function") return opOut(out);
        return isRun(out) || out instanceof Instruction ? Effect.map(callEffect(() => out), dataToEffect) : dataToEffect(out);
      };

const implOut = (impl: unknown): unknown =>
  new Proxy(typeof impl === "object" && impl !== null ? impl : {}, {
    get: (target, name) => opOut(Reflect.get(target, name)),
  });

/** The Context of the evidence `ctx`: Effect's own, with the kernel's services in, computed once per node. */
const rawHostOf = (ctx: Ctx | null): Context.Context<never> => {
  if (ctx === null) return EMPTY;
  const cached = contextOf(ctx.bridged);
  if (cached !== undefined) return cached;
  const below = ctx.next;
  let out: Context.Context<never>;
  if (ctx.host !== below?.host) out = contextOf(ctx.host) ?? EMPTY;
  else if (ctx.owner !== below?.owner && ctx.owner !== undefined) out = Context.add(rawHostOf(below), CurrentOwner, hostOwnerOf(ctx.owner));
  else if (ctx.tag !== null && Object.isFrozen(ctx.tag) && ctx.tag !== MEMO_TAG(ctx.tag)) {
    out = Context.addUnsafe(rawHostOf(below), String(Reflect.get(ctx.tag, "key")), implOut(ctx.impl));
  } else out = rawHostOf(below);
  ctx.bridged = out;
  return out;
};
export const hostOf = (ctx: Ctx | null): Context.Context<never> => inNativeFiber() ? updatedContext(currentFiber(), ctx, rawHostOf(ctx)) : rawHostOf(ctx);
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
  addTo(owner, (exit: Exit) => fromEffect(closeHost(made, exit.cause === undefined ? EExit.void : EExit.failCause(causeToEffect(exit.cause)))), true);
  return made;
};

/** What Effect's scope of one crossing holds goes on the owner's stack, at its place in the order. */
export const settled = (owner: Owner | undefined, scope: EScope.Closeable): unknown => {
  if (owner === undefined || scope.state._tag === "Empty") return undefined;
  return addTo(
    owner,
    (exit: Exit) => fromEffect(EScope.close(scope, exit.cause === undefined ? EExit.void : EExit.failCause(causeToEffect(exit.cause)))),
    true,
  );
};

/** What a kernel layer built, `[tag, impl]` pairs, as Effect's Context. */
export const servicesOut = (services: unknown): Context.Context<never> => {
  const map = new Map<string, unknown>();
  const entries: ReadonlyArray<unknown> = Array.isArray(services) ? services : [];
  for (const entry of entries) {
    if (!Array.isArray(entry)) continue;
    const tag: unknown = entry[0];
    const impl: unknown = entry[1];
    const inherited = tag === HOST ? contextOf(impl) : undefined;
    if (inherited !== undefined) {
      for (const [key, value] of inherited.mapUnsafe) map.set(key, value);
    } else map.set(String(Reflect.get(Object(tag), "key")), implOut(impl));
  }
  return Context.makeUnsafe(map);
};

