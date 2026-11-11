---
title: "Command-line applications"
---

A `Cli.Command r e` describes a command line: flags, positional arguments,
subcommands and a handler that needs services `r` and fails with errors `e`
(see [CLI eDSL: flags, arguments and subcommands](../../spec/08-type-programming-and-edsl/#cli-edsl-flags-arguments-and-subcommands)). Building one parses and runs nothing; `Cli.run`
parses the arguments and only then runs the handler.

The block `cli "name" { … }` is the plugin `cli` of the `std` package (see
[plugins](../plugins/)), which every module has without an import.

```flick
import Cli

greet : Cli.Command none none
let greet =
  cli "greet" {
    description "Prints a greeting"
    /// Print in capitals
    flag loud : Bool
    /// How many times
    flag times "n" : Int = 1
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

Inside a `cli` block, the language server completes all five clause words
with syntax, help and insertion snippets. Hover a clause word for its
syntax and help, or a declared flag or argument name for its written type
and `///` documentation. Clause words and parameter names are highlighted
contextually; types, defaults and handler expressions keep ordinary Flick
completion.

| Type | Behaviour | Combinator |
| - | - | - |
| `Bool` | a switch, `false` when absent | `Cli.switch` |
| `T` | required | `Cli.required` |
| `T = d` | `d` when absent | `Cli.default d` |
| `Option T` | `None` when absent; takes no default | `Cli.optional` |
| `List T` | a repeated flag; for an `arg` the rest of the line, last only | `Cli.many` |
| `NonEmpty T` | as `List T`, at least one | `Cli.some` |

A value of type `T` is parsed by the class `Cli.Value T`: instances exist for
`String`, `Bool`, `Float` (no NaN, no empty text), `BigInt`,
`Redacted String`, `DateTime.DateTime` (ISO 8601) and `(String, String)`
(`key=value`, split at the first `=`). `deriving Cli.Value` makes one for a type of constructors
without fields — a choice among their names in kebab case, `GreenLight` is
`green-light`, or among an enum's literals — and for a refined type, which parses its base and checks its
predicate.

## Combinators

The block is sugar: it stands for calls of the same functions, which
programs may use directly. Each clause is applied to the spec before it,
and `Cli.handled spec handler` ends a command with a handler: the spec comes
first, so the handler is checked against the options its clauses make. A
pipeline written by hand reads as well:

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
the subcommands `path` (the flags of the commands above it in
`inheritedFlags`, shown under `INHERITED FLAGS`), `Cli.help` its text, and
`Cli.completions shell command` the completion script. Interactive questions are `Cli.Prompt`:
values such as `Cli.Prompt.text`, `Cli.Prompt.select` or `Cli.Prompt.custom`,
asked by `Cli.Prompt.run` on the service `Cli.Prompt.Tty` (`Cli.Prompt.node`
is the process's terminal; a fake is a `Layer.succeed Tty` record), one key
press at a time. The options record grows through
`Record.Lacks` and `Record.UnionDisjoint`, so a field given twice or a
handler pattern naming a field the command does not have is a type error.

## Running

`Cli.run command args` parses `args` and only then runs
the handler of the command they name, in the current fiber, with the
Node.js services the CLI needs. `Cli.run_version version command args`
also gives the text of `--version` (`Cli.run` uses `0.0.0`), and
`Cli.run_with settings` chooses the built-in flags and a `Cli.Formatter`
of help and version as well.

```flick
# import App.Greet exposing (greet)
#
main : Unit ->{Async; AppError} Unit
let main () =
  try Cli.run greet (Platform.args ()) with
  | Cli.CliError message -> App.fail message
```

`--help`, `--version` and `--completions bash|zsh|fish|powershell` print to
stdout and run no handler. `--wizard` asks for the command's flags and
arguments one at a time, then runs it; `--log-level debug` (any level from
`all` to `none`) sets the minimum level of the program's log. `--trace`
runs the handler as a span named after its command and sends its spans,
logs and metrics over OTLP to `OTEL_EXPORTER_OTLP_ENDPOINT`
(`http://localhost:4318` by default) as the service `OTEL_SERVICE_NAME`
(the command's name by default); without it nothing loads the exporter.
`Cli.run_with` with `builtIns` leaving out `Cli.TraceFlag` removes it. Invalid input — an unknown flag, a missing argument, a value
`Cli.Value` refuses — prints the command's help and fails with
`Cli.CliError` and its message; no handler runs, and `App.fail` turns it
into exit code 1. The flags of a command with subcommands come before the
subcommand's name and are seen only by that command's handler.

A command needs what its handler uses: one whose handler uses a resource
of a `scope` runs inside that scope and cannot leave it (`FLICK-G001`).

## Rules

| Rule | Code |
| - | - |
| a field, long name or alias used twice, or one of `--help`, `-h`, `--version`, `--completions` | `std/L001` |
| `= default` on an `Option` | `std/L002` |
| an `arg` of many values before the last `arg` | `std/L003` |
| `description` or `handle` twice | `std/L004` |
| a command without `handle` and without subcommands | `std/L005` |
| an `arg` beside subcommands | `std/L006` |
| other names on an `arg` | `std/L007` |
| a clause that is not one of the five, or a block without its name | `std/L009` |

The codes `std/…` are the plugin's; `flick explain std/L001` shows what
each means. A type without `Cli.Value` is `FLICK-C001`, a default of the wrong type
`FLICK-T003`.
