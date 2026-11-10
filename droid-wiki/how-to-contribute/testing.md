# Testing infrastructure

Active contributors: bondiano, Vassiliy Kuzenkov — nonbot author names recorded on `origin/main` for fixtures, Rust integration tests, and runtime tests.

Flick combines annotated diagnostics, executable golden cases, seeded language tests, host integration, browser tests, runtime trace monitoring, and a separately checked model/formal contract. The entry points are `justfile`, `docs/agents/validation.md`, and `docs/src/content/docs/contributing/testing.md`; harness source wins when older prose differs.

## Recipe boundaries

Run from the repository root. `justfile` exports the checkout's `FLICK_STD_PATH`; set `FLICK_STD_PATH="$PWD/packages/std"` yourself for direct Cargo/compiler calls that need it.

| Command | Actual scope |
| - | - |
| `just lint` | Rust format check, workspace/all-target Clippy with warnings denied, isolated capability Clippy, browser-target Clippy, `cargo machete` |
| `just unit` | `cargo test --workspace --exclude flick-model`; ordinary ignored tests stay ignored |
| `just feature-test` | Minimal core plus independently enabled capabilities, without relying on workspace feature unification |
| `just test` | `unit` plus `feature-test` |
| `just check` | `lint` plus `test`, **not** every CI job |
| `just model` | `flick-model` tests including ignored tests: reference execution and generated-program properties |
| `just e2e` | `e2e-js`, `e2e-tests`, `e2e-cases`, `go-test`, `language` |
| `just e2e-prepare` | Builds the CLI in the `e2e` Cargo profile, installs frozen pnpm dependencies, builds runtime/adapter and the declaration importer |
| `just e2e-js` | Browser WASM preparation; recursive JS typecheck/lint/tests; site build, chunk checks, and Playwright |
| `just e2e-tests` | Ignored `flick`/`flick-core` test targets under Nextest, excluding run-case, PostgreSQL, Conduit, dump, and signature-rewrite exceptions |
| `just e2e-cases` | The ignored `run_cases` test only; optional `k/n` shard |
| `just wasm-plugins` | Builds/clippies `flick-test-pairs` as a `wasm32-wasip2` guest, checks host feature, runs plugin integration with Wasmtime |

`flick/tests/ignored_tests.rs` checks that the ignored-test selection does not accidentally strand new suites. `.config/nextest.toml` reserves CPU capacity for tests that run their own pools, keeps CI fail-fast disabled, and writes JUnit to `target/nextest/ci/junit.xml`.

Lean is separate: from `formal/`, run `lake build --wfail`. Neither a normal workspace test nor `just check` substitutes for the model/Lean gates.

## Fixture and test-directory map

Every top-level directory under `tests/` has a distinct role:

| Location | Purpose and consumer |
| - | - |
| `tests/check/pass/`, `tests/check/fail/` | Standalone accepted/rejected checker programs; `flick-core/tests/check.rs` |
| `tests/check/projects/` | Whole manifests, source/test roots, dependency trees, and per-file annotations; same checker harness |
| `tests/syntax/` | Grammar-only `.fl` cases plus reviewed Russian-spec example fingerprints; `flick-core/tests/corpus.rs` |
| `tests/run/` | Executable `.fl` cases and stdout/stderr/exit expectations; `flick/tests/e2e.rs` |
| `tests/language/` | A Flick test project with literal semantic expectations and seeded JS/Go/own-runtime report checks; `just language` |
| `tests/rt/` | Deep calls/control, late effects, layers, and wake-chain programs with their own expectations; selected through `tests/run/rt.txt` and `flick/tests/rt/mod.rs`, including Go checks |
| `tests/ffi/` | `lookup-lib` version/drift fixtures, `import-identity` declaration identity fixtures, and `ts-holes`; `flick/tests/ffi_command.rs` and `tools/dts-importer/test/import.test.mjs` |
| `tests/postgres/` | Real SQL programs exercised against PostgreSQL by `flick/tests/sql_postgres.rs` |
| `tests/plugins/pairs/` | The Rust `flick-test-pairs` plugin fixture, not a language application; native SDK/core tests and WASM host integration |

Additional test homes:

