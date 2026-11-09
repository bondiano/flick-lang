// Effect's `ExecutionPlan` for the bindings of the standard library, and
// the adapters that build its steps one option at a time and hand its
// events to Flick as values of `ExecutionPlan.Event`.
import { type Duration, Effect, ExecutionPlan, type Layer, type Schedule, Stream } from "effect";

import { toFailure } from "./Cause.ts";
import { tag } from "../tags.ts";

export * from "effect/ExecutionPlan";

/** One step of a plan, as `ExecutionPlan.make` takes it. */
interface Step {
  readonly provide: Layer.Any;
  readonly attempts?: number;
  readonly schedule?: Schedule.Schedule<unknown, unknown, unknown, unknown>;
  readonly while?: (input: unknown) => Effect.Effect<boolean, unknown, unknown>;
}

type Plan = ExecutionPlan.ExecutionPlan<{ provides: unknown; input: unknown; error: unknown; requirements: unknown }>;

export const flickStep = (provide: Layer.Any): Step => ({ provide });
export const flickWithAttempts =
  (attempts: number) =>
  (step: Step): Step => ({ ...step, attempts });
export const flickWithSchedule =
  (schedule: Schedule.Schedule<unknown, unknown, unknown, unknown>) =>
  (step: Step): Step => ({ ...step, schedule });
export const flickWithWhile =
  (f: (input: unknown) => Effect.Effect<boolean, unknown, unknown>) =>
  (step: Step): Step => ({ ...step, while: f });

export const flickMake = (steps: readonly [Step, ...Array<Step>]): Plan => ExecutionPlan.make(...steps);
export const flickMerge = (plans: readonly [Plan, ...Array<Plan>]): Plan => ExecutionPlan.merge(...plans);

export const flickCurrentMetadata = (): Effect.Effect<ExecutionPlan.Metadata> => Effect.service(ExecutionPlan.CurrentMetadata);

const START = /*#__PURE__*/ tag("ExecutionPlan.Event.AttemptStart");
const SUCCESS = /*#__PURE__*/ tag("ExecutionPlan.Event.AttemptSuccess");
const FAILURE = /*#__PURE__*/ tag("ExecutionPlan.Event.AttemptFailure");

interface Attempt {
  readonly attempt: number;
  readonly stepAttempt: number;
  readonly stepIndex: number;
}

/** The Flick `ExecutionPlan.Event` of a native event. */
const event = (e: ExecutionPlan.Event<unknown>): { readonly _tag: string; readonly _0: Attempt; readonly _1?: Duration.Duration; readonly _2?: unknown } => {
  const attempt: Attempt = { attempt: e.attempt, stepAttempt: e.stepAttempt, stepIndex: e.stepIndex };
  switch (e._tag) {
    case "AttemptStart":
      return { _tag: START, _0: attempt };
    case "AttemptSuccess":
      return { _tag: SUCCESS, _0: attempt, _1: e.duration };
    default:
      return { _tag: FAILURE, _0: attempt, _1: e.duration, _2: toFailure(e.cause) };
  }
};

/** `Effect.with_execution_plan`: `on_event` gets each event as a Flick value. */
export const flickWithExecutionPlan =
  (plan: Plan) =>
  (onEvent: (event: unknown) => Effect.Effect<void>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.withExecutionPlan(self, plan, { onEvent: (e) => onEvent(event(e)) });

/** `Stream.with_execution_plan`: `on_event` gets each event as a Flick value. */
export const flickStreamWithExecutionPlan =
  (plan: Plan, preventFallbackOnPartialStream: boolean, onEvent: (event: unknown) => Effect.Effect<void>) =>
  <A, E, R>(self: Stream.Stream<A, E, R>) =>
    Stream.withExecutionPlan(self, plan, { preventFallbackOnPartialStream, onEvent: (e) => onEvent(event(e)) });
