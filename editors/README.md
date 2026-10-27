# Editor support

| Directory | What |
| - | - |
| `vscode/` | VS Code extension: TextMate grammar, brackets, comments, spaces-only indentation |
| `tree-sitter-flick/` | Token-level tree-sitter grammar (brackets nest, everything else is flat) |
| `zed/` | Zed extension built on `tree-sitter-flick` |

Both highlight lexically; precise highlighting (types vs constructors,
definitions) is meant to come from the language server's semantic tokens.

## VS Code

Requires VS Code 1.82.0 or later, matching `vscode-languageclient`.

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
The command also checks `test/highlight` assertions against Zed's
`highlights.scm`; parser corpus tests alone do not validate that query.
The `cli` clause words (`description`, `flag`, `arg`, …) still highlight as
plain variables: a keyword capture on the same identifier collides with the
generic identifier rule, so precise coloring belongs to the language server's
semantic tokens instead.

The `Release` workflow publishes `flick` archives for macOS, Linux and Windows
(aarch64 and x86_64) when a `v*` tag is pushed. The tag must match the versions
in `flick/Cargo.toml`, `zed/Cargo.toml` and `zed/extension.toml`: the extension
downloads that exact release, not the latest one.

## Language server

`flick lsp --stdio` serves diagnostics (errors and warnings, as `flick
check` reports them, when a document is opened or saved), hover with the source type and the
`///` documentation of public and private values (including inferred locals
and the `ignore` keyword), generic type aliases and decoded `codec` fields,
definition, type definition, references, completion,
signature help, rename, quick fixes and refactors, document and workspace
symbols, semantic tokens, folding, inlay hints and formatting (see
`docs/src/content/docs/reference/editor.md`). Both editors format on save
with it: VS Code through the extension's defaults, Zed by its default
`format_on_save` with the language server as formatter.

- VS Code: `extension.js` starts it with `vscode-languageclient` (installed
  by `pnpm install` at the repository root, as a workspace package).
  `flick.path` names the executable.
- Zed: `src/lib.rs` uses an explicit `lsp.flick.binary.path`, then the
  worktree's `PATH`, otherwise a matching release, with an offline cache.
  See [local configuration](../docs/src/content/docs/reference/editor.md#zed).
  Zed builds the extension itself when installed as a dev extension;
  `just zed` checks that it compiles.

Built-in module definitions open as real read-only `.fl` files in both
editors. The server writes its embedded sources to a content-keyed system
temporary directory; no virtual-document provider is needed. Hover on a
module qualifier shows its `///` documentation. SQL blocks complete tables,
aliases, columns and parameters, with the same nullable types as the checker.
