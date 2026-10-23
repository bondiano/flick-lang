// Effect's `Stdio` for the bindings of the standard library: the Flick
// service `Stdio.Stdio` built from an Effect `Stdio`.
import * as NodeStdio from "@effect/platform-node/NodeStdio";
import { type Context, Effect, Layer, Stdio, Stream } from "effect";

import { mapError } from "./PlatformError.ts";

export * from "effect/Stdio";

/** The operations of `Stdio.Stdio`, curried as Flick calls them. */
const impl = (stdio: Stdio.Stdio) => ({
  args: () => stdio.args,
  stdin_is_terminal: () => stdio.stdinIsTerminal,
  stdout_is_terminal: () => stdio.stdoutIsTerminal,
  stderr_is_terminal: () => stdio.stderrIsTerminal,
  write_stdout: (text: string) => mapError(Stream.run(Stream.make(text), stdio.stdout({ endOnDone: false }))),
  write_stderr: (text: string) => mapError(Stream.run(Stream.make(text), stdio.stderr({ endOnDone: false }))),
});

type Impl = ReturnType<typeof impl>;

/** `Stdio.node`. */
export const flickNode = <I>(key: Context.Key<I, Impl>): Layer.Layer<I> =>
  Layer.effect(key, Effect.map(Effect.service(Stdio.Stdio), impl)).pipe(Layer.provide(NodeStdio.layer));
