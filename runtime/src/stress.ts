// Stress (`just stress`): with FLICK_STRESS set to a seed, the runner
// runs main under a scheduler that shuffles the tasks of each round, within
// a priority, and makes fibers yield at random, so the runs of a program
// take interleavings a plain run never does; the monitor of λRT checks the
// trace of each. The same seed gives the same shuffles.
import { Effect, type Fiber, Scheduler } from "effect";

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

class ShufflingDispatcher implements Scheduler.SchedulerDispatcher {
  #tasks: Array<readonly [priority: number, order: number, task: () => void]> = [];
  #scheduled = false;
  readonly #random: () => number;

  constructor(random: () => number) {
    this.#random = random;
  }

  scheduleTask(task: () => void, priority: number): void {
    this.#tasks.push([priority, this.#random(), task]);
    if (!this.#scheduled) {
      this.#scheduled = true;
      setImmediate(() => this.#run());
    }
  }

  #run(): void {
    this.#scheduled = false;
    const tasks = this.#tasks.toSorted((a, b) => a[0] - b[0] || a[1] - b[1]);
    this.#tasks = [];
    for (const [, , task] of tasks) task();
  }

  flush(): void {
    while (this.#tasks.length > 0) this.#run();
  }
}

class ShufflingScheduler implements Scheduler.Scheduler {
  readonly executionMode = "async";
  readonly #base = new Scheduler.MixedScheduler("async");
  readonly #random: () => number;

  constructor(random: () => number) {
    this.#random = random;
  }

  shouldYield(fiber: Fiber.Fiber<unknown, unknown>): boolean {
    return this.#base.shouldYield(fiber) || this.#random() < 0.05;
  }

  makeDispatcher(): Scheduler.SchedulerDispatcher {
    return new ShufflingDispatcher(this.#random);
  }
}

/** `program` under the shuffling scheduler of FLICK_STRESS, if it is set. */
export const stressed = <A, E>(program: Effect.Effect<A, E>): Effect.Effect<A, E> => {
  const seed = env("FLICK_STRESS");
  if (seed === undefined || seed === "") return program;
  return Effect.provideService(program, Scheduler.Scheduler, new ShufflingScheduler(seeded(Number(seed) || 1)));
};