- `flick-core/tests/`: corpus/checker, capabilities, IR round-trip, projections, implicit packages/imports, stdlib signatures, numeric constant evaluation against Node, mutation fuzzing, scaling, and transformer captures.
- `flick/tests/`: CLI/project/cache/package/build/run/test lifecycle tests, editor/LSP tests, FFI, plugin hosting, application examples, PostgreSQL, and Conduit. Shared helpers and split suites live in subdirectories.
- `flick-model/tests/`: reference checking/execution, generated properties, and generic contracts; `tests/run/model.txt` selects the executable cases supported by the model.
- `flick-plugin/tests/flick.rs`: SDK macro/DSL behavior. `flick-plugin/`, `flick-plugin/macros/`, first-party `plugins/`, `flick-ir/`, `flick-pretty/`, and compiler modules also have crate-local/unit tests covered by the applicable Cargo invocation.
- `runtime/test/`, `packages/effect-ts/test/`, and Go `_test.go` files under `runtime-go/`: own RT/pure helpers, Effect integration, and Go runtime contracts.
- `docs/test/`, `docs/e2e/`, and tool package test directories: compiler interop/linking, examples, hydration, content, browser UI, upgrade/import tooling, measurement helpers, and budget policy.
- `editors/tree-sitter-flick/test/` and the Zed crate's tests: lexical grammar and editor launcher; `just tree-sitter` and the CI Zed commands own these.

This is a harness map, not a numerical coverage claim. Counts of test-named files miss inline Rust tests and do not establish behavior exercised.

## Checker fixtures and diagnostic contracts

`flick-core/tests/check.rs` compares expected diagnostics to actual diagnostics by **registered code, exact optional span, and optional message substring**. Missing and unexpected diagnostics fail the fixture.

- Pass fixtures must be annotation-free and diagnostic-free.
- Fail fixtures declare every expected diagnostic. Caret comments mark character columns on the nearest preceding non-annotation line.
- `//~ CODE @L:C-L:C text` supplies a 1-based explicit range with an exclusive end; useful for multi-line spans and columns hidden by the caret comment prefix.
- `//~ CODE text` expects a diagnostic **without** a source span; it is not a wildcard for any location.
- Whole-project fixtures can annotate any participating file; `flick.toml` uses `#` comments, and carries unlocated diagnostics.
- Message matching includes help-only lines appended as `: help` when diagnostic fixes are enabled.

Illustrative annotation syntax, not a checked Flick program:

```text
//               ^^^^ FLICK-T012 expected `Float`
//~ FLICK-E003 @4:1-6:9 effect row is not closed
//~ FLICK-P001 cannot read
```

Columns count characters, not UTF-8 bytes. Unused-code warnings R012–R014 are opt-in per fixture code mention; rejected syntax fixtures similarly opt out of later-phase diagnostics unless named. These are existing harness rules, not permission to suppress a new error. Core codes come from `flick-core/src/diagnostics/codes.rs`; the current harness also recognizes syntactically registered package/plugin codes.

Safe-fix behavior needs a test of edits and the corrected program as well as the original diagnostic. Do not reformat location-sensitive fail/project fixtures blindly.

Focused entry points:

```sh
export FLICK_STD_PATH="$PWD/packages/std"
cargo test -p flick-core --test check
cargo test -p flick-core --test corpus
```

## Executable golden cases

`flick/tests/e2e.rs` builds each selected `tests/run/**/*.fl` and compares actual results to independently reviewed neighboring files:

| Suffix | Meaning |
| - | - |
| `.stdout` | Required exact stdout |
| `.stderr` | Exact stderr; empty if absent |
| `.stderr.contains` | Instead of exact stderr, each listed line must occur |
| `.exit` | Numeric exit code; defaults to `0` |

The inspected harness runs **four Node passes**: JS debug, TS debug, JS release, and TS release. It then strictly typechecks both TS output trees. This is newer than the public testing guide's three-pass description.

Cases in `tests/run/portable.txt` also run as Go executables in debug and release. `FLICK_E2E_GO_ONLY` selects the Go-only route for non-Linux-x64 CI jobs. Traces are checked against the λRT monitor; the runtime cases also compare eligible projected JS/Go traces. Timing reports include per-case build/run data and `times.tsv`.

```sh
# Prepares dependencies/builds, then keeps every pass of matching cases.
FLICK_E2E_ONLY=tests/run/runtime/ FLICK_E2E_JOBS=1 just e2e-cases

# One complete shard; a case's target/profile comparisons stay together.
just e2e-cases 1/3
```

`FLICK_E2E_ONLY` matches a path substring or comma-separated substrings; `FLICK_E2E_JOBS` controls each harness pool. `FLICK_E2E_SHARD=k/n` is set by the shard recipe. Inspect `flick/tests/e2e.rs` before combining filters.

Known gaps in `tests/run/known_gaps.txt` are checked expectations: a listed gap that now passes is reported and should be removed after review. The own-runtime suite similarly checks that newly passing unlisted cases graduate into `tests/run/rt.txt`. Do not add gaps to make a regression green.

## Language, runtime, plugin, and formal checks

