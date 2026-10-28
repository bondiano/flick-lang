---
title: "1. Goal and confirmed requirements"
---

| Requirement | Meaning |
| - | - |
| Name and tools | Flick, CLI `flick`, `.fl` sources; a shared Rust core for the compiler and LSP |
| ML-like syntax | Functions, `let`, ADTs, pattern matching and a compact notation for application code |
| HKT | Abstractions over type constructors, including `Functor`, `Foldable`, `Traversable` |
| Type classes | Constraints in signatures, instances and selection of an implementation by types |
| Single backend on Effect | Execution through Effect, its standard types and runtime contracts; pure sections are optimized down to plain JS/TS |
| Part of Effect in the language semantics | Built-in rules for typed recovery, provide, lexical owner and structured concurrency |
| Direct effectful calls | An ordinary `let` binds the result of an expression; the effect of the call is accounted for in the function's type |
| Effect in the arrow | `a -> b` for a pure call; `a ->{r; e} b` for a runtime call |
| Uniform function style | A full signature directly above every named function |
| Higher-rank callbacks | Polymorphic function parameters with an explicit nested `forall` |
| JavaScript numbers | `Float` and `BigInt` with the corresponding JS arithmetic; BigDecimal and other special types are separate |
| Full Schema deriving | Generation for the whole supported data language, including generics and recursion, with explicit policies and extensions |
| Values of computations | `task { ... }` creates an Eff; `await` runs it in the current fiber; application startup lives in the entry point |
| Static resource safety | Forbids scope-dependent values and bound tasks from escaping; fresh regions, transitive captures and two-phase closing are chosen |
| Access to the JS ecosystem | Explicit bindings and a fixed ABI; generation of bindings from a checkable subset of `.d.ts` |
| Optimization | Suitable passes and techniques from ReScript, preserving the chosen semantics of Effect and Float |
| Schema integrations | Extensible compilers/exporters; an unsupported wire contract produces an explicit diagnostic |
| Checking type-level expressiveness | SQL and a typed UI eDSL as real tasks for rows, classes, callbacks and lifetime |

A generic `do` over `Monad`, `for … yield` generators and `[for … yield]` comprehensions are sugar over ordinary calls; `let*` and a single effect-polymorphic map are not part of the language. Function blocks are indentation-based, while the special task/scope/sql/ui/cli blocks use curly braces; the last expression returns the result. The names task and await and an explicit entry point are fixed.

The Flick frontend checks the program, the single Effect backend generates executable JS/TS, and the Content Mapper serves the TS projection and tool integration. Pure calls and Effect calls go through one typed IR. The source type system evolves independently of the expressiveness of TypeScript.
