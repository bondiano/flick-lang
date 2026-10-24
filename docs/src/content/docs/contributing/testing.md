---
title: "Testing"
---

| Command | What runs |
| - | - |
| `just check` | fmt, clippy (`-D warnings`, also for `wasm32`), `cargo test --workspace` |
| `just e2e` | `pnpm install --frozen-lockfile`, runtime tests, `tests/run/**` on Node, `flick test` on sample projects |

Snapshot tests use [`insta`](https://insta.rs); review changes with
`cargo insta review` or accept them with `INSTA_UPDATE=always cargo test`.

## Checker fixtures — `tests/check/`

- `tests/check/pass/**/*.fl` must check without any diagnostics and must not
  contain annotations. Each file is checked on its own, so it declares the
  types and services it uses.
- `tests/check/fail/**/*.fl` declare every expected diagnostic in annotation
  comments. Missing and unexpected diagnostics are reported per file.

Annotation forms:

```flick
let x : Number = "hi"
//               ^^^^ FLICK-T012 expected `Number`
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
`node --check` on the output, and `e2e_numbers.rs` compares random `Number`
arithmetic, compiled from Flick, with the same expressions in JS.

## `flick test` — `flick/tests/test_command.rs`

Builds sample projects in `target/tmp/test_command/` and runs `flick test` on
them: each kind of outcome (assertion, typed failure, defect, failed cleanup)
is reported apart, a seed draws the same values again, `--filter` and
`--jobs` apply, and under `--watch` the next run starts only after the
previous one released its resources.

The standard library declares every signature the specification gives for
it word for word: `flick-core/tests/stdlib_signatures.rs` reads them from the
spec's `text` blocks.

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
concurrency, races and layers, the Effect `4.0.5` pin and its capability
matrix, codecs against their compiled decoders. Entry runner tests start
`runtime/fixtures/entry.ts` in a child process and send it signals.

## Syntax corpus and fuzzing — `flick-core/tests/{corpus,fuzz}.rs`

- Every ```` ```flick ```` block of the specification and of `docs/` must parse
  without diagnostics. Spec examples are pinned by fingerprint in
  `tests/syntax/spec_examples.txt`: a new or edited example fails the test
  until its parse is reviewed and the printed line is added there.
- Every `.fl` file under `tests/` and every doc block must round-trip through
  the lossless tree byte for byte; those without diagnostics must format
  idempotently, and the formatter re-checks that the AST is unchanged.
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
