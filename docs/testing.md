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
from the workspace. Until the real codegen (M7) the cases use the bootstrap
subset described in `flick-core/src/bootstrap.rs`.
