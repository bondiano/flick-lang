// The bindings of `Schedule` on the kernel, as Go's `flickrt` has them:
// each run of a schedule starts a step function, which from the time and an
// input decides the step's output and the delay before the next one, or ends
// the run with a last output. A step runs the schedule's callbacks, so it is a
// call the driver runs.
import { type Option, err, ok } from "../../core/data.ts";
import { absent } from "../builtins.ts";
import { Duration } from "../duration.ts";
import { type Run, lookup } from "../fiber.ts";

/** What a step decides: its output, the delay before the next, or the end of the run. */
export type Out = readonly [output: unknown, delay: Duration, done: boolean];

const isOut = (value: unknown): value is Out => Array.isArray(value) && value.length === 3 && value[1] instanceof Duration;

/** What a step gave, known to be an {@link Out}. */
export const outOf = (value: unknown): Out => {
  if (isOut(value)) return value;
  throw new TypeError("not a step of a schedule");
};

const durationOf = (value: unknown): Duration => {
  if (value instanceof Duration) return value;
  throw new TypeError("not a duration");
};

/** A step of a run: it gives an {@link Out}. */
export type Step = (now: number, input: unknown) => Run;

export class Schedule {
  /** Starts a run. */
  readonly start: () => Step;
  constructor(start: () => Step) {
    this.start = start;
  }
}

const NO_DELAY = /*#__PURE__*/ new Duration(0);

/** What a step of a run is told of itself, a Flick record. */
export interface Metadata {
  readonly input: unknown;
  readonly output: unknown;
  readonly duration: Duration;
  readonly attempt: number;
  readonly start: number;
  readonly now: number;
  readonly elapsed: number;
  readonly elapsedSincePrevious: number;
}

/** The attempts and times of a run's steps. */
export const counter = (): ((now: number, input: unknown) => Metadata) => {
  let n = 0;
  let start = 0;
  let previous = 0;
  return (now, input) => {
    const since = n === 0 ? 0 : now - previous;
    if (n === 0) start = now;
    previous = now;
    n++;
    return { input, output: undefined, duration: NO_DELAY, attempt: n, start, now, elapsed: now - start, elapsedSincePrevious: since };
  };
};

// oxlint-disable-next-line require-yield -- a step without callbacks decides at once
function* decided(out: Out): Run {
  return out;
}

/** A schedule whose steps `f` decides from their metadata. */
const withMeta = (start: () => (m: Metadata) => Out): Schedule =>
  new Schedule(() => {
    const f = start();
    const count = counter();
    return (now, input) => decided(f(count(now, input)));
  });

/** A schedule of `self` whose steps `f` decides from its step's, with that step's metadata. */
const around = (
  self: Schedule,
  f: (step: Step, meta: () => Metadata, now: number, input: unknown) => Run,
): Schedule =>
  new Schedule(() => {
    const step = self.start();
    const count = counter();
    return (now, input) => f(step, () => count(now, input), now, input);
  });

const millis = (ms: number): Duration => (Number.isNaN(ms) || ms === 0 ? NO_DELAY : new Duration(ms));

export const spaced = (d: Duration): Schedule => withMeta(() => (m) => [m.attempt - 1, d, false]);
export const forever: Schedule = /*#__PURE__*/ spaced(NO_DELAY);

const IDENTITY = /*#__PURE__*/ new Schedule(() => (_now, input) => decided([input, NO_DELAY, false]));
export const identity = (): Schedule => IDENTITY;

export const duration = (d: Duration): Schedule =>
  withMeta(() => (m) => (m.attempt === 1 ? [d, d, false] : [NO_DELAY, NO_DELAY, true]));

export const during = (d: Duration): Schedule => withMeta(() => (m) => [millis(m.elapsed), NO_DELAY, m.elapsed > d.millis]);

export const exponential = (base: Duration, factor: number): Schedule =>
  withMeta(() => (m) => {
    const d = millis(base.millis * factor ** (m.attempt - 1));
    return [d, d, false];
  });

/** The first delay is `one`, then 1, 2, 3, 5 times it. */
export const fibonacci = (one: Duration): Schedule =>
  new Schedule(() => {
    let a = 0;
    let b = one.millis;
    return () => {
      const d = millis(b);
      [a, b] = [b, a + b];
      return decided([d, d, false]);
    };
  });

export const fixed = (interval: Duration): Schedule => {
  const window = interval.millis;
  return withMeta(() => {
    let start = 0;
    let lastRun = 0;
    return (m) => {
      if (window === 0) return [m.attempt - 1, NO_DELAY, false];
      if (m.attempt === 1) {
        start = m.now;
        lastRun = m.now + window;
        return [0, millis(window), false];
      }
      const behind = m.now > lastRun + window;
      const boundary = window - ((m.now - start) % window);
      const delay = behind ? 0 : boundary === 0 ? window : boundary;
      lastRun = behind ? m.now : m.now + delay;
      return [m.attempt - 1, millis(delay), false];
    };
  });
};

export const windowed = (interval: Duration): Schedule => {
  const window = interval.millis;
  return withMeta(() => (m) => {
    if (window === 0) return [m.attempt - 1, NO_DELAY, false];
    const delay = window - (m.elapsed % window);
    return [m.attempt - 1, millis(delay === 0 ? window : delay), false];
  });
};

/** Maps each output by `f`, the last output too. */
const mapOutput = (self: Schedule, f: (m: Metadata) => unknown): Schedule =>
  around(self, function* (step, meta, now, input): Run {
    const [output, delay, done] = outOf(yield step(now, input));
    const m: Metadata = { ...meta(), output, duration: done ? NO_DELAY : delay };
    return [yield f(m), delay, done];
  });

