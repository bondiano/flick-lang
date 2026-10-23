// Effect's `Console` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Console as NodeConsole } from "node:console";

import { Console, Effect, Option } from "effect";

export * from "effect/Console";

/** `Console.current`. */
export const flickCurrent = (): Effect.Effect<Console.Console> => Console.consoleWith(Effect.succeed);

/** `Console.with_console`. */
export const flickWithConsole =
  (console: Console.Console) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Console.Console, console);

/** `Console.to_stderr`: a Node.js console on stderr only. */
export const flickToStderr = (): Console.Console => new NodeConsole(process.stderr, process.stderr);

/** `Console.clear`. */
export const flickClear = (): Effect.Effect<void> => Console.clear;

interface GroupOptions {
  readonly label: Option.Option<string>;
  readonly collapsed: boolean;
}

const groupOptions = (options: GroupOptions) => ({
  label: Option.getOrUndefined(options.label),
  collapsed: options.collapsed,
});

/** `Console.group`. */
export const flickGroup = (options: GroupOptions) => Console.group(groupOptions(options));

/** `Console.with_group`. */
export const flickWithGroup = (options: GroupOptions) => Console.withGroup(groupOptions(options));

/** `Console.table`: the columns first, `None` for all of them. */
export const flickTable = (properties: Option.Option<ReadonlyArray<string>>, data: unknown): Effect.Effect<void> =>
  Console.table(data, Option.getOrUndefined(properties));
