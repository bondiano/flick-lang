# Systems

Active contributors: bondiano, Vassiliy

This section follows the compiler and development tools across crate boundaries. Start with the compiler pipeline, then follow the entry point relevant to your change.

## Subsystems

| System | Scope | Key source files |
| - | - | - |
| [Compiler](compiler/index.md) | Parsing, checking, lowering, optimization, emission, diagnostics | `flick-core/src/lib.rs`, `flick-core/src/build/mod.rs` |
| [Project and CLI](project-and-cli/index.md) | Loading, package management, caching, commands, LSP and FFI | `flick/src/main.rs`, `flick-core/src/project/mod.rs` |
| [Browser tools](browser-tools.md) | Playground, workers, widgets and editor integrations | `flick-wasm/src/lib.rs`, `docs/src/playground/` |
| [Formal model](formal-model.md) | Reference checking, interpretation, translation and proofs | `flick-model/src/lib.rs`, `formal/README.md` |
| [Developer tools](developer-tools.md) | Importers, generators, benchmark and budget harnesses | `justfile`, `tools/` |

## Modification boundaries

Use the owning phase rather than patching generated output. A source-language change can require syntax, checker, formatter, editor grammar, diagnostics, fixtures, both documentation locales, model, and runtime work. `AGENTS.md` and `docs/agents/validation.md` specify which contracts and checks apply.

Runtime implementation details are in [packages](../packages/index.md), while [features](../features/index.md) traces user-facing capabilities across these systems.
