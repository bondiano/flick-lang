// Live `Host.*` service implementations for Flick's own runtime runners, one
// export per service, so a launcher imports only those its `main` names.
import { flickCurrentTimeMillis, flickCurrentTimeNanos, flickMonotonicTimeNanos } from "../effect/Clock.ts";
import { flickNext } from "../effect/Random.ts";
import { platformArgs, platformEnv, stderrLine, stdoutLine, stdoutWrite } from "../effects.ts";
import { Under, currentCtx, extend } from "../fiber.ts";
import { service } from "../handle.ts";
import { type Services } from "../layer.ts";

/** A live service: its tag and its implementation. */
export type HostService = Services[number];

export const hostConsole: HostService = [
  /*#__PURE__*/ service("Host.Console"),
  { stdout_line: stdoutLine, stderr_line: stderrLine, stdout_write: stdoutWrite },
];

export const hostClock: HostService = [
  /*#__PURE__*/ service("Host.Clock"),
  {
    current_time_millis: flickCurrentTimeMillis,
    current_time_nanos: flickCurrentTimeNanos,
    monotonic_time_nanos: flickMonotonicTimeNanos,
  },
];

export const hostRandom: HostService = [/*#__PURE__*/ service("Host.Random"), { next: flickNext }];

export const hostEnvironment: HostService = [/*#__PURE__*/ service("Host.Environment"), { env: platformEnv }];

export const hostArguments: HostService = [/*#__PURE__*/ service("Host.Arguments"), { args: platformArgs }];

export const browserLiveServices: Services = [hostConsole, hostClock, hostRandom];

export const hostLiveServices: Services = [...browserLiveServices, hostEnvironment, hostArguments];

/**
 * `main` with `services` as its root: what the launcher of a `main` without a
 * runner passes `runMain`, the services that `main` names. It runs under the
 * root owner; what `main` provides itself is inner, so it wins.
 */
export const withHost =
  <I>(services: Services, main: (input: I) => unknown) =>
  (input: I): Under =>
    new Under(services.reduce((ctx, [key, impl]) => extend(ctx, key, impl), currentCtx()), () => main(input));
