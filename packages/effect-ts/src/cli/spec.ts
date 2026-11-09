// The value `Cli.Command r e` and its parts: plain data the combinators of
// the standard library build, turned into Effect's CLI only by `Cli.run`.
import { type Config, type Effect, Option, type Result } from "effect";
import type { Prompt } from "effect/cli";

import { constructorName } from "@flick-lang/runtime/tags";

/** `Cli.Value a`: parses one value; the error is shown to the user. */
export interface ValueDict {
  readonly parse: (text: string) => Effect.Effect<Result.Result<unknown, string>>;
}

/** Where the text of a value comes from before `Cli.Value` parses it. */
export type Source =
  | { readonly kind: "text" }
  | { readonly kind: "path"; readonly pathType: "file" | "directory" | "either"; readonly mustExist: boolean }
  | { readonly kind: "fileText" }
  | { readonly kind: "fileParse"; readonly format: "ini" | "json" | "toml" | "yaml" | undefined };

/** What the modifiers of a multiplicity add to its parameter. */
export interface Mods {
  readonly hidden: boolean;
  readonly metavar: string | undefined;
  readonly optionalValue: string | undefined;
  readonly source: Source;
}

/** `Cli.Multi a t`: how the values of a parameter make its field. */
export type MultiKind =
  | { readonly kind: "switch" }
  | { readonly kind: "required" }
  | { readonly kind: "default"; readonly value: unknown }
  | { readonly kind: "optional" }
  | { readonly kind: "variadic"; readonly min: number | undefined; readonly max: number | undefined }
  | { readonly kind: "some" }
  | { readonly kind: "config"; readonly config: Config.Config<unknown> }
  | { readonly kind: "prompt"; readonly prompt: Prompt.Prompt<unknown> }


/** `Cli.Multi a t`, with what its modifiers add. */
export type Multi = MultiKind & { readonly mods: Mods };

export interface Param {
  readonly flag: boolean;
  /** The field of the options record. */
  readonly label: string;
  readonly aliases: ReadonlyArray<string>;
  readonly help: string;
  readonly multi: Multi;
  readonly value: ValueDict;
}

export interface Subcommand {
  readonly command: Command;
  readonly help: string;
}

/** `Cli.ActionContext`. */
export interface ActionContext {
  readonly commandPath: ReadonlyArray<string>;
  readonly version: string;
}

/** `Cli.action`: a flag that runs `run` instead of the command. */
export interface Action {
  readonly name: string;
  readonly help: string;
  readonly run: (context: ActionContext) => Effect.Effect<unknown>;
}

/** `Cli.Spec r e o`: a command being described. */
export interface Spec {
  readonly name: string;
  readonly description: string;
  readonly shortDescription: string;
  readonly alias: string;
  readonly unlisted: boolean;
  readonly examples: ReadonlyArray<{ readonly command: string; readonly description: string }>;
  readonly actions: ReadonlyArray<Action>;
  readonly params: ReadonlyArray<Param>;
  readonly subcommands: ReadonlyArray<Subcommand>;
}

/** `Cli.Command r e`: with a handler, or a group of subcommands. */
export interface Command extends Spec {
  readonly handler: ((options: Readonly<Record<string, unknown>>) => Effect.Effect<unknown, unknown, unknown>) | null;
}

const NO_MODS: Mods = /*#__PURE__*/ Object.freeze({
  hidden: false,
  metavar: undefined,
  optionalValue: undefined,
  source: Object.freeze({ kind: "text" }),
});

const make = (m: MultiKind): Multi => Object.freeze({ ...m, mods: NO_MODS });

export const multiSwitch: Multi = /*#__PURE__*/ make({ kind: "switch" });
export const multiRequired: Multi = /*#__PURE__*/ make({ kind: "required" });
export const multiOptional: Multi = /*#__PURE__*/ make({ kind: "optional" });
export const multiMany: Multi = /*#__PURE__*/ make({ kind: "variadic", min: 0, max: undefined });
export const multiSome: Multi = /*#__PURE__*/ make({ kind: "some" });
export const multiDefault = (value: unknown): Multi => make({ kind: "default", value });
export const multiAtLeast = (min: number): Multi => make({ kind: "variadic", min, max: undefined });
export const multiAtMost = (max: number): Multi => make({ kind: "variadic", min: undefined, max });
export const multiBetween = (min: number, max: number): Multi => make({ kind: "variadic", min, max });
export const multiConfig = (config: Config.Config<unknown>): Multi => make({ kind: "config", config });
export const multiPrompt = (prompt: Prompt.Prompt<unknown>): Multi => make({ kind: "prompt", prompt });

