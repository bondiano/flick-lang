// The value `Cli.Command r e` and its parts: plain data the combinators of
// the standard library build, turned into Effect's CLI only by `Cli.run`.
import type { Effect, Result } from "effect";

/** `Cli.Value a`: parses one value; the error is shown to the user. */
export interface ValueDict {
  readonly parse: (text: string) => Effect.Effect<Result.Result<unknown, string>>;
}

/** `Cli.Multi a t`: how the values of a parameter make its field. */
export type Multi =
  | { readonly kind: "switch" }
  | { readonly kind: "required" }
  | { readonly kind: "default"; readonly value: unknown }
  | { readonly kind: "optional" }
  | { readonly kind: "many" }
  | { readonly kind: "some" };

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

/** `Cli.Spec r e o`: a command being described. */
export interface Spec {
  readonly name: string;
  readonly description: string;
  readonly params: ReadonlyArray<Param>;
  readonly subcommands: ReadonlyArray<Subcommand>;
}

/** `Cli.Command r e`: with a handler, or a group of subcommands. */
export interface Command extends Spec {
  readonly handler: ((options: Readonly<Record<string, unknown>>) => Effect.Effect<unknown, unknown, unknown>) | null;
}

export const multiSwitch: Multi = Object.freeze({ kind: "switch" });
export const multiRequired: Multi = Object.freeze({ kind: "required" });
export const multiOptional: Multi = Object.freeze({ kind: "optional" });
export const multiMany: Multi = Object.freeze({ kind: "many" });
export const multiSome: Multi = Object.freeze({ kind: "some" });
export const multiDefault = (value: unknown): Multi => Object.freeze({ kind: "default", value });

export const specCommand = (name: string): Spec =>
  Object.freeze({ name, description: "", params: [], subcommands: [] });

export const specDescription = (description: string, spec: Spec): Spec => Object.freeze({ ...spec, description });

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
