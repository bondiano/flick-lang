// `Cli.run`: a command of the standard library as Effect's CLI command, run
// on the arguments with the Node.js services Effect's CLI needs. Invalid
// input becomes the Flick error `Cli.CliError` with Effect's text; the
// handler runs only after the whole command line parsed.

import { Console, Effect, Layer, Option } from "effect";
import * as TestConsole from "effect/testing/TestConsole";
import {
  CliConfig,
  CliError,
  CliOutput,
  Command,
  Flag,
  GlobalFlag,
  type HelpDoc,
  Param as EffectParam,
  Primitive,
} from "effect/cli";

import type { Tagged } from "../data.ts";
import { env, nodeModule } from "../host.ts";
import { constructorName, tag } from "@flick-lang/runtime/tags";
import { type FlickFormatter, type FlickHelpDoc, fromEffectDoc, toEffectFormatter, withHelp } from "./output.ts";
import { callEffect } from "../bridge.ts";
import type { Command as Spec, Param, Source, ValueResult } from "./spec.ts";

/** `Cli.CliError`. */
export type FlickCliError = Tagged<string, string>;

const CLI_ERROR = /*#__PURE__*/ tag("Cli.CliError.CliError");

/** The value of a parameter's text, through its `Cli.Value` dictionary. */
const parse =
  (param: Param) =>
  (text: string): Effect.Effect<unknown, CliError.CliError> => {
    const parsed = param.value.parse(text);
    // The dictionary signature fixes the result and has no typed error row;
    // native runtime stages still need their driver even when that row is closed.
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- Cli.Value.parse fixes this boundary's result and error types
    const evaluated = (Effect.isEffect(parsed) ? parsed : callEffect(() => parsed)) as Effect.Effect<ValueResult>;
    return Effect.flatMap(evaluated, (result) =>
      result._tag === "Success"
        ? Effect.succeed(result.success)
        : Effect.fail(
            new CliError.InvalidValue({
              option: param.label,
              value: text,
              expected: result.failure,
              kind: param.flag ? "flag" : "argument",
            }),
          ),
    );
  };

/** What Effect reads for a parameter before `Cli.Value` parses its text. */
const primitive = (source: Source): Primitive.Primitive<unknown> => {
  if (source.kind === "path") return Primitive.Path(source.pathType, source.mustExist);
  if (source.kind === "fileText") return Primitive.FileText;
  if (source.kind === "fileParse")
    return Primitive.FileParse(source.format === undefined ? {} : { format: source.format });
  return Primitive.String;
};

/** The text of what Effect read: the data of a parsed file is its JSON. */
const text = (source: Source, value: unknown): string =>
  source.kind === "fileParse" ? JSON.stringify(value) : String(value);

type AnyParam<K extends EffectParam.ParamKind> = EffectParam.Param<K, unknown>;

/** Effect's flag or argument of a parameter, its multiplicity and modifiers applied. */
const param = <K extends EffectParam.ParamKind>(kind: K, p: Param): AnyParam<K> => {
  const name = p.label.replaceAll("_", "-");
  const { multi } = p;
  const { mods } = multi;
  let f: AnyParam<K> =
    multi.kind === "switch"
      ? EffectParam.withDefault(EffectParam.Boolean(kind, name), false)
      : EffectParam.mapEffect(
          EffectParam.makeSingle({ kind, name, primitiveType: primitive(mods.source) }),
          (value: unknown) => parse(p)(text(mods.source, value)),
        );
  for (const alias of p.aliases) f = EffectParam.withAlias(f, alias);
  if (p.help !== "") f = EffectParam.withDescription(f, p.help);
  if (mods.hidden) f = EffectParam.withHidden(f);
  if (mods.metavar !== undefined) f = EffectParam.withMetavar(f, mods.metavar);
  if (multi.kind === "default") return EffectParam.withDefault(f, multi.value);
  if (multi.kind === "optional") return EffectParam.optional(f);
  if (multi.kind === "variadic") return EffectParam.variadic(f, { min: multi.min, max: multi.max });
  if (multi.kind === "some") return EffectParam.atLeast(f, 1);
  if (multi.kind === "config") return EffectParam.withFallbackConfig(f, multi.config);
  if (multi.kind === "prompt") return EffectParam.withFallbackPrompt(f, multi.prompt);
  return f;
};

