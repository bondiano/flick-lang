// Running tasks by a schedule: repeat, retry and scheduleFrom, as Go's
// `flickrt` has them. Each step of the schedule reads the clock, and the delay
// it chooses is a wait.
import { type Option, none, some } from "../core/data.ts";
import { type Cause, type Fail, fail, isFail, raise, unwinds } from "./cause.ts";
import { clock } from "./clock.ts";
import { sleep } from "./duration.ts";
import { type Run, Under, asExit, caught, currentCtx, extend } from "./fiber.ts";
import { DEFAULT_METADATA, METADATA, type Metadata, type Schedule, counter, outOf } from "./effect/Schedule.ts";

/** A step of a run of a schedule: its metadata, or with `done` the last output. */
class Stepped {
  readonly meta: Metadata;
  readonly done: boolean;
  constructor(meta: Metadata, done: boolean) {
    this.meta = meta;
    this.done = done;
  }
}

const steppedOf = (value: unknown): Stepped => {
  if (value instanceof Stepped) return value;
  throw new TypeError("not a step of a schedule");
};

/** A run that reads the clock for each input and waits the step's delay. */
const stepper = (s: Schedule): ((input: unknown) => Run) => {
  const step = s.start();
  const count = counter();
  return function* (input): Run {
    const now = clock().millis();
    const [output, delay, done] = outOf(yield step(now, input));
    if (done) return new Stepped({ ...DEFAULT_METADATA, output }, true);
    const m: Metadata = { ...count(now, input), output, duration: delay };
    yield sleep(delay);
    return new Stepped(m, false);
  };
};

/** `task` with `m` as the metadata `Schedule.current_metadata` reads. */
const withMetadata = (m: Metadata, task: () => unknown): Under => new Under(extend(currentCtx(), METADATA, m), task);

/** The first error of a cause that holds only typed errors. */
const errorOnly = (cause: Cause): Fail | undefined => {
  if (cause.reasons.length === 0 || unwinds(cause) || !cause.reasons.every(isFail)) return undefined;
  return cause.reasons.find(isFail);
};

/**
 * Runs `task` again while the schedule, given each result, goes on: the
 * schedule's last output. A typed failure goes to `orElse`, with the metadata
 * of the last step if one ran.
 */
function* repeating(s: Schedule, orElse: (error: unknown, last: Option<Metadata>) => unknown, task: () => unknown): Run {
  const step = stepper(s);
  let meta = DEFAULT_METADATA;
  const exit = asExit(
    yield caught(function* (): Run {
      for (;;) {
        const value: unknown = yield withMetadata(meta, task);
        const { meta: m, done } = steppedOf(yield step(value));
        if (done) return m.output;
        meta = m;
      }
    }),
  );
  if (exit.cause === undefined) return exit.value;
  const error = errorOnly(exit.cause);
  if (error === undefined) return raise(exit.cause);
  return orElse(error.error, meta.attempt === 0 ? none : some(meta));
}

/** Runs `task` again after each typed failure while the schedule, given the error, goes on; then `orElse` of the last error and the output. */
function* retrying(s: Schedule, orElse: (error: unknown, output: unknown) => unknown, task: () => unknown): Run {
  const step = stepper(s);
  let meta = DEFAULT_METADATA;
  for (;;) {
    const exit = asExit(yield caught(() => withMetadata(meta, task)));
    if (exit.cause === undefined) return exit.value;
    const error = errorOnly(exit.cause);
    if (error === undefined) return raise(exit.cause);
    const { meta: m, done } = steppedOf(yield step(error.error));
    if (done) return orElse(error.error, m.output);
    meta = m;
  }
}

/** Steps the schedule with `initial`, then runs `task` and steps it with each result until it ends: its last output. */
function* scheduledFrom(s: Schedule, initial: unknown, task: () => unknown): Run {
  const step = stepper(s);
  let stepped = steppedOf(yield step(initial));
  while (!stepped.done) stepped = steppedOf(yield step(yield withMetadata(stepped.meta, task)));
  return stepped.meta.output;
}

const failWith = (error: unknown): never => raise(fail(error));

export const retry =
  (s: Schedule) =>
  (task: () => unknown): Run =>
    retrying(s, failWith, task);

export const retryOrElse =
  (s: Schedule, orElse: (error: unknown, output: unknown) => unknown) =>
  (task: () => unknown): Run =>
    retrying(s, orElse, task);

export const repeat =
  (s: Schedule) =>
  (task: () => unknown): Run =>
    repeating(s, failWith, task);

export const repeatOrElse =
  (s: Schedule, orElse: (error: unknown, last: Option<Metadata>) => unknown) =>
  (task: () => unknown): Run =>
    repeating(s, orElse, task);

const scheduleIt =
  (s: Schedule) =>
  (task: () => unknown): Run =>
    scheduledFrom(s, undefined, task);
export { scheduleIt as schedule };

export const scheduleFrom =
  (initial: unknown, s: Schedule) =>
  (task: () => unknown): Run =>
    scheduledFrom(s, initial, task);
