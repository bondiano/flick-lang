# Editor support

| Directory | What |
| - | - |
| `vscode/` | VS Code extension: TextMate grammar, brackets, comments, spaces-only indentation |
| `tree-sitter-flick/` | Token-level tree-sitter grammar (brackets nest, everything else is flat) |
| `zed/` | Zed extension built on `tree-sitter-flick` |

Both highlight lexically; precise highlighting (types vs constructors,
definitions) is meant to come from the language server's semantic tokens.

## VS Code

```sh
ln -s "$PWD/editors/vscode" ~/.vscode/extensions/flick
```

Restart VS Code. To package: `npx @vscode/vsce package` inside `editors/vscode`.

## Zed

Zed builds the grammar from git, so `rev` in `zed/extension.toml` must name a
pushed commit that contains `editors/tree-sitter-flick` (or set `repository`
to `file:///path/to/flick-lang` while developing). Then run
`zed: install dev extension` and pick `editors/zed`.

After changing `grammar.js` or `src/scanner.c`, run `just tree-sitter`
(needs the `tree-sitter` CLI) and commit the regenerated `src/`.

The `Release` workflow publishes `flick` archives for macOS, Linux and Windows
(aarch64 and x86_64) when a `v*` tag is pushed. The tag must match the versions
in `flick/Cargo.toml`, `zed/Cargo.toml` and `zed/extension.toml`: the extension
downloads that exact release, not the latest one.

## Language server

`flick lsp --stdio` serves diagnostics (errors and warnings, as `flick
check` reports them, on every change), hover with the source type and the
`///` documentation (the inferred type of a local, a parameter or a pattern
variable), definition, references, completion, rename and formatting (see
`docs/src/content/docs/reference/editor.md`). Both editors format on save
with it: VS Code through the extension's defaults, Zed by its default
`format_on_save` with the language server as formatter.

- VS Code: `extension.js` starts it with `vscode-languageclient` (installed
  by `pnpm install` at the repository root, as a workspace package) and shows built-in modules
  (`flick-builtin:///List.fl`) as read-only documents. `flick.path` names
  the executable.
- Zed: `src/lib.rs` uses an explicit `lsp.flick.binary.path`, then the
  worktree's `PATH`, otherwise a matching release, with an offline cache.
  See [local configuration](../docs/src/content/docs/reference/editor.md#zed).
  Zed builds the extension itself when installed as a dev extension;
  `just zed` checks that it compiles. Definitions in built-in modules
  (`flick-builtin:` documents) do not open in Zed, which has no virtual documents.
