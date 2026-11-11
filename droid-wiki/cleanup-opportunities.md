# Verified cleanup opportunities

Active contributors: bondiano, Vassiliy Kuzenkov — nonbot author names recorded on `origin/main` for contributor guidance and test/task infrastructure.

The most directly verified cleanup is aligning contributor prose with the actual harness and task graph. The comparisons below use `justfile`, `flick-core/tests/corpus.rs`, `flick/tests/e2e.rs`, and their public guidance; they are bounded follow-ups, not a code-quality audit.

## Align validation descriptions

| Verified discrepancy | Current implementation | Bounded follow-up |
| - | - | - |
| `docs/agents/validation.md` says Markdown `flick fail` only requires “some error”; `CONTRIBUTING.md` also describes the weaker requirement | `flick-core/tests/corpus.rs` requires named codes, checks every named code occurs, and rejects other error codes | Update prose to `flick fail CODE`; retain annotated fixtures for exact spans/counts and safe fixes |
| `docs/src/content/docs/contributing/testing.md` describes `just check` as lint plus `cargo test --workspace` | `justfile`: `check = lint + test`; `test = unit + feature-test`; `unit` explicitly excludes `flick-model` | Describe capability tests and the separate model/Lean/e2e gates accurately; update paired public-site translation |
| The same public testing guide describes three execution passes | `flick/tests/e2e.rs` runs JS/TS in **both** debug and release, then typechecks both TS trees; portable cases additionally run Go in both profiles | Describe four Node passes and separate portable/own-runtime paths |
| The comment above `check` in `justfile` says “Everything CI runs” | `.github/workflows/ci.yml` separately schedules e2e, model, Lean, Nix, plugins, database, installation, and platform jobs | Clarify the comment rather than expanding the recipe or weakening separate gates |

These are differences observed in the dirty `main` snapshot based at `6e9096fb`; ongoing source work may change them. The wiki documents current behavior in [testing](how-to-contribute/testing.md) without modifying the upstream guidance.

Validation for such a follow-up should be proportionate: review commands against the actual recipes/harness; check links and whitespace; apply public-site locale/source-hash rules where relevant. Run executable checks only if the follow-up changes executable behavior or checked examples.

## Keep the cache limitation explicit

`library_files` in `flick/src/cache.rs` explicitly skips fetched-library manifests when collecting loader-extension declarations. Its comment notes that files loaded through a fetched library's extension do not affect the build key until `flick.lock` changes.

This is a source-documented limitation, not a newly reproduced bug or a supported hot-reload guarantee. Before changing it:

1. Pin the intended behavior with a fetched-plugin/foreign-extension project regression.
2. Check both positive cache reuse and invalidation after the actual foreign input changes.
3. Keep `flick build --inputs` and artifact-cache inventories consistent.
4. Preserve the immutable fetched-source store contract rather than treating store contents as editable path dependencies.

The implementation context and other cache boundaries are in [caches and artifacts](systems/project-and-cli/caches-and-artifacts.md). No cache code or expectation was changed for this wiki.

## What this page does not conclude

There is no claim that code is unused, dependencies are stale, or large files are inherently poorly designed. No dependency-age analysis, dead-code analysis, full security audit, benchmark, or runtime reproduction was performed. Optional background is already covered by [lore](lore.md) and [formal scope](systems/formal-model.md); a duplicate background page is unnecessary.

## Key source files

| Source | Evidence |
| - | - |
| `docs/agents/validation.md`, `CONTRIBUTING.md` | Older Markdown-failure wording |
| `docs/src/content/docs/contributing/testing.md` | Older task/pass descriptions |
| `justfile` | Real task dependency graph |
| `flick-core/tests/corpus.rs` | Named-code Markdown failure contract |
| `flick/tests/e2e.rs`, `flick/tests/rt/mod.rs` | Execution target/profile selection |
| `flick/src/cache.rs` | Explicit fetched-loader-extension inventory shortcut |
| `.github/workflows/ci.yml` | Gates outside `just check` |

See [development workflow](how-to-contribute/development-workflow.md) before taking a cleanup item into an implementation change.
