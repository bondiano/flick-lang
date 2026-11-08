---
title: "Flick Specification"
source_hash: fa485c41aa68276a
---

This specification is the normative description of the Flick language as implemented by this compiler. Its chapters state the rules every program follows, with the reasoning a reader needs to apply them. The [reference](../reference/grammar/) pages describe the same language in more detail: the exact grammar, the diagnostics of the checker, the standard library and the code each target generates. Where a chapter and a reference page cover one topic, the chapter states the rule and links the page.

## Design goals

- **Name and tools.** Flick, the CLI `flick`, `.fl` sources; one Rust core serves the compiler and the language server.
- **ML-like syntax.** Functions, `let`, ADTs and pattern matching. Function bodies and `match` are laid out by indentation, records and special blocks use braces, and the last expression of a block is its result.
- **HKT and type classes.** Abstractions over type constructors such as `Functor`, `Foldable` and `Traversable`; constraints in signatures and instances selected by types.
- **Direct effectful calls.** An ordinary `let` binds the result of a runtime call. The effect lives in the arrow: `a -> b` is a pure call, `a ->{r; e} b` a runtime call that needs services `r` and fails with errors `e`.
- **One function style.** A full signature directly above every named function; higher-rank callbacks take an explicit nested `forall`.
- **Computations are values.** `task { ... }` builds a cold `Eff`, `await` runs it in the current fiber, and the application starts at its entry point.
- **Execution rules in the language.** Typed recovery, `provide`, lexical ownership and structured concurrency are rules of Flick, the same on every backend.
- **Numbers and strings.** `Int` is 32 bits with wrap-around, `Float` is IEEE 754 binary64 and `BigInt` has any size, each with the arithmetic of its JS counterpart; `==` on numbers is SameValueZero; string lengths and indices count UTF-16 code units. The same holds on every backend.
- **Static resource safety.** Scope-dependent values and bound tasks cannot escape their scope: fresh regions, transitive captures and two-phase closing.
- **Several backends, one semantics.** The frontend lowers a checked program to one typed IR. The JS backend generates JS or TS on Effect and compiles pure code to plain JS; the Go backend builds one native executable per entry. Where a chapter names Effect, it describes how the JS backend realizes a rule; the rule itself binds every backend.
- **Access to the JS ecosystem.** Explicit `foreign` bindings with a fixed ABI, and bindings generated from a checked subset of `.d.ts`.
- **Full Schema deriving.** Schemas for the whole supported data language, generics and recursion included; an external format that cannot express a schema is an explicit diagnostic.

## Key decisions

- **Sequencing:** direct calls with `let`, `task`/`await`, `do` over `Monad` with pure continuations, `for` generators and comprehensions ([Direct effectful calls](./01-direct-calls/), [Higher-order functions](./04-higher-order/)).
- **Computation type:** `Eff r e a`, the result last ([Values of computations](./02-computations/)).
- **Arrows:** pure `a -> b`, runtime `a ->{r; e} b`, and the Sync stage `a ->{Atom.Read;} b`, which compiles to a plain JS function ([Sync stages](./01-direct-calls/#sync-stages)).
- **Classes:** multi-parameter classes, superclasses and functional dependencies; no overlapping, orphan or local instances, ownership by package ([classes](../reference/classes/)).
- **Generalization:** only provably pure constructions generalize; results of runtime calls are monomorphic ([types](./03-types-and-classes/)).
- **Records and collections:** exact structural records with row polymorphism and no implicit width subtyping; `List` is a dense immutable array, `Chunk` a separate type for repeated concatenation ([stdlib](../reference/stdlib/)).
- **Higher-order API:** `List.map` takes a pure callback, `List.map_effect` runs a runtime callback in order, `parallel_all` takes an explicit concurrency limit; there is no effect-polymorphic `map` ([Higher-order functions](./04-higher-order/)).
- **Lifetime:** fresh regions and captures, no early close ([regions](../reference/regions/)).
- **Errors and concurrency:** typed recovery over an opaque `Failure`, `Exit` and `RawCause`, the full `Cause` on every backend; structured combinators, a scoped supervisor, `race` and `timeout`; atomic `Ref`; layers shared by identity and memoized ([Errors, services, resources and concurrency](./05-errors-and-concurrency/)).
- **FFI:** `foreign` declarations with the modes `pure`, `sync`, `async`, `effect` and `blocked`, checked against `.d.ts` ([FFI](../reference/ffi/)).
- **Schema:** `deriving Schema`, explicit codecs, strict exporters ([schemas](../reference/schema/)).
- **SQL:** a typed PostgreSQL eDSL whose queries are pure values ([SQL](../reference/sql/)).
- **UI:** `ui { ... }` on React, state in `effect/reactivity` atoms, actions with an explicit event policy, server rendering with a registry per request; no JSX and no React hooks ([user interfaces](../reference/ui/)).
- **Components:** `component name (props : P) = body`, whose setup runs once per instance and whose last expression renders ([components](../reference/ui/#components)).
- **Server functions:** `server fn`, a block of std's plugin `server`, declares a function that runs on the server and that a client calls by name, with the endpoint and codecs generated from one declaration ([server functions](../reference/server-fn/)).
- **RPC and HTTP APIs:** `Rpc` and `HttpApi` describe a contract once as values that client and server share; a missing or extra handler is a type error ([RPC](../reference/rpc/), [HTTP APIs](../reference/http-api/)).
- **Routes and forms:** `deriving Url` makes a page's URL a typed value, `deriving Form` a record a text form ([routes](../reference/route/), [forms](../reference/form/)).
- **Observability:** traces, logs and metrics through ordinary runtime functions, exported by OTLP, DevTools or Prometheus ([observability](../reference/observability/)).
- **Command-line applications:** `cli "name" { ... }`, a plugin of `std`, on Effect CLI, with an exact record of options ([command-line applications](../reference/cli-dsl/)).
- **Compiler:** a Rust core, a typed IR, a language server over the source, a versioned JSON `.iface` per module ([Compiler architecture](../contributing/implementation-decisions/#compiler-architecture)).
- **Backends:** `[build] emit` selects `"js"` (the default), `"ts"` or `"go"`; importing a module the target does not support is a compile error ([Go target](../reference/codegen/#go-target)).
- **Optimization:** the debug profile is the reference lowering; the release profile runs typed passes on the IR ([release profile](../reference/codegen/#release-profile)).
- **Tooling:** `flick` with `flick.toml` and one pnpm lockfile; `check`, `build`, `run`, `dev`, `test`, `fmt` and the FFI and Schema tools share one checker ([Compiler CLI](../reference/cli/)).
