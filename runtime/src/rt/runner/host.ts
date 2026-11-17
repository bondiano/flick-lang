// Live `Host.*` service implementations for Flick's own runtime runners, one
// export per service, so a launcher imports only those its `main` names.
import { flickCurrentTimeMillis, flickCurrentTimeNanos, flickMonotonicTimeNanos } from "../effect/Clock.ts";
import { flickNext } from "../effect/Random.ts";
import { platformArgs, platformEnv, stderrLine, stdoutLine, stdoutWrite } from "../effects.ts";
import { Under, currentCtx, extend, readOwnerEvidence } from "../fiber.ts";
import { service } from "../handle.ts";
import { type Services } from "../layer.ts";
import { within } from "../owner.ts";

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

/**
 * How `hostProcess` starts a command. The kernel has no child processes:
 * the Effect adapter fills this in when generated code crosses into it, as a
 * command can only come from `ChildProcess.make`, which only it has.
 */
export const processHooks: { spawn: ((command: unknown) => unknown) | undefined } = { spawn: undefined };

/** `Host.Process.spawn` of the live service. */
export const processSpawn = (command: unknown): unknown => {
  if (processHooks.spawn === undefined) throw new Error("`Host.Process` needs the Effect adapter, `@flick-lang/effect-ts`");
  return processHooks.spawn(command);
};

// the child belongs to the scope the caller's stage acquires into, as for a compiled foreign
export const hostProcess: HostService = [
  /*#__PURE__*/ service("Host.Process"),
  { spawn: (command: unknown): unknown => within(readOwnerEvidence(), () => processSpawn(command)) },
];

export const browserLiveServices: Services = [hostConsole, hostClock, hostRandom];

export const hostLiveServices: Services = [...browserLiveServices, hostEnvironment, hostArguments, hostProcess];

/**
 * `main` with `services` as its root: what the launcher of a `main` without a
 * runner passes `runMain`, the services that `main` names. It runs under the
 * root owner; what `main` provides itself is inner, so it wins.
 */
export const withHost =
  <I>(services: Services, main: (input: I) => unknown) =>
  (input: I): Under =>
    new Under(services.reduce((ctx, [key, impl]) => extend(ctx, key, impl), currentCtx()), () => main(input));
