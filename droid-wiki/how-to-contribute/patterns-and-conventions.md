# Patterns and conventions

Compiler changes must preserve diagnostics, execution contracts, and the supported editor/browser entry points. `AGENTS.md` and `docs/agents/compiler-workflow.md` define the repository-wide rules.

## Boundaries

Keep filesystem and host behavior behind `Host` in `flick-core/src/project/mod.rs`. The same compiler must work for the native CLI and the browser. Optional Cargo capabilities in `flick-core/Cargo.toml` must remain independently usable; workspace feature unification does not prove a minimal build works.

The checker produces metadata rather than forcing code generation to rediscover typing decisions. `flick-core/src/ir/mod.rs` describes left-to-right evaluation order and explicit application stages. Keep that contract when modifying lowering or optimization.

## Diagnostics and tests

Preserve diagnostic codes, spans, and safe fixes unless changing them intentionally. Add a failing regression before fixing a bug. Use annotated cases under `tests/check/fail/` when a rejection's exact code and location matter. The current corpus harness in `flick-core/tests/corpus.rs` requires `flick fail CODE` examples to report the named codes without unrelated errors; it does not pin source spans. This is stricter than the older description in `docs/agents/validation.md`.

Expected execution results must come from the contract, not from copying whatever the compiler printed. Backend agreement can preserve the same bug twice. `docs/agents/validation.md` describes independent model and runtime checks.

## Style

`Cargo.toml` forbids unsafe Rust, denies unused must-use values, and configures Clippy. `rustfmt.toml` configures formatting. Use the existing abstractions and dependencies before adding alternatives.

Commits use Conventional Commits, as required by `AGENTS.md`. Do not commit, publish, or overwrite another task's changes without authorization.

## Documentation

English and Russian site pages are paired under `docs/src/content/docs/`. The Russian specification is the original for `spec/`; English is the original elsewhere. `docs/scripts/parity.mjs` checks pairing and source hashes.

Interactive documentation behavior belongs in Flick under `docs/src/widgets/` and `docs/src/playground/`; TypeScript is for mounting and browser/compiler interoperability.

## Formal scope

Changes to typing, effects, regions, or execution semantics must account for `flick-model/`, `docs/formal/`, and `formal/`. Explain why unchanged contracts need no proof edits. Never weaken theorem premises or add proof gaps to hide a failing guarantee.

See [testing](testing.md) and [development workflow](development-workflow.md) for the concrete change loop.
