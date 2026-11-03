// `Cli.HelpDoc` and `Cli.Formatter`: Effect's help documents as Flick
// records (`type` is the field `type_`, absent text is `None`), and a Flick
// formatter as Effect's `CliOutput.Formatter`.
import { Context, Option } from "effect";
import { CliOutput, type HelpDoc } from "effect/cli";

export interface FlickFlagDoc {
  readonly name: string;
  readonly aliases: ReadonlyArray<string>;
  readonly type_: string;
  readonly description: Option.Option<string>;
  readonly required: boolean;
}

export interface FlickArgDoc {
  readonly name: string;
  readonly type_: string;
  readonly description: Option.Option<string>;
  readonly required: boolean;
  readonly variadic: boolean;
}

export interface FlickSubcommandDoc {
  readonly name: string;
  readonly alias: Option.Option<string>;
  readonly shortDescription: Option.Option<string>;
  readonly description: string;
}

export interface FlickHelpDoc {
  readonly description: string;
  readonly usage: string;
  readonly flags: ReadonlyArray<FlickFlagDoc>;
  readonly inheritedFlags: ReadonlyArray<FlickFlagDoc>;
  readonly globalFlags: ReadonlyArray<FlickFlagDoc>;
  readonly args: ReadonlyArray<FlickArgDoc>;
  readonly subcommands: ReadonlyArray<{
    readonly group: Option.Option<string>;
    readonly commands: NonEmpty<FlickSubcommandDoc>;
  }>;
  readonly examples: ReadonlyArray<{ readonly command: string; readonly description: Option.Option<string> }>;
}

/** `Cli.Formatter`: its functions arrive curried, as Flick holds them. */
export interface FlickFormatter {
  readonly formatHelpDoc: (doc: FlickHelpDoc) => string;
  readonly formatVersion: (name: string) => (version: string) => string;
}

type NonEmpty<A> = readonly [A, ...Array<A>];

const mapNonEmpty = <A, B>(xs: NonEmpty<A>, f: (a: A) => B): NonEmpty<B> => {
  const [head, ...tail] = xs;
  return [f(head), ...tail.map(f)];
};

const flag = (f: HelpDoc.FlagDoc): FlickFlagDoc => ({
  name: f.name,
  aliases: f.aliases,
  type_: f.type,
  description: f.description,
  required: f.required,
});

const unflag = (f: FlickFlagDoc): HelpDoc.FlagDoc => ({
  name: f.name,
  aliases: f.aliases,
  type: f.type_,
  description: f.description,
  required: f.required,
});

export const fromEffectDoc = (doc: HelpDoc.HelpDoc): FlickHelpDoc => ({
  description: doc.description,
  usage: doc.usage,
  flags: doc.flags.map(flag),
  inheritedFlags: (doc.inheritedFlags ?? []).map(flag),
  globalFlags: (doc.globalFlags ?? []).map(flag),
  args: (doc.args ?? []).map((a) => ({
    name: a.name,
    type_: a.type,
    description: a.description,
    required: a.required,
    variadic: a.variadic,
  })),
  subcommands: (doc.subcommands ?? []).map((g) => ({
    group: Option.fromUndefinedOr(g.group),
    commands: mapNonEmpty(g.commands, (c) => ({
      name: c.name,
      alias: Option.fromUndefinedOr(c.alias),
      shortDescription: Option.fromUndefinedOr(c.shortDescription),
      description: c.description,
    })),
  })),
  examples: (doc.examples ?? []).map((e) => ({
    command: e.command,
    description: Option.fromUndefinedOr(e.description),
  })),
});

export const toEffectDoc = (doc: FlickHelpDoc): HelpDoc.HelpDoc => ({
  description: doc.description,
  usage: doc.usage,
  flags: doc.flags.map(unflag),
  inheritedFlags: doc.inheritedFlags.map(unflag),
  globalFlags: doc.globalFlags.map(unflag),
  annotations: Context.empty(),
  args: doc.args.map((a) => ({
    name: a.name,
    type: a.type_,
    description: a.description,
    required: a.required,
    variadic: a.variadic,
  })),
  subcommands: doc.subcommands.map((g) => ({
    group: Option.getOrUndefined(g.group),
    commands: mapNonEmpty(g.commands, (c) => ({
      name: c.name,
      alias: Option.getOrUndefined(c.alias),
      shortDescription: Option.getOrUndefined(c.shortDescription),
      description: c.description,
    })),
  })),
  examples: doc.examples.map((e) => ({ command: e.command, description: Option.getOrUndefined(e.description) })),
});

/** Effect's formatter of a Flick one: errors keep Effect's default text. */
export const toEffectFormatter = (f: FlickFormatter): CliOutput.Formatter =>
  withHelp(CliOutput.defaultFormatter({ colors: false }), (doc) => f.formatHelpDoc(fromEffectDoc(doc)), (name, version) =>
    f.formatVersion(name)(version),
  );

/** `base` with other help and version texts. */
export const withHelp = (
  base: CliOutput.Formatter,
  formatHelpDoc: CliOutput.Formatter["formatHelpDoc"],
  formatVersion: CliOutput.Formatter["formatVersion"] = base.formatVersion,
): CliOutput.Formatter => ({
  formatHelpDoc,
  formatCliError: base.formatCliError,
  formatError: base.formatError,
  formatVersion,
  formatErrors: base.formatErrors,
});

/** `Cli.default_formatter colors`. */
export const cliDefaultFormatter = (colors: boolean): FlickFormatter => {
  const effect = CliOutput.defaultFormatter({ colors });
  return {
    formatHelpDoc: (doc) => effect.formatHelpDoc(toEffectDoc(doc)),
    formatVersion: (name) => (version) => effect.formatVersion(name, version),
  };
};

/** `Cli.display_width`. */
export const cliDisplayWidth = CliOutput.displayWidth;
