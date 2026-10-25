---
title: "2. Decisions of the first version"
---

| Area | Accepted | Cost of the choice |
| - | - | - |
| Sequencing | Direct calls with let; task/await; `do` over `Monad` with pure continuations; `for` generators | Two styles: lints and fixes move `do` over Eff to direct calls |
| Computation type | Eff r e a, result last | Source order differs from native TS order |
| Classes | MPTC, superclasses, FD; canonical instances | Strict coverage/termination; some instance chains do not carry over |
| Coherence | No overlap/orphans/local implicits; package ownership | Alternatives need wrappers or explicit dictionaries |
| Generalization | Only provably pure constructions; runtime results are monomorphic | A factory is sometimes required; variance-generalization is deferred |
| Higher-order API | map and sequential map_effect; explicit parallel limit | Two families of operations instead of a hidden runtime bridge |
| Collections | List = dense immutable JS array; a separate Effect Chunk | Prepend/tail/update are usually O(n) |
| Records | Exact structural + row polymorphism | No implicit width subtyping and no silent overwrite |
| Type-level | Fixed row primitives, labels, finite kinds, MPTC/FD | General type families and associated types are deferred |
| Lifetime | Fresh regions, captures, no early close | Closing waits for children and foreign quiescence |
| Errors | Native recovery; opaque Failure/Exit/RawCause; region-free typed payloads | Raw failures cannot be treated as the residual e |
| Concurrency | Structured combinators, scoped supervisor, native race/timeout | No nursery, no detached scoped work and no hard deadline |
| FFI | Fixed ABI, four callback families, checked `.d.ts` subset | Purity/retention/cancellation require contracts |
| Schema | Full deriving, explicit codecs, strict exporters | Not every schema is expressible in an external format |
| SQL | Pure DSL, PostgreSQL, Query/Template | A restricted SQL subset is fixed |
| UI | ui block, React adapter, state in `effect/reactivity` atoms, actions with explicit event policy, server rendering with a registry per request | No JSX and no custom renderer; no React hooks, state lives only in atoms; Suspense is opt-in |
| CLI applications | cli block on top of Effect CLI, exact record of options, `Cli.Value` for parsing | A subcommand does not see the parent's options; no custom argument parser |
| Compiler | Rust core of Flick, typed IR, source LSP, versioned JSON iface | Own checker, Mapper is an integration layer |
| Optimization | Debug baseline and typed release passes from ReScript approaches | Proofs that observable contracts are preserved are needed |
| CLI | `flick`, `.fl`, `flick.toml`, one pnpm lockfile; check/build/run/dev/test/fmt and FFI/Schema tools | The commands share one checker; runtime and interface versions are checked explicitly |

All the listed decisions belong to the first version. Pure `a -> b`, runtime `a ->{r; e} b` and runtime without requirements/errors `a ->{} b` are distinguished in the checker and the IR. Static guarantees hold for checked source and truthful trusted bindings; native Effect and the Content Mapper do not prove them automatically.
