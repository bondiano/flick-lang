# Command-line applications

Active contributors: bondiano, Vassiliy Kuzenkov. Non-bot authors on `origin/main` for CLI plugins, application surfaces, and examples.

`Cli.Command r e` is a pure command description whose handler has explicit service and error requirements. It is separate from the compiler's own native [CLI](../systems/project-and-cli/index.md).

## Description before execution

The `Cli` combinators describe flags, positional arguments, subcommands, documentation, and a handler. The `cli "name" { ... }` plugin expands to those same combinator calls, so option-record types, handler requirements, and class constraints are checked through ordinary Flick signatures.

Only `Cli.run`/`Cli.run_version` parse the provided argument list and invoke an appropriate handler. Constructing or importing a command does not execute it.

| Abstraction | Role |
| - | - |
| `Cli.Spec r e options` | Description being assembled; `options` records the parameters so far |
| `Cli.Command r e` | Completed command |
| `Cli.Value a` | Text parser for one parameter value |
| `Cli.Multi a field` | Multiplicity/default policy turning parsed values into a field |
| `Cli.Prompt.Prompt a` | Interactive question producing `a` when explicitly run |

Multiplicity policies cover switches, required/defaulted/optional values, repeated values, nonempty values, and bounds. Modifiers connect absent options to configuration or an interactive prompt, adjust help/metavariables, and specify file/directory constraints.

`deriving Cli.Value` supports constructor-only choices (names in kebab case) and refined types. It does not magically parse every algebraic datatype. Builtin instances cover primitive values and selected application types.

## Plugin and library ownership

`plugins/cli/src/read.rs` reads clauses and delegates embedded expressions/types to the compiler. `plugins/cli/src/shape.rs` checks command-shape properties that type signatures cannot state: parameter names/aliases, defaults, and structural relationships. `plugins/cli/src/code.rs` emits the combinator pipeline.

The plugin also supplies formatting, completion, hover, and highlighting. These should evolve with the syntax, not be retrofitted only after expansion works. Diagnostics use standard-plugin codes such as `std/L001`; `packages/std/explain/` contains their explanations.

Runtime argument parsing, help, shell completion generation, and terminal execution are library/runtime concerns. A plugin test of generated code is not proof of the interactive parser's behavior.

## Configuration and terminal work

`packages/std/src/Config.fl` describes typed configuration and recoverable configuration errors; providers control where values come from. `Cli.from_config` uses configuration when an option is absent. Redacted values are a distinct type, not permission to print secrets.

`Cli.Prompt` describes validated questions and requires a supplied prompt implementation. A tool explicitly provides `Cli.Prompt.node` to run against the terminal and handles interruption/quit as appropriate. File operations are likewise supplied through a file service.

`examples/cli-tool/src/App/Main.fl` demonstrates this separation: `NOTES_FILE` configuration, typed notes transformations in `examples/cli-tool/src/App/Notes.fl`, file I/O supplied at the composition root, four subcommands, and an explicitly run first-note wizard.

## Modification entry points and validation

| Change | Start here | Evidence |
| - | - | - |
| New clause or syntax policy | `plugins/cli/src/read.rs`, `plugins/cli/src/shape.rs` | `plugins/cli/tests/expand.rs`, formatter/editor behavior |
| Emitted combinator structure | `plugins/cli/src/code.rs` | Exact generated declarations and checker acceptance/rejection |
| New value derivation | `plugins/cli/src/derive.rs`, `packages/std/src/Cli.fl` | Accepted choice/refined types and unsupported shapes |
| Parameter parsing or terminal behavior | `packages/std/src/Cli.fl`, `packages/std/src/Cli/Prompt.fl` and selected runtime | `tests/run/stdlib/cli/`, including parse/exit/prompt cases |
| Example workflow | `examples/cli-tool/src/App/Main.fl` | `examples/cli-tool/test/App/WizardTest.fl`, `examples/cli-tool/test/App/NotesTest.fl` |

Select plugin/host checks for expansion changes and emitted-program/runtime checks for actual command behavior. SDK changes need both native and WASM plugin execution. No CLI/TUI application was launched while writing these wiki pages.

## Key source files

| Path | Purpose |
| - | - |
| `plugins/cli/src/lib.rs` | Standard CLI plugin implementation and editor hooks |
| `plugins/cli/src/layout.rs` | Plugin clause formatting |
| `plugins/cli/src/editor.rs` | Completions, hover, and highlighting |
| `packages/std/src/Cli.fl` | Public descriptions, multiplicities, value parsing, execution |
| `packages/std/src/Cli/Prompt.fl` | Interactive prompt API |
| `packages/std/src/Config.fl` | Typed configuration API |
| `examples/cli-tool/src/App/Main.fl` | Complete notes tool composition |

For the SDK implementing this sugar, see [plugins](../packages/plugins.md).
