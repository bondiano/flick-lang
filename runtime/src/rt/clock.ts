// The clock fibers read and sleep by: the host's, or the virtual one of a test
// in the evidence, which moves only when the test moves it (the test clock, as
// Go's `flickrt` has it).
import { type Run, Suspend, Tracker, awaitFiber, currentCtx, fork, lookup, tracked, yieldNow } from "./fiber.ts";
import { writeStderr } from "./host.ts";

export interface Clock {
  /** Milliseconds since 1970-01-01 UTC. */
  millis(): number;
  /** Nanoseconds since 1970-01-01 UTC. */
  nanos(): bigint;
  /** Nanoseconds that never go back. */
  monotonic(): bigint;
  /** Calls `wake` in `millis`, finite and positive; what stops the wait. */
  timer(millis: number, wake: () => void): () => void;
}

const live: Clock = {
  millis: () => Date.now(),
  nanos: () => BigInt(Math.round((performance.timeOrigin + performance.now()) * 1e6)),
  monotonic: () => BigInt(Math.round(performance.now() * 1e6)),
  timer: (millis, done) => {
    const timer = setTimeout(done, millis);
    return () => clearTimeout(timer);
  },
};

/** The key of the clock in the evidence. */
export const CLOCK: object = /*#__PURE__*/ Object.freeze({ key: "effect/Clock" });

/** The clock of what runs now. */
export const clock = (): Clock => {
  const found = lookup(CLOCK);
  return found instanceof TestClock ? found : live;
};

const BUSY =
  "A fiber woken by the test clock is still running without suspending, so the test clock stopped waiting for it. " +
  "Sleeps it starts later may be scheduled relative to a later time.";

interface Sleeper {
  readonly at: number;
  readonly wake: () => void;
}

const millisToNanos = (millis: number): bigint => {
  const whole = Math.floor(millis);
  return BigInt(whole) * 1_000_000n + BigInt(Math.floor((millis - whole) * 1e6));
};

/** Wakes a sleeper and waits until what the wake scheduled has run, or a second of real time. */
function* wake(sleeper: Sleeper): Run {
  const t = new Tracker();
  tracked(t, sleeper.wake);
  if (t.pending > 0) {
    yield new Suspend((resolve) => {
      const timer = setTimeout(() => {
        writeStderr(`warning: ${BUSY}\n`);
        resolve(undefined);
      }, 1000);
      t.idle = () => {
        clearTimeout(timer);
        resolve(undefined);
      };
      return () => {
        clearTimeout(timer);
        t.idle = undefined;
      };
    });
  }
  return yieldNow();
}

/** Virtual time from 0: `adjust` and `setTime` move it, waking the sleepers due on the way, in order. */
export class TestClock implements Clock {
  private now = 0;
  private wall = 0n;
  private mono = 0n;
  /** By descending time: the next to wake is last. */
  private readonly sleeps: Array<Sleeper> = [];
  /** The fibers waiting for the adjustment that runs now to end. */
  private queue: Array<() => void> | undefined = undefined;

  millis(): number {
    return this.now;
  }
  nanos(): bigint {
    return this.wall;
  }
  monotonic(): bigint {
    return this.mono;
  }

  timer(millis: number, wakeUp: () => void): () => void {
    const sleeper: Sleeper = { at: this.now + millis, wake: wakeUp };
    // before the sleepers of the same time, so that those wake in the order they came
    let low = 0;
    let high = this.sleeps.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if ((this.sleeps[mid]?.at ?? 0) > sleeper.at) low = mid + 1;
      else high = mid;
    }
    this.sleeps.splice(low, 0, sleeper);
    return () => {
      const i = this.sleeps.indexOf(sleeper);
      if (i >= 0) this.sleeps.splice(i, 1);
    };
  }

  private advanceTo(at: number): void {
    const delta = at - this.now;
    if (delta > 0 && Number.isFinite(delta)) this.mono += BigInt(Math.round(delta * 1e6));
    if (Number.isFinite(at)) this.wall = millisToNanos(at);
    this.now = at;
  }

  /** Moves the clock to `step` of its time, one adjustment at a time; `nanos` is an adjustment's exact length. */
  *run(step: (now: number) => number, nanos: bigint | undefined): Run {
    while (this.queue !== undefined) {
      const queue = this.queue;
      yield new Suspend((resolve) => {
        const go = (): void => resolve(undefined);
        queue.push(go);
        return () => {
          const i = queue.indexOf(go);
          if (i >= 0) queue.splice(i, 1);
        };
      });
    }
    const queue: Array<() => void> = [];
    this.queue = queue;
    try {
      // the fibers started before the adjustment register their sleeps first
      yield awaitFiber(fork(yieldNow, currentCtx()));
      const [wall, mono] = [this.wall, this.mono];
      const end = step(this.now);
      for (let next = this.sleeps.at(-1); next !== undefined && next.at <= end; next = this.sleeps.at(-1)) {
        this.sleeps.pop();
        this.advanceTo(next.at);
        yield wake(next);
      }
      this.advanceTo(end);
      if (nanos !== undefined && Number.isFinite(end)) {
        this.wall = wall + nanos;
        if (nanos > 0n) this.mono = mono + nanos;
      }
    } finally {
      this.queue = undefined;
      // the first of them to run takes the next turn, and the others wait for it
      for (const go of queue) go();
    }
    return undefined;
  }
}

/** The test clock of what runs now. */
export const currentTestClock = (): TestClock => {
  const found = lookup(CLOCK);
  if (found instanceof TestClock) return found;
  throw new Error("a test moves only the clock of its own `Test.clock_test`");
};
