// Each fiber reads its random source from context. `withSeed` installs a fresh
// ISAAC stream for the task, so identical seeds replay the same draws.
import { currentCtx, extend, lookup, Under } from "../fiber.ts";
import { isaac } from "../isaac.ts";

// every operation of `Host.Random` is made of `next` draws, so a source only draws doubles
interface Generator { nextDoubleUnsafe(): number }
const RANDOM: object = /*#__PURE__*/ Object.freeze({ key: "rt/random" });
const isGenerator = (source: unknown): source is Generator =>
  typeof source === "object" && source !== null && typeof Reflect.get(source, "nextDoubleUnsafe") === "function";
export const flickNext = (): number => {
  const source = lookup(RANDOM);
  return isGenerator(source) ? source.nextDoubleUnsafe() : Math.random();
};
export const withSeed = (seed: number) => (task: () => unknown): Under =>
  new Under(extend(currentCtx(), RANDOM, isaac(seed)), task);
