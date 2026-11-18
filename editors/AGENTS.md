# editors

Highlighting and editor packaging: the VS Code extension, the tree-sitter
grammar and the Zed extension built on it. Language features come from
`flick lsp` (`flick/src/`, `flick-core/src/ide/`), not from here.
[README.md](README.md) describes each part.

## Layout

- `vscode/syntaxes/flick.tmLanguage.json` — TextMate grammar; the site and
  playground highlight with it too (`docs/astro.config.mjs`,
  `docs/src/playground/highlight.ts`).
- `tree-sitter-flick/` — token-level grammar: `grammar.js` and the
  hand-written `src/scanner.c`; the rest of `src/` is generated and
  committed.
- `zed/` — the Zed extension; `zed/languages/flick/highlights.scm` is also
  the tree-sitter grammar's highlight query.

## Rules

- For a syntax change, update the TextMate grammar, `grammar.js` and the
  Zed queries together.
- Keep both grammars lexical; types, constructors and definitions are
  colored by the language server's semantic tokens.
- After editing `grammar.js` or `src/scanner.c`, run `just tree-sitter`
  and keep the regenerated `src/`; do not edit generated files by hand.
- `rev` in `zed/extension.toml` must name a pushed commit that contains
  the grammar. Set it after that commit is pushed, and again after a
  rebase or squash changes the commit.

## Checks

- `pnpm -C editors/vscode test` — grammar, extension and VSIX checks.
- `just tree-sitter` — needs the `tree-sitter` CLI, which neither the Nix
  dev shell nor CI provides; report the check as skipped without it.
- `just zed` and
  `CARGO_TARGET_DIR=target/zed cargo test --manifest-path editors/zed/Cargo.toml --locked`,
  as CI runs them.
- A TextMate change also reaches the site: `pnpm -C docs test`.