const modify = (m: Multi, mods: Partial<Mods>): Multi => Object.freeze({ ...m, mods: { ...m.mods, ...mods } });

export const multiHidden = (m: Multi): Multi => modify(m, { hidden: true });
export const multiMetavar = (metavar: string, m: Multi): Multi => modify(m, { metavar });
export const multiOptionalValue = (optionalValue: string, m: Multi): Multi => modify(m, { optionalValue });
export const multiFile = (mustExist: boolean, m: Multi): Multi =>
  modify(m, { source: { kind: "path", pathType: "file", mustExist } });
export const multiDirectory = (mustExist: boolean, m: Multi): Multi =>
  modify(m, { source: { kind: "path", pathType: "directory", mustExist } });
export const multiPath = (mustExist: boolean, m: Multi): Multi =>
  modify(m, { source: { kind: "path", pathType: "either", mustExist } });
export const multiFileText = (m: Multi): Multi => modify(m, { source: { kind: "fileText" } });

/** `Cli.file_parse`: the format is a `Cli.FileFormat` or `None` (by the extension). */
export const multiFileParse = (format: Option.Option<{ readonly _tag: string }>, m: Multi): Multi =>
  modify(m, {
    source: { kind: "fileParse", format: Option.isSome(format) ? FORMATS[constructorName(format.value._tag)] : undefined },
  });

const FORMATS: Readonly<Record<string, "ini" | "json" | "toml" | "yaml">> = {
  Ini: "ini",
  Json: "json",
  Toml: "toml",
  Yaml: "yaml",
};

export const specCommand = (name: string): Spec =>
  Object.freeze({
    name,
    description: "",
    shortDescription: "",
    alias: "",
    unlisted: false,
    examples: [],
    actions: [],
    params: [],
    subcommands: [],
  });

export const specDescription = (description: string, spec: Spec): Spec => Object.freeze({ ...spec, description });

export const specShortDescription = (shortDescription: string, spec: Spec): Spec =>
  Object.freeze({ ...spec, shortDescription });

export const specAlias = (alias: string, spec: Spec): Spec => Object.freeze({ ...spec, alias });

export const specUnlisted = (spec: Spec): Spec => Object.freeze({ ...spec, unlisted: true });

export const specExample = (command: string, description: string, spec: Spec): Spec =>
  Object.freeze({ ...spec, examples: [...spec.examples, { command, description }] });

export const specAction = (name: string, help: string, run: Action["run"], spec: Spec): Spec =>
  Object.freeze({ ...spec, actions: [...spec.actions, { name, help, run }] });

export const specFlag = (
  value: ValueDict,
  _lacks: unknown,
  _union: unknown,
  label: string,
  aliases: ReadonlyArray<string>,
  help: string,
  multi: Multi,
  spec: Spec,
): Spec => Object.freeze({ ...spec, params: [...spec.params, { flag: true, label, aliases, help, multi, value }] });

export const specArg = (
  value: ValueDict,
  _lacks: unknown,
  _union: unknown,
  label: string,
  help: string,
  multi: Multi,
  spec: Spec,
): Spec => Object.freeze({ ...spec, params: [...spec.params, { flag: false, label, aliases: [], help, multi, value }] });

export const specSubcommand = (command: Command, help: string, spec: Spec): Spec =>
  Object.freeze({ ...spec, subcommands: [...spec.subcommands, { command, help }] });

export const specHandle = (handler: NonNullable<Command["handler"]>, spec: Spec): Command =>
  Object.freeze({ ...spec, handler });

export const specGroup = (spec: Spec): Command => Object.freeze({ ...spec, handler: null });
