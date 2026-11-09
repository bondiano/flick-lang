// Scoped references mutate an Effect fiber's Context. Retain a snapshot within
// the native lexical frame; leaving that frame restores the surrounding one.
// Each child inherits its own list, so later writes stay local to each fiber.
import { Context, type Fiber as EffectFiber } from "effect";
import type { Ctx, Fiber } from "@flick-lang/runtime/rt/fiber";

interface Change {
  readonly at: Ctx;
  readonly initial: Context.Context<never>;
  readonly current: Context.Context<never>;
}
const changes = new WeakMap<Fiber, Change[]>();

export const inheritContext = (parent: Fiber, child: Fiber): void => {
  const inherited = changes.get(parent);
  if (inherited === undefined) return;
  changes.set(child, [...inherited]);
};

export const rememberContext = (native: Fiber, at: Ctx, initial: Context.Context<never>, host: EffectFiber.Fiber<unknown, unknown>): void => {
  const current = host.context;
  if (current.mapUnsafe.size === initial.mapUnsafe.size &&
      Array.from(current.mapUnsafe).every(([key, value]) => initial.mapUnsafe.get(key) === value)) return;
  const recorded = changes.get(native) ?? [];
  recorded.push({ at, initial, current });
  changes.set(native, recorded);
};

const activeAt = (ctx: Ctx | null, at: Ctx): boolean => {
  for (let node = ctx; node !== null; node = node.next) {
    if (node === at) return true;
    // An Effect callback supplies its own full Context, including the scoped
    // references already in effect before that callback entered the kernel.
    if (node.host !== node.next?.host) return false;
  }
  return false;
};

export const updatedContext = (native: Fiber, at: Ctx | null, base: Context.Context<never>): Context.Context<never> => {
  const recorded = changes.get(native);
  if (recorded === undefined) return base;
  const services = new Map(base.mapUnsafe);
  for (const change of recorded) {
    if (!activeAt(at, change.at)) continue;
    const current = change.current;
    for (const [key, value] of current.mapUnsafe)
      if (change.initial.mapUnsafe.get(key) !== value) services.set(key, value);
    for (const key of change.initial.mapUnsafe.keys())
      if (!current.mapUnsafe.has(key)) services.delete(key);
  }
  return Context.makeUnsafe(services);
};