/** A flag, which may take the text of `Cli.optional_value` when written alone. */
const flagParam = (p: Param): AnyParam<"flag"> => {
  const f = param("flag", p);
  const { optionalValue } = p.multi.mods;
  return optionalValue === undefined ? f : EffectParam.withOptionalValue(f, optionalValue);
};

/** `Cli.action`: a switch that runs the Flick action instead of the command. */
const action = (a: Spec["actions"][number]): GlobalFlag.GlobalFlag<boolean> =>
  GlobalFlag.Action({
    flag: Flag.withDefault(Flag.withDescription(Flag.Boolean(a.name), a.help), false),
    run: (present, context) =>
      present ? Effect.asVoid(a.run({ commandPath: context.commandPath, version: context.version })) : Effect.void,
  });

/** `--trace`: whether the handler sends its telemetry over OTLP. */
const TRACE = /*#__PURE__*/ GlobalFlag.Setting("trace")({
  flag: Flag.withDefault(
    Flag.withDescription(Flag.Boolean("trace"), "Send spans, logs and metrics to OTEL_EXPORTER_OTLP_ENDPOINT"),
    false,
  ),
});

/**
 * The handler of command `name`: under `--trace` a span of that name, with
 * OTLP provided; the exporter loads only then and flushes as it ends.
 */
const traced = (name: string, handler: Effect.Effect<unknown, unknown, unknown>): Effect.Effect<void, unknown, unknown> =>
  Effect.flatMap(Effect.serviceOption(TRACE), (trace) =>
    Option.getOrElse(trace, () => false)
      ? Effect.flatMap(
          Effect.promise(() => import("../effect/observability/Otlp.ts")),
          ({ otlpLayer }) =>
            Effect.provide(
              Effect.asVoid(Effect.withSpan(handler, name)),
              otlpLayer({
                url: env("OTEL_EXPORTER_OTLP_ENDPOINT") ?? "http://localhost:4318",
                service_name: env("OTEL_SERVICE_NAME") ?? name,
              }),
            ),
        )
      : Effect.asVoid(handler),
  );

/**
 * Effect's command of a Flick command, its subcommands included. The flags
 * of a command with subcommands are shared flags: Effect parses them before
 * the subcommand's name, and they are seen only by this command's handler.
 */
const command = (spec: Spec): Command.Command.Any => {
  const config: Record<string, AnyParam<EffectParam.ParamKind>> = {};
  const flags: Record<string, AnyParam<"flag">> = {};
  for (const p of spec.params) {
    if (!p.flag) config[p.label] = param("argument", p);
    else if (spec.subcommands.length > 0) flags[p.label] = flagParam(p);
    else config[p.label] = flagParam(p);
  }
  const finish = <I, CI, E, R>(made: Command.Command<string, I, CI, E, R>): Command.Command.Any => {
    let c: Command.Command<string, I, CI, E, R> = made;
    if (spec.description !== "") c = Command.withDescription(c, spec.description);
    if (spec.shortDescription !== "") c = Command.withShortDescription(c, spec.shortDescription);
    if (spec.alias !== "") c = Command.withAlias(c, spec.alias);
    if (spec.unlisted) c = Command.unlisted(c);
    if (spec.examples.length > 0)
      c = Command.withExamples(
        c,
        spec.examples.map((e) => (e.description === "" ? { command: e.command } : e)),
      );
    if (spec.actions.length > 0) c = Command.withGlobalFlags(c, spec.actions.map(action));
    if (spec.subcommands.length === 0) return c;
    const shared = Command.withSharedFlags(c, flags);
    const subcommands = spec.subcommands.map((s) => {
      const sub = command(s.command);
      // the help of `subcommand` lists it, unless it has a short description of its own
      return s.help !== "" && s.command.shortDescription === "" ? Command.withShortDescription(sub, s.help) : sub;
    });
    return Command.withSubcommands(shared, subcommands);
  };
  const handler = spec.handler;
  if (handler === null) return finish(Command.make(spec.name, config));
  return finish(Command.make(spec.name, config, (options) => traced(spec.name, handler(options))));
};

/** What went wrong: Effect asks for the help of a command with the errors that caused it. */
const cliError = (error: CliError.CliError): FlickCliError => {
  const causes = error._tag === "ShowHelp" ? error.errors.map((e) => e.message) : [];
  return { _tag: CLI_ERROR, _0: causes.length > 0 ? causes.join("\n") : error.message };
};

