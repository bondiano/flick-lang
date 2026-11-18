# flick-test-support

Helpers the Rust test suites share: `files` collects fixtures under a
directory and `entries` reads a list of cases. It is a dev-dependency of
`flick`, `flick-core` and `flick-model`.

## Rules

- Keep it free of dependencies on workspace crates. `flick-core`'s own
  unit tests (`src/ir/tests.rs`, `src/fmt/tests/`) use it, and a
  dependency back would build a second copy of `flick-core` into them.
- Fixtures belong in the suites' directories, mostly `tests/` at the
  repository root; add a helper here only when several suites share it.
- `files` returns sorted paths and skips hidden directories,
  `node_modules`, `dist` and `target`; an unreadable directory yields
  nothing: assert that a suite found cases, as
  `flick-core/tests/corpus.rs` does.
- `entries` drops everything from the first `#` on a line and blank lines,
  so a case name cannot contain `#`. Lists such as
  `tests/run/portable.txt` and `tests/run/known_gaps.txt` rely on it.

## Checks

- `cargo test -p flick-test-support`
- A change of what `files` or `entries` returns changes the cases of
  every suite that calls it. Find them with
  `grep -rln flick_test_support flick flick-core flick-model` and run
  them with the rows of the [validation matrix](../docs/agents/validation.md)
  that cover those fixtures; compare the case counts before and after.
