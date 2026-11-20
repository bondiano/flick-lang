// Shared logical fiber identity across RT <-> Effect-TS bridge crossings so
// fiber-local state and `abort` fiber checks treat bridged crossings as one fiber.
import { Effect, type Fiber as EFiber } from "effect";
import type { Fiber } from "@flick-lang/runtime/rt/fiber";
import { bridgeHooks } from "@flick-lang/runtime/rt/handle";
import { fiberIdentityHook } from "./handlers.ts";

type EffectFiber = EFiber.Fiber<unknown, unknown>;

const nativeIdentities = /*#__PURE__*/ new WeakMap<Fiber, number>();
const effectIdentities = /*#__PURE__*/ new WeakMap<EffectFiber, number>();

export const nativeIdentity = (fiber: Fiber): number => nativeIdentities.get(fiber) ?? -fiber.id;
export const effectIdentity = (fiber: EffectFiber): number => effectIdentities.get(fiber) ?? fiber.id;
export const bindNativeIdentity = (fiber: Fiber, ef: EffectFiber): void => {
  nativeIdentities.set(fiber, effectIdentity(ef));
};
export const bindEffectIdentity = (ef: EffectFiber, id: number): void => {
  effectIdentities.set(ef, id);
};

/** Fiber-local library state keeps its identity across interpreter calls. */
export const fiberIdentity: Effect.Effect<number> = Effect.withFiber((fiber) =>
  Effect.succeed(effectIdentity(fiber)),
);

export const installIdentityBridge = (): void => {
  bridgeHooks.identity = nativeIdentity;
  fiberIdentityHook.current = effectIdentity;
};
