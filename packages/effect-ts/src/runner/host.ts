// Live `Host.*` service implementations for Effect-TS runners, one export per
// service, so a launcher imports only those its `main` names.
import { Effect } from "effect";

import { stderrLine, stdoutLine } from "../builtins.ts";
import { flickCurrentTimeMillis, flickCurrentTimeNanos, flickMonotonicTimeNanos } from "../effect/Clock.ts";
import { flickSpawn } from "../effect/ChildProcess.ts";
import { flickNext } from "../effect/Random.ts";
import { type ServiceImpl, service } from "../effects.ts";
import { platformArgs, platformEnv, stdoutWrite } from "../stdlib/text.ts";

/** A live service: its key and its implementation. */
export type HostService = readonly [string, ServiceImpl];

export const hostConsole: HostService = [
  "Host.Console",
  {
    stdout_line: (text: string) => Effect.sync(() => stdoutLine(text)),
    stderr_line: (text: string) => Effect.sync(() => stderrLine(text)),
    stdout_write: (text: string) => Effect.sync(() => stdoutWrite(text)),
  },
];

export const hostClock: HostService = [
  "Host.Clock",
  {
    current_time_millis: flickCurrentTimeMillis,
    current_time_nanos: flickCurrentTimeNanos,
    monotonic_time_nanos: flickMonotonicTimeNanos,
  },
];

export const hostRandom: HostService = ["Host.Random", { next: flickNext }];

export const hostEnvironment: HostService = [
  "Host.Environment",
  { env: (name: string) => Effect.sync(() => platformEnv(name)) },
];

export const hostArguments: HostService = ["Host.Arguments", { args: () => Effect.sync(() => platformArgs()) }];

/** `Host.Process.spawn` of the live service: Node's spawner, in the caller's scope. */
export const processSpawn = flickSpawn;

export const hostProcess: HostService = ["Host.Process", { spawn: processSpawn }];

/**
 * Provides `services` around a program: what the launcher of a `main` without
 * a runner passes `runMain` as the root, the services that `main` names.
 */
export const provideHost =
  (services: ReadonlyArray<HostService>) =>
  <A, E>(program: Effect.Effect<A, E>): Effect.Effect<A, E> =>
    services.reduce((eff, [key, impl]) => Effect.provideService(eff, service(key), impl), program);

export const provideBrowserServices = /*#__PURE__*/ provideHost([hostConsole, hostClock, hostRandom]);

export const provideNodeServices = /*#__PURE__*/ provideHost([
  hostConsole,
  hostClock,
  hostRandom,
  hostEnvironment,
  hostArguments,
  hostProcess,
]);
