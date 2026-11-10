# Glossary

These terms recur across compiler, runtime, and formal documentation. The source pointers identify where to investigate each term.

| Term | Meaning and source |
| - | - |
| CST | Lossless concrete syntax tree, including the structure needed for formatting; `flick-core/src/syntax/cst.rs`. |
| Interface | Exported information shared across module boundaries; `flick-core/src/iface.rs`. |
| Host | Project I/O and plugin boundary; `flick-core/src/project/mod.rs`. |
| Typing | Checker metadata consumed by lowering and editor queries; `flick-core/src/types/typing.rs`. |
| IR | Typed intermediate representation shared by backends and tools; `flick-ir/src/lib.rs`. |
| Stage | Whether an application stage executes purely or through runtime machinery; `flick-ir/src/stage.rs`. |
| Dictionary | Runtime evidence for a class constraint; `flick-ir/src/dict.rs`. |
| Effect row | Function signature information about runtime requirements and failures; `flick-core/src/types/effects/`. |
| Region | Static lifetime/capture discipline checked in `flick-core/src/types/regions/`. |
| Layer | Service construction and composition API; `packages/std/src/Layer.fl`. |
| Lang item | Compiler-recognized library identity; `flick-core/src/types/lang.rs`. |
| Plugin | Compiler extension accessed through the SDK and host expansion boundary; `flick-plugin/src/lib.rs`. |
| RT | Flick's own runtime, distinct from the Effect adapter; `runtime/src/core/` and `runtime/src/rt/`. |
| λFlick | Reference calculus described in `docs/formal/core.md` and modeled by `flick-model/src/`. |
| Safe fix | Diagnostic edit intended to preserve the relevant contract; `flick-core/src/diagnostics/`. |

See [architecture](architecture.md) for the relationships and [features](../features/index.md) for practical capabilities.