/** Recurs while `f` holds of a step's metadata. */
const whileMeta = (self: Schedule, f: (m: Metadata) => unknown): Schedule =>
  around(self, function* (step, meta, now, input): Run {
    const [output, delay, done] = outOf(yield step(now, input));
    if (done) return [output, delay, true];
    const holds = (yield f({ ...meta(), output, duration: delay })) === true;
    return [output, delay, !holds];
  });

export const recurs = (times: number): Schedule => whileMeta(forever, (m) => m.attempt <= times);
export const once: Schedule = /*#__PURE__*/ mapOutput(recurs(1), () => undefined);

export const map =
  (f: (m: Metadata) => unknown) =>
  (self: Schedule): Schedule =>
    mapOutput(self, f);

const whileIt =
  (f: (m: Metadata) => unknown) =>
  (self: Schedule): Schedule =>
    whileMeta(self, f);
export { whileIt as while };

export const tap =
  (f: (m: Metadata) => unknown) =>
  (self: Schedule): Schedule =>
    around(self, function* (step, meta, now, input): Run {
      const out = outOf(yield step(now, input));
      const [output, delay, done] = out;
      if (!done) yield f({ ...meta(), output, duration: delay });
      return out;
    });

/** `f` replaces each delay. */
const modified = (self: Schedule, f: (m: Metadata) => unknown): Schedule =>
  around(self, function* (step, meta, now, input): Run {
    const out = outOf(yield step(now, input));
    const [output, delay, done] = out;
    if (done) return out;
    return [output, durationOf(yield f({ ...meta(), output, duration: delay })), false];
  });

export const modifyDelay =
  (f: (m: Metadata) => unknown) =>
  (self: Schedule): Schedule =>
    modified(self, f);

export const addDelay =
  (f: (m: Metadata) => unknown) =>
  (self: Schedule): Schedule =>
    modified(self, function* (m: Metadata): Run {
      const added = durationOf(yield f(m));
      return millis(added.millis + m.duration.millis);
    });

/** Each delay scaled by a random factor in [0.8, 1.2]. */
export const jittered = (self: Schedule): Schedule =>
  modified(self, (m) => {
    const r = Math.random();
    return millis(m.duration.millis * 0.8 * (1 - r) + m.duration.millis * 1.2 * r);
  });

export const passthrough = (self: Schedule): Schedule =>
  around(self, function* (step, _meta, now, input): Run {
    const [, delay, done] = outOf(yield step(now, input));
    return [input, delay, done];
  });

/** `self` to its end, then `other`: outputs `Err` of the first and `Ok` of the second. */
export const concatResult =
  (other: Schedule) =>
  (self: Schedule): Schedule => {
    const left = mapOutput(self, (m) => err(m.output));
    const right = mapOutput(other, (m) => ok(m.output));
    return new Schedule(() => {
      let current: Step | undefined = left.start();
      let first = true;
      return function* (now, input): Run {
        if (first && current !== undefined) {
          const out = outOf(yield current(now, input));
          if (!out[2]) return out;
          first = false;
          current = undefined;
        }
        current ??= right.start();
        return yield current(now, input);
      };
    });
  };

const either = (r: unknown): unknown => Reflect.get(Object(r), Reflect.get(Object(r), "_tag") === "Success" ? "success" : "failure");

export const concat =
  (other: Schedule) =>
  (self: Schedule): Schedule =>
    mapOutput(concatResult(other)(self), (m) => either(m.output));

/** Each step runs every schedule's, and `pick` chooses among the delays, `undefined` where one ended. */
const combined = (schedules: ReadonlyArray<Schedule>, pick: (ds: ReadonlyArray<Duration | undefined>) => Duration | undefined): Schedule =>
  new Schedule(() => {
    const steps = schedules.map((s) => s.start());
    return function* (now, input): Run {
      const delays: Array<Duration | undefined> = [];
      for (const step of steps) {
        const [, delay, done] = outOf(yield step(now, input));
        delays.push(done ? undefined : delay);
      }
      const d = pick(delays);
      return d === undefined ? [NO_DELAY, NO_DELAY, true] : [d, d, false];
    };
  });

export const max = (schedules: ReadonlyArray<Schedule>): Schedule =>
  combined(schedules, (ds) => {
    if (ds.includes(undefined)) return undefined;
    return ds.reduce((most, d) => (d !== undefined && most !== undefined && d.millis > most.millis ? d : most));
  });

export const min = (schedules: ReadonlyArray<Schedule>): Schedule =>
  combined(schedules, (ds) =>
    ds.reduce<Duration | undefined>((least, d) => (d !== undefined && (least === undefined || d.millis < least.millis) ? d : least), undefined),
  );

export const flickUpTo =
  (options: { readonly duration: Option<Duration>; readonly times: Option<number> }) =>
  (self: Schedule): Schedule => {
    const { duration: limit, times } = options;
    return whileMeta(
      self,
      (m) => (times._tag === "None" || m.attempt <= times.value) && (limit._tag === "None" || m.elapsed <= limit.value.millis),
    );
  };

/** The key of the metadata of the step that runs a task again. */
export const METADATA: object = /*#__PURE__*/ Object.freeze({ key: "effect/Schedule/CurrentMetadata" });

export const DEFAULT_METADATA: Metadata = /*#__PURE__*/ Object.freeze({
  input: undefined,
  output: undefined,
  duration: NO_DELAY,
  attempt: 0,
  start: 0,
  now: 0,
  elapsed: 0,
  elapsedSincePrevious: 0,
});

export const flickCurrentMetadata = (): unknown => lookup(METADATA) ?? DEFAULT_METADATA;

// Cron remains on the optional adapter; the compiler routes this placeholder there.
export const cron: (c: unknown) => Schedule = /*#__PURE__*/ absent("Schedule", "cron");
