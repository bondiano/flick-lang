# docs

The Astro/Starlight site (English at `/`, Russian at `/ru/`) with its
playground, tour and courses, and repository-only contract documents.
[CONTRIBUTING.md](../CONTRIBUTING.md#documentation-site) describes the
site, its doctest markers and its locales.

## Layout

- `src/content/docs/` — site pages; each Russian page has the same path
  under `ru/`.
- `src/widgets/`, `src/playground/` — interactive parts, as Flick
  programs; `tour/`, `course/`, `exercises/` — lesson indexes and programs.
- `agents/` (agent guidance), `formal/`, `backends/` (contracts) — English
  only, not site pages.
- `scripts/` — locale parity, generated API and error pages, link and
  chunk checks.

## Rules

- Change a page in both locales with identical code blocks, hidden lines
  and fence markers. After updating a translation, run
  `node docs/scripts/parity.mjs --stamp docs/src/content/docs/ru/<page>.md`;
  under `spec/` the Russian page is the original, so stamp
  `docs/src/content/docs/spec/<page>.md`.
- Pages under `reference/errors/` are generated: edit
  `flick-diagnostics/src/diagnostics/explain/<CODE>.md` and
  `src/error-translations/<CODE>.md`, then run
  `node docs/scripts/errors.mjs --write`.
- `ru/spec/` code blocks are pinned in `tests/syntax/spec_examples.txt`,
  and std signatures are read from its `text` blocks; see the
  [testing guide](src/content/docs/contributing/testing.md).
- Tests and builds load `@flick-lang/runtime` and `@flick-lang/effect-ts`
  from `dist/` and the compiler from `target/debug/flick` (or `FLICK_BIN`).
  Rebuild them first (`pnpm -C runtime build`,
  `pnpm -C packages/effect-ts build`, `cargo build -p flick`); a stale
  `dist/` fails as a missing named export.

## Checks

The "Documentation content" and "Playground" rows of the
[validation matrix](agents/validation.md); its documentation-only section
covers `agents/`.
