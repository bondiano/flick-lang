# Flick for VS Code

Flick language support: a TextMate grammar for highlighting, and a client of
`flick lsp --stdio` for diagnostics, hover, navigation, completion, rename,
quick fixes, formatting, symbols, semantic tokens, folding and inlay hints.

## Requirements

- VS Code 1.91.0 or later.
- The `flick` executable, either on `PATH` or named by `flick.path`. The
  extension starts it as `flick lsp --stdio`; `flick doctor` checks the
  toolchain.

## Install

Download `flick-<version>.vsix` from the
[releases](https://github.com/bondiano/flick-lang/releases) of the matching
Flick version, then:

```sh
code --install-extension flick-<version>.vsix
```

The VSIX contains the language client bundled with the extension, so it needs
no `node_modules` and no other npm package.

## Settings

| Setting | Default | What |
| - | - | - |
| `flick.path` | `flick` | the executable that serves `flick lsp --stdio` |

Files ending in `.fl` are recognized as Flick, with spaces-only indentation,
and are formatted by the server on save.

## Development

The extension directory is a workspace package of the Flick repository:

```sh
ln -s "$PWD/editors/vscode" ~/.vscode/extensions/flick
pnpm install --frozen-lockfile   # the language client
```

Restart VS Code. To package: `node editors/vscode/scripts/package.mjs`, which
bundles `dist/extension.js`, runs the pinned `vsce` and checks the archive.