const BUILT_INS: Readonly<Record<string, CliConfig.CliConfig.Service["builtIns"][number]>> = {
  HelpFlag: GlobalFlag.Help,
  VersionFlag: GlobalFlag.Version,
  WizardFlag: GlobalFlag.Wizard,
  CompletionsFlag: GlobalFlag.Completions,
  LogLevelFlag: GlobalFlag.LogLevel,
};

const TRACE_FLAG = "TraceFlag";

/** The command of `spec` with `--trace`, a flag of Flick's rather than Effect's, when `trace`. */
const root = (spec: Spec, trace: boolean): Command.Command.Any =>
  trace ? Command.withGlobalFlags(command(spec), [TRACE]) : command(spec);

/** `Cli.Settings`. */
export interface Settings {
  readonly version: string;
  readonly builtIns: ReadonlyArray<{ readonly _tag: string }>;
  readonly formatter: Option.Option<FlickFormatter>;
}

/** The layers of Effect's CLI the settings choose, with the Node.js services. */
const services = (settings: Settings) =>
  Layer.suspend(() =>
    Layer.mergeAll(
      nodeModule("@effect/platform-node/NodeServices").layer,
      CliConfig.layer({ builtIns: settings.builtIns.flatMap((b) => BUILT_INS[constructorName(b._tag)] ?? []) }),
      Option.isSome(settings.formatter) ? CliOutput.layer(toEffectFormatter(settings.formatter.value)) : Layer.empty,
    ),
  );

/** `Cli.run_with settings command args`. */
export const cliRunWith = (
  settings: Settings,
  spec: Spec,
  args: ReadonlyArray<string>,
): Effect.Effect<void, unknown, unknown> =>
  Command.runWith(root(spec, settings.builtIns.some((b) => constructorName(b._tag) === TRACE_FLAG)), {
    version: settings.version,
    renderErrors: false,
  })(args).pipe(
    Effect.catchIf(CliError.isCliError, (error) => Effect.fail(cliError(error))),
    Effect.provide(services(settings)),
  );

const ALL = /*#__PURE__*/ [...Object.keys(BUILT_INS), TRACE_FLAG].map((name) => ({ _tag: tag(`Cli.BuiltIn.${name}`) }));

/** `Cli.run command args`, with `version` for `--version`. */
export const cliRunVersion = (
  version: string,
  spec: Spec,
  args: ReadonlyArray<string>,
): Effect.Effect<void, unknown, unknown> =>
  cliRunWith({ version, builtIns: ALL, formatter: Option.none() }, spec, args);


/** What the command prints for `args`, with every built-in flag; failures are ignored. */
const printed = (
  spec: Spec,
  args: ReadonlyArray<string>,
  formatter: CliOutput.Formatter,
): Effect.Effect<string, never, unknown> =>
  Effect.flatMap(TestConsole.make, (console) =>
    Command.runWith(root(spec, true), { version: "0.0.0", renderErrors: false })(args).pipe(
      Effect.ignore,
      Effect.provideService(Console.Console, console),
      Effect.provide(
        Layer.merge(
          nodeModule("@effect/platform-node/NodeServices").layer,
          CliOutput.layer(formatter),
        ),
      ),
      Effect.andThen(console.logLines),
      Effect.map((lines) => lines.map(String).join("\n")),
    ),
  );

const PLAIN = /*#__PURE__*/ CliOutput.defaultFormatter({ colors: false });

/** `Cli.completions shell command`. */
export const cliCompletions = (shell: { readonly _tag: string }, spec: Spec): Effect.Effect<string, never, unknown> =>
  printed(spec, ["--completions", constructorName(shell._tag).toLowerCase()], PLAIN);

/** `Cli.help_doc path command`: an unknown subcommand gives the help of the command above it. */
export const cliHelpDoc = (path: ReadonlyArray<string>, spec: Spec): Effect.Effect<FlickHelpDoc, never, unknown> => {
  let doc: HelpDoc.HelpDoc | undefined;
  const capture = withHelp(PLAIN, (d) => {
    doc = d;
    return "";
  });
  return Effect.flatMap(printed(spec, [...path, "--help"], capture), () =>
    doc === undefined ? Effect.die(new Error("the CLI printed no help")) : Effect.succeed(fromEffectDoc(doc)),
  );
};
