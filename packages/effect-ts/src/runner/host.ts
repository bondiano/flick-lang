// Live `Host.*` service implementations for Effect-TS runners.
import { Effect } from "effect";

import { stderrLine, stdoutLine } from "../builtins.ts";
import { flickCurrentTimeMillis, flickCurrentTimeNanos, flickMonotonicTimeNanos } from "../effect/Clock.ts";
import { flickNext } from "../effect/Random.ts";
import { type ServiceImpl, service } from "../effects.ts";
import { platformArgs, platformEnv, stdoutWrite } from "../stdlib/text.ts";

type ServiceEntry = readonly [string, ServiceImpl];

const browserEntries: ReadonlyArray<ServiceEntry> = [
  [
    "Host.Console",
    {
      stdout_line: (text: string) => Effect.sync(() => stdoutLine(text)),
      stderr_line: (text: string) => Effect.sync(() => stderrLine(text)),
      stdout_write: (text: string) => Effect.sync(() => stdoutWrite(text)),
    },
  ],
  [
    "Host.Clock",
    {
      current_time_millis: flickCurrentTimeMillis,
      current_time_nanos: flickCurrentTimeNanos,
      monotonic_time_nanos: flickMonotonicTimeNanos,
    },
  ],
  [
    "Host.Random",
    {
      next: flickNext,
    },
  ],
];

const nodeEntries: ReadonlyArray<ServiceEntry> = [
  ...browserEntries,
  [
    "Host.Environment",
    {
      env: (name: string) => Effect.sync(() => platformEnv(name)),
    },
  ],
  [
    "Host.Arguments",
    {
      args: () => Effect.sync(() => platformArgs()),
    },
  ],
];

const provideServices =
  (entries: ReadonlyArray<ServiceEntry>) =>
  (program: Effect.Effect<unknown, unknown>): Effect.Effect<unknown, unknown> =>
    entries.reduce((eff, [key, impl]) => Effect.provideService(eff, service(key), impl), program);

export const provideBrowserServices = provideServices(browserEntries);

export const provideNodeServices = provideServices(nodeEntries);
