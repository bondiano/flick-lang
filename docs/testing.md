# Testing

| Command | What runs |
| - | - |
| `just check` | fmt, clippy (`-D warnings`, also for `wasm32`), `cargo test --workspace` |
| `just e2e` | `pnpm install --frozen-lockfile`, runtime tests (incl. the Effect `4.0.5` pin), `tests/run/**` on Node |

Snapshot tests use [`insta`](https://insta.rs); review changes with
`cargo insta review` or accept them with `INSTA_UPDATE=always cargo test`.

## Checker fixtures — `tests/check/`

- `tests/check/pass/**/*.fl` must check without any diagnostics and must not
  contain annotations.
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

Columns count characters, not bytes. `text` is optional and must be a
substring of the diagnostic message. Codes must be registered in
`flick-core/src/diagnostics/codes.rs`.

## E2E programs — `tests/run/`

Each `<name>.fl` is compiled to ESM and run with `node`. Expected results sit
next to it:

| File | Default when absent |
| - | - |
| `<name>.stdout` | required |
| `<name>.stderr` | empty |
| `<name>.exit` | `0` |

Compiled modules go to `target/tmp/e2e/` and import `@flick-lang/runtime`
from the workspace. Until code generation from the typed IR exists the cases use the bootstrap
subset described in `flick-core/src/bootstrap.rs`.

## Syntax corpus and fuzzing — `flick-core/tests/{corpus,fuzz}.rs`

- Every ```` ```flick ```` block of the specification and of `docs/` must parse
  without diagnostics. Spec examples are pinned by fingerprint in
  `tests/syntax/spec_examples.txt`: a new or edited example fails the test
  until its parse is reviewed and the printed line is added there.
- Every `.fl` file under `tests/` and every doc block must round-trip through
  the lossless tree byte for byte; those without diagnostics must format
  idempotently, and the formatter re-checks that the AST is unchanged.
- `fuzz.rs` mutates the corpus and asserts that the parser does not panic or
  hang, stays lossless and keeps spans in bounds. `FLICK_FUZZ_ITERATIONS`
  sets the number of inputs: 20 000 in `just check`, 1 000 000 in the nightly
  CI job (`just fuzz`).
- `DUMP=out.txt [DUMP_FILE=x.fl] cargo test -p flick-core --test corpus dump -- --ignored`
  writes the trees of the spec examples (or of one file) for inspection.
