# A notes CLI

A command line built with a `cli` block: subcommands with their own
arguments, the notes file chosen by the environment, a wizard that asks for
the first note, and shell completions, all from one description of the
command.

| Part | Where |
| - | - |
| subcommands `init`, `add`, `list`, `done` | `cli "notes" { subcommand … }` in `src/App/Main.fl` |
| the file from the environment | `Config.string "NOTES_FILE"`, `notes.txt` by default |
| the wizard | `notes init` asks with `Cli.Prompt.text`, which refuses a blank note |
| help, version, completions | built in: `--help`, `--version`, `--completions bash` |
| the logic | `src/App/Notes.fl`, pure, so the tests need no file |

## Run it from the repository root

```sh
pnpm install --frozen-lockfile
cargo build -p flick

./target/debug/flick run --project examples/cli-tool -- --help
./target/debug/flick run --project examples/cli-tool -- init
./target/debug/flick run --project examples/cli-tool -- add buy milk
./target/debug/flick run --project examples/cli-tool -- list
./target/debug/flick run --project examples/cli-tool -- done 1
NOTES_FILE=/tmp/work.txt ./target/debug/flick run --project examples/cli-tool -- add review
./target/debug/flick run --project examples/cli-tool -- --completions bash
./target/debug/flick test --project examples/cli-tool
```

## Build a native Go executable

With Go 1.23 or newer on `PATH`, the same source builds without Node.js or
the JavaScript runtime:

```sh
cargo build -p flick
./target/debug/flick build --project examples/cli-tool --emit go
./examples/cli-tool/dist/flick-main --help
./examples/cli-tool/dist/flick-main init
NOTES_FILE=/tmp/work.txt ./examples/cli-tool/dist/flick-main add review
./examples/cli-tool/dist/flick-main --completions bash
./target/debug/flick test --project examples/cli-tool --emit go
```

Set `emit = "go"` in `flick.toml` to make Go the default for `build`, `run`
and `test`. The checked-in example keeps JavaScript as its default.

On Go, the CLI is a port of Effect's: commands, prompts, help, version,
errors and completions for every shell print the same text as on
JavaScript, `--log-level` sets the handler's minimum log level and
`--trace` sends the command's span over OTLP. The built-in `--wizard` is
not ported yet.

## Tests

The tests run the wizard on a fake terminal, a `Cli.Prompt.Tty` that
types a script of keys. `flick/tests/examples.rs` builds and runs both
JavaScript and Go, checks argument failures leave the notes unchanged, and
executes the generated Bash completion script.
