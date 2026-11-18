# flick-pretty

The arena-based Wadler-Lindig printer behind the Flick formatter
(`flick-core/src/fmt/`) and the JS/Go emitters (`backends/*/src/codegen/`).
The crate docs in `src/lib.rs` describe the document model.

## Rules

- Keep it language-neutral: Flick layout belongs in `flick-core/src/fmt/`,
  target layout in the emitters.
- Preserve the guarantees in `src/lib.rs`: linear time, widths in grapheme
  clusters, no whitespace the document did not ask for.
- A printer change can reformat `.fl` sources and shift generated JS/Go;
  review those diffs. The official components in `backends/*/package/`
  embed this crate: rebuild them with `just update-component js` and
  `just update-component go`.

## Checks

- `cargo test -p flick-pretty` (unit tests and the crate doctest).
- `cargo test -p flick-pretty --release -- --ignored --nocapture` runs the
  linear-time check.
- `cargo test -p flick-core --lib fmt::`,
  `cargo test -p flick-core --test corpus` and
  `cargo test -p flick-core --lib codegen::`.
- `just check-component js` and `just check-component go`. When emitted
  code changes, also the "IR, lowering or optimization" row of the
  [validation matrix](../docs/agents/validation.md).
