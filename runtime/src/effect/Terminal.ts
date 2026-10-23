// Effect's `Terminal` for the bindings of the standard library: the Flick
// service `Terminal.Terminal` built from an Effect `Terminal`.
import * as NodeTerminal from "@effect/platform-node/NodeTerminal";
import { type Context, Effect, Layer, Option, type Terminal } from "effect";

import { tag } from "../tags.ts";
import { mapError } from "./PlatformError.ts";

export * from "effect/Terminal";

const QUIT = Object.freeze({ _tag: tag("Terminal.QuitError.QuitError") });

/** The operations of `Terminal.Terminal`, curried as Flick calls them. */
const impl = (terminal: Terminal.Terminal) => ({
  columns: () => terminal.columns,
  rows: () => terminal.rows,
  read_line: () => Effect.mapError(terminal.readLine, () => QUIT),
  display: (text: string) => mapError(terminal.display(text)),
});

type Impl = ReturnType<typeof impl>;

/** `Terminal.node` and `Terminal.node_quitting`. */
export const flickNode = <I>(
  key: Context.Key<I, Impl>,
  shouldQuit: Option.Option<(input: Terminal.UserInput) => boolean>,
): Layer.Layer<I> =>
  Layer.effect(key, Effect.map(NodeTerminal.make(Option.getOrUndefined(shouldQuit)), impl));
