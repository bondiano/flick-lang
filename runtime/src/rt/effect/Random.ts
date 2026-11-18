// Each fiber reads its random source from context. `withSeed` installs a fresh
// ISAAC stream for the task, so identical seeds replay the same draws.
import { currentCtx, extend, lookup, Under } from "../fiber.ts";
import { isaac } from "../isaac.ts";

interface Generator { nextIntUnsafe(): number; nextDoubleUnsafe(): number }
const RANDOM: object = /*#__PURE__*/ Object.freeze({ key: "rt/random" });
const live: Generator = {
  nextIntUnsafe: () => Math.floor(Math.random() * 0x20000000000000) - Number.MAX_SAFE_INTEGER,
  nextDoubleUnsafe: () => Math.random(),
};
const isGenerator = (source: unknown): source is Generator =>
  typeof source === "object" && source !== null
  && typeof Reflect.get(source, "nextIntUnsafe") === "function"
  && typeof Reflect.get(source, "nextDoubleUnsafe") === "function";
const generator = (): Generator => {
  const source = lookup(RANDOM);
  return isGenerator(source) ? source : live;
};
const draw = (): number => generator().nextDoubleUnsafe();
export const flickNext = (): number => draw();
export const withSeed = (seed: number) => (task: () => unknown): Under =>
  new Under(extend(currentCtx(), RANDOM, isaac(seed)), task);
