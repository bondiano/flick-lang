// `Cli.run`: a command of the standard library as Effect's CLI command, run
// on the arguments with the Node.js services Effect's CLI needs. Invalid
// input becomes the Flick error `Cli.CliError` with Effect's text; the
// handler runs only after the whole command line parsed.
import * as NodeServices from "@effect/platform-node/NodeServices";
import { Effect, Result } from "effect";
import { Argument, CliError, Command, Flag } from "effect/cli";

import type { Tagged } from "../data.ts";
import { tag } from "../tags.ts";
import type { Command as Spec, Param } from "./spec.ts";

/** `Cli.CliError`. */
export type FlickCliError = Tagged<string, string>;

const CLI_ERROR = tag("Cli.CliError.CliError");

/** The value of a parameter's text, through its `Cli.Value` dictionary. */
const parse =
  (param: Param) =>
  (text: string): Effect.Effect<unknown, CliError.CliError> =>
    Effect.flatMap(param.value.parse(text), (parsed) =>
      Result.isSuccess(parsed)
        ? Effect.succeed(parsed.success)
        : Effect.fail(
            new CliError.InvalidValue({
              option: param.label,
              value: text,
              expected: parsed.failure,
              kind: param.flag ? "flag" : "argument",
            }),
          ),
    );

const flag = (param: Param): Flag.Flag<unknown> => {
  const name = param.label.replaceAll("_", "-");
  let f: Flag.Flag<unknown> =
    param.multi.kind === "switch"
      ? Flag.withDefault(Flag.Boolean(name), false)
      : Flag.mapEffect(Flag.String(name), parse(param));
  for (const alias of param.aliases) f = Flag.withAlias(f, alias);
  if (param.help !== "") f = Flag.withDescription(f, param.help);
  const multi = param.multi;
  if (multi.kind === "default") return Flag.withDefault(f, multi.value);
  if (multi.kind === "optional") return Flag.optional(f);
  if (multi.kind === "many") return Flag.atLeast(f, 0);
  if (multi.kind === "some") return Flag.atLeast(f, 1);
  return f;
};

const argument = (param: Param): Argument.Argument<unknown> => {
  let a: Argument.Argument<unknown> = Argument.mapEffect(Argument.String(param.label.replaceAll("_", "-")), parse(param));
  if (param.help !== "") a = Argument.withDescription(a, param.help);
  const multi = param.multi;
  if (multi.kind === "default") return Argument.withDefault(a, multi.value);
  if (multi.kind === "optional") return Argument.optional(a);
  if (multi.kind === "many") return Argument.atLeast(a, 0);
  if (multi.kind === "some") return Argument.atLeast(a, 1);
  return a;
};

/**
 * Effect's command of a Flick command, its subcommands included. The flags
 * of a command with subcommands are shared flags: Effect parses them before
 * the subcommand's name, and they are seen only by this command's handler.
 */
const command = (spec: Spec): Command.Command.Any => {
  const config: Record<string, Flag.Flag<unknown> | Argument.Argument<unknown>> = {};
  const flags: Record<string, Flag.Flag<unknown>> = {};
  for (const param of spec.params) {
    if (!param.flag) config[param.label] = argument(param);
    else if (spec.subcommands.length > 0) flags[param.label] = flag(param);
    else config[param.label] = flag(param);
  }
  const finish = <I, CI, E, R>(made: Command.Command<string, I, CI, E, R>): Command.Command.Any => {
    const described = spec.description === "" ? made : Command.withDescription(made, spec.description);
    if (spec.subcommands.length === 0) return described;
    const shared = Command.withSharedFlags(described, flags);
    return Command.withSubcommands(shared, spec.subcommands.map((s) => command(s.command)));
  };
  const handler = spec.handler;
  if (handler === null) return finish(Command.make(spec.name, config));
  return finish(Command.make(spec.name, config, (options) => Effect.asVoid(handler(options))));
};

/** What went wrong: Effect asks for the help of a command with the errors that caused it. */
const cliError = (error: CliError.CliError): FlickCliError => {
  const causes = error._tag === "ShowHelp" ? error.errors.map((e) => e.message) : [];
  return { _tag: CLI_ERROR, _0: causes.length > 0 ? causes.join("\n") : error.message };
};

/** `Cli.run command args`, with `version` for `--version`. */
export const cliRunVersion = (
  version: string,
  spec: Spec,
  args: ReadonlyArray<string>,
): Effect.Effect<void, unknown, unknown> =>
  Command.runWith(command(spec), { version, renderErrors: false })(args).pipe(
    Effect.catchIf(CliError.isCliError, (error) => Effect.fail(cliError(error))),
    Effect.provide(NodeServices.layer),
  );
