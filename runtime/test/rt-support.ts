// Helpers of the tests of the kernel of `effect = "rt"`.
import { type Exit, Suspend, currentCtx, fork } from "../src/rt/index.ts";

/** Runs `task` in a fiber of its own; how it ended. */
export const root = (task: () => unknown): Promise<Exit> =>
  new Promise((resolve) => {
    fork(task, currentCtx()).observe(resolve);
  });

/** A wait of `millis`. */
export const sleep = (millis: number): Suspend =>
  new Suspend((resolve) => {
    const timer = setTimeout(() => resolve(undefined), millis);
    return () => clearTimeout(timer);
  });
