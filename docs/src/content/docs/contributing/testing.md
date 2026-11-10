---
title: "Testing"
---

Install the dependency checker used locally and in CI:

```sh
cargo install cargo-machete --version 0.9.2 --locked
```

| Command | What runs |
| - | - |
| `just lint` | fmt, clippy (`-D warnings`, also for `wasm32`), `cargo machete` (unused Rust dependencies) |
| `just check` | `just lint`, `cargo test --workspace` |
| `just e2e` | `pnpm install --frozen-lockfile`, runtime tests, `tests/run/**` on Node, `flick test` on sample projects, `just language` |
| `just language` | the language tests of `tests/language` on Node and as a Go executable, which must pass with the same report |
| `just bundle-size` | release builds of `tools/bundle-size/pure.fl`, `tests/run/ui/counter.fl`, `tests/run/ui/atoms.fl` and the playground, bundled for the browser with rolldown; fails when a gzip size exceeds its budget in `tools/bundle-size/programs.mjs` |

Snapshot tests use [`insta`](https://insta.rs); review changes with
`cargo insta review` or accept them with `INSTA_UPDATE=always cargo test`.

## Checker fixtures — `tests/check/`

- `tests/check/pass/**/*.fl` must check without any diagnostics and must not
  contain annotations. Each file is checked on its own, so it declares the
  types and services it uses.
- `tests/check/fail/**/*.fl` declare every expected diagnostic in annotation
  comments. Missing and unexpected diagnostics are reported per file.

Annotation forms:

```flick fail
let x : Float = "hi"
//               ^^^^ FLICK-T012 expected `Float`
//~ FLICK-E003 @4:1-6:9 effect row is not closed
//~ FLICK-P001 cannot read
```

- **Carets** `// ^^^ CODE text` mark columns of the nearest preceding line that
  is not an annotation. Several caret lines may follow one source line.
- **Explicit** `//~ CODE @L:C-L:C text` gives a 1-based range with the end
  column exclusive; use it for multi-line spans and for spans starting in
  columns 1–2, which the `//` of a caret line covers.
- **No span** `//~ CODE text` matches a diagnostic without a source location.

Each directory of `tests/check/projects/` is a whole project, checked like
`flick check` without paths: `flick.toml`, the source and test roots, and a
`package.json` with a fake `node_modules/` when a case needs packages. Any of
its files may carry annotations, `flick.toml` with `#` instead of `//`;
diagnostics without a span are annotated in `flick.toml`. A project without
annotations must check cleanly.

Columns count characters, not bytes. `text` is optional and must be a
substring of the diagnostic message, with each help line appended as
`: help`. Warnings about unused code (`FLICK-R012`–`R014`) count only in
files that mention their code: fixtures of other rules bind names they never
use. Codes must be registered in
`flick-core/src/diagnostics/codes.rs`.

## E2E programs — `tests/run/`

Each `<name>.fl` is built with `flick build <name>.fl --out-dir …` and its
runner is run with `node --enable-source-maps`. Expected results sit next
to it:

| File | Default when absent |
| - | - |
| `<name>.stdout` | required |
| `<name>.stderr` | empty |
| `<name>.stderr.contains` | instead of `<name>.stderr`: lines stderr must contain |
| `<name>.exit` | `0` |

Every case runs three times: compiled to JS, compiled with `--emit ts`, whose
`flick-main.ts` Node runs directly, and compiled to JS in the release
profile, which must behave as the debug build. Within each pass the cases
run in parallel, `FLICK_E2E_JOBS` at a time (default: the number of CPUs;
`FLICK_E2E_JOBS=1` runs them one by one). Afterwards `tsc --noEmit` (strict) checks all of
the TypeScript output. Output goes to `target/tmp/e2e/` and imports `@flick-lang/runtime` from the
workspace. A foreign module next to a case (`./tags.mjs`) is imported from
there. `just e2e` also builds every `tests/check/pass/**` fixture and runs
`node --check` on the output, and `e2e_numbers.rs` compares random `Float`
arithmetic, compiled from Flick, with the same expressions in JS.

## `flick test` — `flick/tests/test_command.rs`

Builds sample projects in `target/tmp/test_command/` and runs `flick test` on
them: each kind of outcome (assertion, typed failure, defect, failed cleanup)
is reported apart, a seed draws the same values again, `--filter` and
`--jobs` apply, and under `--watch` the next run starts only after the
previous one released its resources.

The standard library declares every signature the specification gives for
it word for word: `flick-core/tests/stdlib_signatures.rs` reads them from the
Russian spec's (`ru/spec`) `text` blocks.

## Language tests — `tests/language/`

A Flick project whose test modules, one per area of the language
(`Language.Ints`, `Language.Strings`, `Language.Effects`, …), pin its
semantics with literal expected values: `Int` wrapping, how `show` writes
numbers, UTF-16 strings, equality, records, patterns, recursion, hash
order, JSON and schema errors, effects and the cases a seed draws for
property tests. `just language` runs them with `flick test --emit js` and
`--emit go` and one `--seed`; both runs must pass, and their reports must be
the same byte for byte. A test belongs here when both targets must agree
on it; a difference between them is fixed in a runtime, not in the test.

`flick/tests/unicode_path.rs` builds, runs and tests a project in
`target/tmp/проект ⭐` on both targets, as Gleam's `test/unicode_path ⭐`,
and checks that the source map and Go's `//line` directives still lead to
its `.fl` files.

## Runtime tests — `runtime/test/`

The runtime is TypeScript 7 (`runtime/src/*.ts`), built to `runtime/dist/`
with declarations; `just e2e` type-checks it with `tsc` (strict, with
`noUncheckedIndexedAccess` and `exactOptionalPropertyTypes`, tests included),
lints it with `oxlint --type-aware` (no `any`, no floating promises, and an
unchecked type assertion only with its reason beside it) and runs the tests.

`node --test` in `runtime/` runs the `.test.ts` files on Node's type
stripping, after building `dist/`, and checks the adapter's
contracts directly on Effect: the recovery table of `try … with`, inert cause
reports, the two-phase close of owners, children and supervisors,
concurrency, races and layers, the Effect `4.1.2` pin and its capability
matrix, codecs against their compiled decoders. Entry runner tests start
`runtime/fixtures/entry.ts` in a child process and send it signals.

## Syntax corpus and fuzzing — `flick-core/tests/{corpus,fuzz}.rs`

- Every ```` ```flick ```` block of `docs/` must parse without diagnostics,
  and each block of the site's pages must check on its own
  (`docs_examples_check`), its imports taken on trust: hidden lines, `#`
  alone or starting with `# `, declare what it uses and are left off the
  page. A `flick ignore` block is only parsed; a `flick fail` block must
  report an error; a block with the word `live` checks under the live
  discipline of regions, which a build opts into with `[build] regions`
  (`flick live`, `flick live fail`), and `flick check --fix` leaves it as
  written.
  The blocks of the Russian specification (`ru/spec`), the original text, are
  pinned by fingerprint in `tests/syntax/spec_examples.txt`: a new or edited
  example fails the test until its parse is reviewed and the printed line is
  added there, and a pin whose example is gone fails it too.
- Every `.fl` file under `tests/` and every doc block must round-trip through
  the lossless tree byte for byte; those without diagnostics must format
  idempotently, and the formatter re-checks that the AST is unchanged.
  `fmt`'s own corpus test formats every `.fl` file of the repository and
  fails when one is not a fixed point or a declaration is left as written,
  and `fl_sources_are_formatted` keeps the sources formatted, all but the
  fixtures of checker failures (`tests/check/fail`, `tests/check/projects`)
  and of the editor grammar, whose annotations point at lines and columns.
- `tests/syntax/*.fl` exercise the grammar only: they must parse without
  diagnostics, but their names need not resolve.
- `fuzz.rs` mutates the corpus and asserts that the parser does not panic or
  hang, stays lossless and keeps spans in bounds, and that the checker does
  not panic on inputs that parse. `FLICK_FUZZ_ITERATIONS`
  sets the number of inputs: 20 000 in `just check`, 1 000 000 in the nightly
  CI job (`just fuzz`).
- `DUMP=out.txt [DUMP_FILE=x.fl] cargo test -p flick-core --test corpus dump -- --ignored`
  writes the trees of the spec examples (or of one file) for inspection.

## Checking time — `flick-core/tests/scaling.rs`

Checks a record of 200 fields read field by field and a function of 2000
lines, each also at a quarter of the size: the full size must stay within a
time budget, and the ratio between the sizes must stay linear.

## What the test suites cover

Where to look for the tests of a behavior, and where a new one goes:

| Behavior | Tests |
| - | - |
| kinds, instance resolution and coherence | `tests/check/{pass,fail}/types/kinds.fl`, `tests/check/fail/classes/` |
| cold tasks, direct calls binding results, stage and evaluation order | `tests/run/codegen/{task_twice,stage_order,eval_order}.fl`, `tests/check/{pass,fail}/effects/`, `flick/tests/first_slice.rs` over `examples/first-slice/` |
| error rows and `Cause`, services and layers | `tests/run/stdlib/effect/`, `tests/run/codegen/{errors,defect,services}.fl`, `runtime/test/{cause,layer}.test.ts` |
| scopes, owners, `parallel` and races | `tests/run/runtime/`, `runtime/test/{scope,concurrency,race}.test.ts` |
| regions and captures | `tests/check/{pass,fail}/regions/`, `tests/check/projects/regions/` |
| `Float` semantics in debug, release and constant folding | `flick/tests/e2e_numbers.rs`, `tests/run/codegen/numbers.fl`, `flick-core/tests/consteval_node.rs` |
| optimizer passes | `flick-core/src/opt/tests.rs`, `tests/run/codegen/release.fl` |
| `deriving Schema`, codec policies, strict export, OpenAPI | `tests/{check,run}/**/schema/`, `runtime/test/{schema,httpapi}.test.ts`, `flick/tests/schema_export.rs` |
| FFI callbacks, `.d.ts` import and reviewed bindings | `tests/run/ffi/`, `runtime/test/callbacks.test.ts`, `flick-core/src/ffi/tests.rs`, `flick/tests/ffi_command.rs`, `tools/dts-importer/test/` |
| SQL | `tests/check/{pass,fail}/sql/`, `tests/run/sql/`, `runtime/test/sql.test.ts`, `flick/tests/sql_postgres.rs` over `tests/postgres/` |
| UI | `tests/check/{pass,fail}/ui/`, `tests/run/ui/`, `runtime/test/ui-*.test.ts` |
| the `cli` block | `tests/check/{pass,fail}/cli/`, `tests/run/stdlib/cli/`, `flick-core/src/project/load/plugins_tests.rs` (the calls it stands for), `flick/tests/lsp_editor/cli.rs` |
| hover, the language server and the TypeScript projection | `flick-core/src/ide/tests.rs`, `flick/tests/{lsp_command.rs,lsp_editor/}`, `flick-core/tests/projection.rs`, `flick-core/src/codegen/ts/tests.rs` |
| the CLI: no code run by `check`/`build`, run generations, `dev`, cache, pnpm, exit codes, NDJSON | `flick/tests/{cli_project,cli_run,check_command}.rs` |
| `.d.ts` façades and `pack` | `flick/tests/dts_command.rs`, `flick/tests/cli_run.rs` |
