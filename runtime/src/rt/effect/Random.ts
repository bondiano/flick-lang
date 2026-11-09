// Each fiber reads its random source from context. `withSeed` installs a fresh
// ISAAC stream for the task, so identical seeds replay the same draws.
import { item } from "../../internal.ts";
import { type Option } from "../../core/data.ts";
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
export const flickNextBoolean = (): boolean => draw() > 0.5;
export const flickNextInt = (): number => Math.floor(draw() * 0x100000000) - 0x80000000;
export const nextBetween = (min: number, max: number): number => min + draw() * (max - min);
export const flickNextIntBetween = (options: { readonly halfOpen: Option<boolean> }, min: number, max: number): number => {
  const extra = options.halfOpen._tag === "Some" && options.halfOpen.value ? 0 : 1;
  const lower = Math.ceil(min);
  return Math.floor(draw() * (Math.floor(max) - lower + extra)) + lower;
};
export const shuffle = <A>(elements: ReadonlyArray<A>): Array<A> => {
  const result = [...elements];
  for (let i = result.length - 1; i >= 1; i--) {
    const index = Math.min(i, Math.floor(draw() * (i + 1)));
    const value = item(result, i);
    result[i] = item(result, index);
    result[index] = value;
  }
  return result;
};
export const choice = <A>(elements: ReadonlyArray<A>): A =>
  item(elements, Math.min(elements.length - 1, Math.floor(draw() * elements.length)));
export const withSeed = (seed: number) => (task: () => unknown): Under =>
  new Under(extend(currentCtx(), RANDOM, isaac(seed)), task);