| Contract | Checks and prerequisites |
| - | - |
| Shared language semantics | `just language` uses one seed (default `42`), requires both JS/Go reports to pass and match, then compares own RT with explicit `tests/language/rt-gaps.txt` handling |
| Go runtime | `just go-test`: `go vet ./...` and `go test ./...` from `runtime-go/` |
| Runtime laws | `just conformance` runs selected compiled runtime/shared-layer cases and model cases; `just conformance-rt` exercises own RT in both profiles plus applicable Go expectations |
| Model and properties | `just model`, optionally configured by `PROPTEST_CASES`; model-supported outputs are checked against `.stdout`, not only backend agreement |
| Schedule variation | `just stress` varies `FLICK_STRESS`, collects `FLICK_TRACE`, and checks the reference monitor; not in normal CI and not exhaustive schedule exploration |
| Plugin host | `cargo test -p flick --test plugins` plus `just wasm-plugins`; exercise permissions, offline answers, cache invalidation, failure diagnostics, and resource traps |
| PostgreSQL | `just e2e-postgres`; Docker-backed real database |
| Conduit | `just conduit-api` needs Docker/Go/hurl; `just conduit-e2e` needs Docker/Go/Chromium |
| Formal contract | `just model`, then `lake build --wfail` in `formal/`; regenerate Prelude schemes only when needed and review the result |

The `pairs` fixture includes normal expansion/group state, typed/generated errors, watched calls, deriving, published metadata, external reads, foreign-file loading, IR/emit hooks, `spin` fuel exhaustion, and `hog` memory exhaustion. `just wasm-plugins` produces `target/wasm32-wasip2/release/flick_test_pairs.wasm`. A default host build without `wasm-plugins` tests the explicit unavailable-host diagnostic; it does not exercise Wasmtime by itself.

## Documentation, fuzzing, size, and performance

`flick-core/tests/corpus.rs` parses documentation blocks, checks public-site examples independently, round-trips the lossless tree, and checks formatting/AST invariants. Imports in these examples are taken on trust rather than loaded as project dependencies.

- Default blocks must have no errors; hidden `#`/`# ` lines compile but stay off the page.
- `flick fail CODE` must name codes; every named code must occur and unrelated **error** codes are rejected. This does not assert exact spans/counts or forbid every unrelated warning.
- `flick keep` still checks but is preserved by safe-fix rewriting.
- Outside diagnostic-reference pages, `flick ignore reason` is parse-only with an explanatory reason of at least 20 characters. Reference exemptions are fingerprinted/reviewed in `docs/error-examples.txt`.
- Russian-spec example pins live in `tests/syntax/spec_examples.txt`; review changed examples before updating their fingerprints.

The older statement in `docs/agents/validation.md` that a fail block only needs “some error” does not describe the current harness.

For public site changes, select the corpus check, `pnpm -C docs test`, locale parity, `just docs`, and `node docs/scripts/links.mjs` as applicable. Browser behavior uses `just e2e-js` plus `just bundle-size`, with Chromium installed or `CHROMIUM` configured.

`flick-core/tests/fuzz.rs` mutates corpus inputs, checks parser termination/losslessness/span bounds and checker panic resistance. `FLICK_FUZZ_ITERATIONS` controls the run; `just fuzz` defaults to one million, as does nightly CI. `flick-core/tests/scaling.rs` checks bounded large-input time and scaling ratios.

Size/performance checks are separate: `just bundle-size`, `just perf`, and `just ratchet`. The raw browser-WASM limit is in `justfile`; bundle/chunk budgets live in `tools/bundle-size/programs.mjs`, and performance thresholds in `tools/perf/budgets.json`. See [measurement tooling](tooling.md#measurement-and-generated-data).

## Key source files

| Source | Responsibility |
| - | - |
| `justfile`, `.config/nextest.toml` | Suite composition, prerequisites, selection, reports |
| `flick-core/tests/check.rs` | Diagnostic annotations and matching |
| `flick-core/tests/corpus.rs`, `flick-core/tests/fuzz.rs` | Documentation/syntax invariants and mutation tests |
| `flick/tests/e2e.rs`, `flick/tests/rt/mod.rs`, `flick/tests/trace/mod.rs` | Target/profile execution and runtime trace contracts |
| `tests/plugins/pairs/src/lib.rs`, `flick/tests/plugins.rs` | SDK/host fixture and Wasmtime integration |
| `flick/tests/ignored_tests.rs` | Ignored-suite selection guard |
| `flick-model/tests/run.rs`, `flick-model/tests/props.rs` | Model oracle and generated properties |
| `.github/workflows/ci.yml`, `.github/workflows/nightly.yml` | Distributed CI coverage and extended nightly checks |

This page describes the dirty `main` working tree based at `6e9096fb`. No executable suite was run for wiki generation. Use [development workflow](development-workflow.md) to select required gates for an actual code change.
