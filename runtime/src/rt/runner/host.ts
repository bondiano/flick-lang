// Live `Host.*` service implementations for Flick's own runtime runners.
import { flickCurrentTimeMillis, flickCurrentTimeNanos, flickMonotonicTimeNanos } from "../effect/Clock.ts";
import { flickNext } from "../effect/Random.ts";
import { platformArgs, platformEnv, stderrLine, stdoutLine, stdoutWrite } from "../effects.ts";
import { service } from "../handle.ts";
import { type Services } from "../layer.ts";

export const browserLiveServices: Services = [
  [
    service("Host.Console"),
    {
      stdout_line: stdoutLine,
      stderr_line: stderrLine,
      stdout_write: stdoutWrite,
    },
  ],
  [
    service("Host.Clock"),
    {
      current_time_millis: flickCurrentTimeMillis,
      current_time_nanos: flickCurrentTimeNanos,
      monotonic_time_nanos: flickMonotonicTimeNanos,
    },
  ],
  [
    service("Host.Random"),
    {
      next: flickNext,
    },
  ],
];

export const hostLiveServices: Services = [
  ...browserLiveServices,
  [
    service("Host.Environment"),
    {
      env: platformEnv,
    },
  ],
  [
    service("Host.Arguments"),
    {
      args: platformArgs,
    },
  ],
];
