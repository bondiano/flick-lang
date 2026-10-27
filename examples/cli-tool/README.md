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

The tests run the wizard on a fake terminal, a `Cli.Prompt.Tty` that
types a script of keys. `flick/tests/examples.rs` runs the commands with
`NOTES_FILE` set and checks the file they leave.
