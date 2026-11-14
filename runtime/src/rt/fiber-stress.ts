import { env } from "./host.ts";

/** A generator of numbers in [0, 1) from a seed (mulberry32). */
const seeded = (seed: number): (() => number) => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
};

/**
 * Stress (`just stress`): with FLICK_STRESS set to a seed, each round of tasks
 * is shuffled and fibers give their turn at random, so that runs take
 * interleavings a plain run never does. The same seed gives the same choices.
 */
export const stressing = (): (() => number) | undefined => {
  const seed = env("FLICK_STRESS");
  return seed === undefined || seed === "" ? undefined : seeded(Number(seed) || 1);
};
