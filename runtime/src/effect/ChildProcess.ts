// Effect's child processes for the bindings of the standard library: a
// `ChildProcess.Command` is Effect's command, run by the spawner of Node.js;
// a `PlatformError` becomes the Flick error, and bytes are Flick lists.
import { type Duration, Effect, Option, type PlatformError, Stream } from "effect";
import { ChildProcess, ChildProcessSpawner } from "effect/process";

import { nodeModule } from "../host.ts";
import { mapError, toFlick } from "./PlatformError.ts";

export * from "effect/process/ChildProcess";

type Command = ChildProcess.Command;
type Handle = ChildProcessSpawner.ChildProcessHandle;

/** Node's services, which have the spawner: loaded when a command runs. */
const services = () => nodeModule("@effect/platform-node/NodeServices").layer;

/** `ChildProcess.make`: the program and its arguments, without a shell. */
export const flickMake = (command: string, args: ReadonlyArray<string>): Command => ChildProcess.make(command, args);

/** `ChildProcess.set_env`: the variables as pairs. */
export const flickSetEnv = (env: ReadonlyArray<readonly [string, string]>, self: Command): Command =>
  ChildProcess.setEnv(self, Object.fromEntries(env));

/** `ChildProcess.prefix`. */
export const flickPrefix = (command: string, args: ReadonlyArray<string>, self: Command): Command =>
  ChildProcess.prefix(self, command, args);

/** The command with its options changed: of the first command of a pipeline, or of the last. */
const withOptions = (self: Command, first: boolean, options: ChildProcess.CommandOptions): Command => {
  if (self._tag === "PipedCommand")
    return first
      ? ChildProcess.pipeTo(withOptions(self.left, first, options), self.right, self.options)
      : ChildProcess.pipeTo(self.left, withOptions(self.right, first, options), self.options);
  return ChildProcess.make(self.command, self.args, { ...self.options, ...options });
};

/** `ChildProcess.set_stdin`: the text, then the end of the input. */
export const flickSetStdin = (text: string, self: Command): Command =>
  withOptions(self, true, { stdin: Stream.make(new TextEncoder().encode(text)) });

/** `ChildProcess.inherit_output`. */
export const flickInheritOutput = (self: Command): Command =>
  withOptions(self, false, { stdout: "inherit", stderr: "inherit" });

/** `ChildProcess.spawn`, in the scope of the caller. */
export const flickSpawn = (command: Command) =>
  mapError(Effect.provide(Effect.flatMap(ChildProcessSpawner.ChildProcessSpawner, (s) => s.spawn(command)), services()));

export const flickPid = (handle: Handle): number => handle.pid;

export const flickWait = (handle: Handle) => mapError(handle.exitCode);

export const flickIsRunning = (handle: Handle) => mapError(handle.isRunning);

export const flickKill = (handle: Handle) => mapError(handle.kill());

/** `ChildProcess.kill_with`. */
export const flickKillWith = (
  options: { readonly signal: string; readonly force_after: Option.Option<Duration.Duration> },
  handle: Handle,
) =>
  mapError(
    handle.kill({
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- Node rejects a name it does not know, as a failure of `kill`
      killSignal: options.signal as ChildProcess.Signal,
      forceKillAfter: Option.getOrUndefined(options.force_after),
    }),
  );

/** Effect's bytes as Flick lists, its `PlatformError` the Flick one. */
const bytes = (stream: Stream.Stream<Uint8Array, PlatformError.PlatformError>) =>
  Stream.mapError(
    Stream.map(stream, (chunk) => Array.from(chunk)),
    toFlick,
  );

export const flickStdout = (handle: Handle) => bytes(handle.stdout);

export const flickStderr = (handle: Handle) => bytes(handle.stderr);

/** A method of the spawner of Node.js. */
const spawner = <A>(f: (s: ChildProcessSpawner.ChildProcessSpawner["Service"]) => Effect.Effect<A, unknown>) =>
  mapError(Effect.provide(Effect.flatMap(ChildProcessSpawner.ChildProcessSpawner, f), services()));

export const flickString = (command: Command) => spawner((s) => s.string(command));

export const flickLines = (command: Command) => spawner((s) => s.lines(command));

export const flickExitCode = (command: Command) => spawner((s) => Effect.map(s.exitCode(command), Number));
