---
title: "Command-line applications"
---

A `Cli.Command r e` describes a command line: flags, positional arguments,
subcommands and a handler that needs services `r` and fails with errors `e`
(see [CLI eDSL: flags, arguments and subcommands](../../spec/10-type-programming-and-edsl/#cli-edsl-flags-arguments-and-subcommands)). Building one parses and runs nothing; `Cli.run`
parses the arguments with Effect's CLI and only then runs the handler.

```flick
greet : Cli.Command none none
let greet =
  cli "greet" {
    description "Prints a greeting"
    /// Print in capitals
    flag loud : Bool
    /// How many times
    flag times "n" : Number = 1
    /// Who to greet
    arg name : String
    handle fun { loud; times; name } ->
      let text = "Hello, " ++ name
      let line = if loud then String.to_upper text else text
      Platform.stdout_line (String.join "\n" (List.repeat times line))
  }
```

## Clauses

| Clause | Meaning |
| - | - |
| `description e` | the help text of the command, `e : String`; at most one |
| `flag name "alias"… : T [= default]` | the option `--name`; more names follow it |
| `arg name : T [= default]` | the next positional argument |
| `subcommand e` | a nested command, `e : Cli.Command r e` |
| `handle f` | the handler, `f : Options ->{r; e} Unit`; at most one |

A `///` comment right above a `flag`, `arg` or `subcommand` is its help
text. The handler gets an exact record with a field per `flag` and `arg`,
named after the clause; on the command line `dry_run` is `--dry-run`, an
alias of one character `-n`, a longer one `--name`.

| Type | Behaviour | Combinator |
| - | - | - |
| `Bool` | a switch, `false` when absent | `Cli.switch` |
| `T` | required | `Cli.required` |
| `T = d` | `d` when absent | `Cli.default d` |
| `Option T` | `None` when absent; takes no default | `Cli.optional` |
| `List T` | a repeated flag; for an `arg` the rest of the line, last only | `Cli.many` |
| `NonEmpty T` | as `List T`, at least one | `Cli.some` |

A value of type `T` is parsed by the class `Cli.Value T`: instances exist for
`String`, `Bool`, `Number` (no NaN, no empty text), `BigInt`,
`Redacted String`, `DateTime.DateTime` (ISO 8601) and `(String, String)`
(`key=value`, split at the first `=`). `deriving Cli.Value` makes one for a type of constructors
without fields — a choice among their names in kebab case, `GreenLight` is
`green-light` — and for a refined type, which parses its base and checks its
predicate.

## Combinators

The block is sugar: it elaborates into a pipeline of the same functions,
which programs may use directly, and both forms compile to the same IR.

```flick
greet : Cli.Command none none
let greet =
  Cli.command "greet"
    |> Cli.description "Prints a greeting"
    |> Cli.flag (Record.label : Record.Label "loud") [] "Print in capitals" Cli.switch
    |> Cli.flag (Record.label : Record.Label "times") ["n"] "How many times" (Cli.default 1)
    |> Cli.arg (Record.label : Record.Label "name") "Who to greet" Cli.required
    |> Cli.handle (fun { loud; times; name } -> Platform.stdout_line name)
```

`Cli.subcommand command help` adds a subcommand (`help` lists it in its
parent's help); `Cli.group` ends a command without a handler of its own.

Only the combinators reach these:

| Combinator | Meaning |
| - | - |
| `Cli.at_least n`, `Cli.at_most n`, `Cli.between m n` | a `List` of that many values |
| `Cli.from_config config` | one value, read from a `Config.Config` when absent |
| `Cli.from_prompt prompt` | one value, asked with a `Cli.Prompt.Prompt` when absent |
| `Cli.hidden`, `Cli.metavar name` | applied to a multiplicity: left out of help, or the name of the value there |
| `Cli.optional_value text` | a flag written alone takes `text` |
| `Cli.file b`, `Cli.directory b`, `Cli.path b` | the value names a path, which must exist when `b` |
| `Cli.file_text`, `Cli.file_parse format` | `Cli.Value` parses the file's text, or the JSON of its INI, JSON, TOML or YAML data |
| `Cli.short_description`, `Cli.alias`, `Cli.unlisted`, `Cli.example` | how the command shows in help |
| `Cli.action name help act` | the switch `--name`, which runs `act` instead of any handler |

`Cli.help_doc path command` is the `Cli.HelpDoc` record `--help` shows after
the subcommands `path`, `Cli.help` its text, and `Cli.completions shell
command` the completion script. Interactive questions are `Cli.Prompt`:
values such as `Cli.Prompt.text`, `Cli.Prompt.select` or `Cli.Prompt.custom`,
asked by `Cli.Prompt.run` on the service `Cli.Prompt.Tty` (`Cli.Prompt.node`
is the process's terminal; a fake is a `Layer.succeed Tty` record), one key
press at a time. The options record grows through
`Record.Lacks` and `Record.UnionDisjoint`, so a field given twice or a
handler pattern naming a field the command does not have is a type error.

## Running

`Cli.run command args` parses `args` with Effect's CLI and only then runs
the handler of the command they name, in the current fiber, with the
Node.js services Effect's CLI needs. `Cli.run_version version command args`
also gives the text of `--version` (`Cli.run` uses `0.0.0`), and
`Cli.run_with settings` chooses the built-in flags and a `Cli.Formatter`
of help and version as well.

```flick
main : Unit ->{; AppError} Unit
let main () =
  try Cli.run greet (Platform.args ()) with
  | Cli.CliError message -> App.fail message
```

`--help`, `--version` and `--completions bash|zsh|fish` print to stdout and
run no handler. Invalid input — an unknown flag, a missing argument, a value
`Cli.Value` refuses — prints the command's help and fails with
`Cli.CliError` and Effect's text; no handler runs, and `App.fail` turns it
into exit code 1. The flags of a command with subcommands come before the
subcommand's name and are seen only by that command's handler.

A command keeps the captures of its handler: one whose handler holds a
resource of a `scope` runs inside that scope and cannot leave it
(`FLICK-G001`).

## Rules

| Rule | Code |
| - | - |
| a field, long name or alias used twice, or one of `--help`, `-h`, `--version`, `--completions` | `FLICK-L001` |
| `= default` on an `Option` | `FLICK-L002` |
| an `arg` of many values before the last `arg` | `FLICK-L003` |
| `description` or `handle` twice | `FLICK-L004` |
| a command without `handle` and without subcommands | `FLICK-L005` |
| an `arg` beside subcommands | `FLICK-L006` |
| other names on an `arg` | `FLICK-L007` |

A type without `Cli.Value` is `FLICK-C001`, a default of the wrong type
`FLICK-T003`.
