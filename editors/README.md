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

## Language server

Neither extension starts a server yet. When `flick lsp` (stdio) exists:

- VS Code: add a `main` entry that starts it with `vscode-languageclient`.
- Zed: add `[language_servers.flick]` (`languages = ["Flick"]`) to
  `extension.toml` and a Rust `src/lib.rs` implementing
  `language_server_command` that finds `flick` on `PATH`.
