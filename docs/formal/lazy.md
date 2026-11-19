# Lazy computations: `Lazy r e a`, `let lazy`, and suspended parameters

Status: **contract** (RFC #563, #548). The core primitive of §3, `let lazy` of §4 (T7), R9 of §5 and the O1 rewrites of §6 are proved in Lean and implemented; O2–O4 of §6 are not yet. This page fixes the semantics that the checker, the model, the Lean core, both runtimes and the optimizer must agree on. Each rule lists where it will be checked.

References as in [`core.md`](core.md): `spec/NN`, `reference/X`; checker paths are relative to `flick-core/src/types/`.

## 0. Scope

| Form | Meaning | Section |
| - | - | - |
| `lazy e`, `lazy { b }` | a first-class, shareable, memoized computation of type `Lazy r e a` | §1–§3 |
| `await l` with `l : Lazy r e a` | forces `l`: runs its initializer at most once, then returns the cached outcome | §3 |
| `let lazy x = e` | a binding of its function, forced by `await x` at most once; no cell | §4 |
| `let lazy mut x = e` | the same, whose writes before the first read replace the initializer | §4 |
| `f : ~(Eff r e a) -> …` | a suspended parameter: a direct call as its argument is wrapped in `task` | §7 |

Out of scope for v1: an `Await` class (dispatch is closed: `Eff` or `Lazy`), pure top-level lazies, auto-forcing on access, `Await (Fiber …)`.

## 1. Surface

```text
expr  ::= … | "lazy" atom | "lazy" "{" block "}"
decl  ::= … | "let" "lazy" ["mut"] pattern-var [":" type] "=" block
param ::= … | "~" atom-type                -- only directly left of `->` in a signature
```

- `lazy` is a keyword. `Schema.lazy` becomes `Schema.suspend` (breaking, recorded in the changelog).
- `lazy e` binds like `await e` and `task`: `lazy f x` is `lazy (f x)`.
- Formatting: `lazy { … }` lays out as `task { … }`.

## 2. Types

```text
-- Prelude
@lang lazy
@stores
type Lazy (r : ServiceRow) (e : ErrorRow) a
```

`Lazy r e a` has the rows of `Eff r e a`: `r` is the services the initializer needs, `e` the errors it can fail with. It has no equality, no `Show` and no codec: its identity is not observable.

| Rule | Typing | Stage of the form |
| - | - | - |
| (T-Lazy) | `Γ ⊢ e : a ! ρ; ε` and `Detached ρ` ⟹ `Γ ⊢ lazy e : Lazy ρ ε a` | runtime: `R⟨none; none⟩`, it allocates (as `Ref.make`) |
| (T-Force) | `Γ ⊢ l : Lazy ρ ε a` ⟹ `Γ ⊢ await l : a` | `R⟨Async \| ρ; ε⟩`: forcing may wait for another fiber |
| (T-LetLazy) | `Γ ⊢ e : a ! ρ; ε`, `x` confined (§4) ⟹ `await x : a` | `R⟨ρ; ε⟩`, no `Async`: nothing else can hold `x` |

- `Detached ρ` (`classes/detached.rs`): the initializer uses no `control` operation. This is what makes the reset after an owner's unwind (§5) well-defined.
- **Reach** (`regions.md` §2.1): `⌈Lazy ρ ε a⌉ = ⌈a⌉ ∪ ⌈ε⌉ ∪ captures(e)`. Unlike `⌈Eff ρ ε a⌉`, the error row counts, because a `Lazy` *stores* a typed failure once it is forced. `@stores` gives the captures. As a result, a `Lazy` holding `File s` cannot leave `scope s` (`FLICK-G001`/`G002`), so no lazy is forced after its region closes (RFC question 5).
- `await e` dispatches on the type of `e`: `Eff` → (Await), `Lazy` → (T-Force). An operand whose type is still unknown at the site is taken as `Eff`. This is the same incompleteness as `?` (`elaboration.md` "unknown type at the site"), recorded in `core.md` §5.

Checked by: `tests/check/{pass,fail}/lazy/*`, the checker's unit tests, `flick-model`, Lean `T1` / `T2b` (typing is preserved).

## 3. The core primitive

The core (`core.md`) gains one type former, two term forms and one runtime frame.

```text
τ  ::= … | Lazy ρ ε τ
e  ::= … | lazy e | force e
v  ::= … | ℓ                        ℓ a location whose cell holds a lazy state
μ(ℓ) ::= … | Pending e | Running e | Done o
o  ::= ok v | err v | defect
E  ::= … | force E | settle ℓ E     -- settle is a runtime-only frame
```

Steps (`core.md` §3.4):

```text
⟨μ, E[lazy e]⟩                        →  ⟨μ[ℓ ↦ Pending e], E[ℓ]⟩          ℓ ∉ dom μ
⟨μ, E[force ℓ]⟩, μ(ℓ) = Pending e     →  ⟨μ[ℓ ↦ Running e], E[settle ℓ e]⟩
⟨μ, E[settle ℓ v]⟩                    →  ⟨μ[ℓ ↦ Done (ok v)],  E[v]⟩           if μ(ℓ) = Running e
⟨μ, E[settle ℓ (raise v)]⟩            →  ⟨μ[ℓ ↦ Done (err v)], E[raise v]⟩     if μ(ℓ) = Running e
⟨μ, E[settle ℓ (unw p ℓ' a)]⟩         →  ⟨μ[ℓ ↦ Pending e],    E[unw p ℓ' a]⟩  if μ(ℓ) = Running e
⟨μ, E[settle ℓ die]⟩                  →  ⟨μ[ℓ ↦ Done defect],  E[die]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done (ok v)   →  ⟨μ, E[v]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done (err v)  →  ⟨μ, E[raise v]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done defect   →  ⟨μ, E[die]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Running e     →  ⟨μ, E[die]⟩            -- self-force: one thread forcing its own initializer
⟨μ, E[Ref.get ℓ]⟩, ℓ a lazy cell      →  ⟨μ, E[die]⟩            -- not reachable: no `Ref` term reaches a lazy cell
```

- `settle ℓ □` is transparent to operations: a `perform` inside the initializer is answered by the handlers of `E`, i.e. of the fiber that forces it (no fork, spec/02).
- `raise` and `die` propagate *through* `settle` after recording the outcome. A `try` inside `e` is inside `settle`, so a caught error is not an outcome.
- An outcome, or a reset, is written only into a `Running` cell, which is the state of the cell at every `settle ℓ` a program reaches. With the condition, L1 and L2 hold of every typed configuration, not only of the reachable ones. In one thread `die` ends the thread, so the `Done defect` that `settle ℓ die` writes is not observed; the step is the step of `die` through the frame.
- A force of a cell that holds neither `Pending` nor `Done` is a defect: `Running` above, or a cell that holds no lazy, which no typed term forces. A `Ref.get` of a lazy cell is a defect. The last two keep progress independent of the typing of the store and preservation independent of a premise on (Loc), which also types the location of a lazy cell as a `Ref`.
- A signal of a handler (an `abort`, or a `control` arm that lets the body go) passes `settle ℓ` and resets the cell: `Running e` keeps the initializer, the cell is `Pending e` again, and the next force runs `e` again. A signal is no outcome, as an interruption is none in λRT (§5 (d)).
- Store typing: `Σ(ℓ) = Cell ρ ε τ`, a type of the store only, and `Σ ⊢ ℓ : Lazy ρ ε τ`. No value has the type `Cell ρ ε τ`, so no `Ref` cell has it and `Ref.set` never writes a lazy cell; with `Σ(ℓ) = Lazy ρ ε τ` a `Ref (Lazy ρ ε τ)` cell and a lazy cell would share the type. A cell in `Running e` keeps the typing of its initializer, which a reset gives back to `Pending e`.

**Laws** (core, one thread):

- **L1 (once per attempt).** On any path the starts of `ℓ` (a force of `Pending e`) and its resets (a signal through `settle ℓ`) alternate: between two resets the initializer starts at most once. A reset happens only from `Running e`, on a signal; a value or an error settles the cell.
- **L2 (final).** After `μ(ℓ) = Done o`, `μ(ℓ)` never changes, and every later `force ℓ` reduces to the term of `o` without changing `μ`.

Lean: the core gains `Ty.lazy` and the store type `Ty.lazyCell` (`formal/Formal/TyBase.lean`; `Lazy ρ ε τ` carries the hidden qualifier `C` of its initializer, as `Eff`), `Tm.lazy`, `Tm.force`, `Tm.settle` and the states of a cell `Tm.running`, `Tm.done` (`Syntax.lean`), the frames `Frame.force`, `Frame.settle` (`EvalCtx.lean`), (T-Lazy) `Typ.lazy`, (T-Force) `Typ.force`, (LazyLoc) `Typ.lazyLoc` and (Settle) `Typ.settle` (`Rules.lean`), the store typing `CellTy`, `StoreTy` (`StoreTy.lean`), and the steps `RHead.lazy`, `.forcePending`, `.forceDone`, `.forceRunning`, `.settleVal`, `.settleRaise`, `.settleUnw`, `.refGetCell` with `settleCell` and `resetCell` (`Semantics.lean`). Every lemma over terms, types, frames and steps covers them, `Subst*`, `Reach`, `Regions`, `Live` and `Elab` included; preservation of the new steps is `lazy_new`, `force_pending`, `force_done`, `settle_val`, `settle_raise`, `settle_unw` (`PresLazy.lean`), progress is `progress` (`ProgressMain.lean`), and `T1`, `T2a`, `T2b`, `T3`, `T4'`, `T1_regions` hold with them. L1 is `Lazy.L1` with `Lazy.L1_once` and `Step.pending` (a cell is pending again only after a `Resets` step), and L2 is `Lazy.L2` (`formal/Formal/Lazy.lean`). No `sorry`, no new axioms (`lake build --wfail`; `Formal/Axioms.lean` pins them).

Checked by: `tests/run/runtime/lazy_once.fl` (an outcome is kept) and `tests/run/runtime/lazy_abort.fl` (an `abort` through the initializer resets the cell, and the next `await` runs it again) on JS, TS, Go, the rt runtime and the model, whose natives run the initializer in a frame `settle ℓ` of their own (`flick-model/src/native/lazy.rs`).

## 4. `let lazy` and `let lazy mut`

A `let lazy` binding belongs to its function, as `let mut` does (`mutation.rs`, `Binding`):

- using `x` from a `fun`, a local function, a `task`, a lazy `for` or a handler arm is `FLICK-G005`. Share a lazy with `lazy { … }` instead (the help says so);
- `x` is used only as `await x`; a bare `x` is `FLICK-T003` ("a `let lazy` is forced with `await`").

Elaboration (`elaboration.md` §3.6), with the SSA threading of `let mut` (§3.5, Lean T7). The declaration makes the task of the initializer once. The state `x$ : Option (Result (Errors ε) a)` is `None` until a force, then `Some o`:

```text
let lazy x = e in b       ⇝  let x$run = task (attempt e)              -- immutable, made once
                              let mut x$ = None in b′
await x                   ⇝  match x$ with
                               | Some o → out o
                               | _ → let o = await x$run in x$ = Some o; out o
x = v      (lazy mut)     ⇝  x$ = Some (Ok v)            -- before or after the first force
attempt e                 =  try Ok e with | err → Err err
out o                     =  from_result o               -- the value, or the error raised again
```

- `attempt e` runs `e` and gives its outcome. A signal of a handler or a defect leaves `await x$run` before `x$` is written, so `x$` stays `None` and a later `await x` runs `e` again, as the reset of §3 leaves a cell `Pending`. A defect ends the fiber, and with it the function that holds `x$`, so a defect needs no outcome.
- These are the states of the cell of §3 without `Running`. One fiber holds `x$`, and `e` cannot read `x` (`FLICK-G005`), so nothing can find it running.
- `e` reads the values it had at the declaration, as a `task` does. `x$run` is made at the declaration and holds them, and `e` reads no `let mut` (`FLICK-G005`).
- No cell, no `Async`, no synchronization: the state is a local variable, threaded as the state of `let mut` is.
- `let lazy mut` written before its first read never runs `e` (RFC). A write after a read replaces the cached outcome.

The compiler lowers it so (`flick-core/src/ir/lower/lazy.rs`). `x$run` is a `Stmt::Let` of a `Task`, `x$` a `Stmt::Var`, and `await x` the `match` above.
- `e` is written once, in `x$run`, with the names in scope at the declaration, however many `await`s run it and however deep it is in another initializer. An earlier version copied `e` at each `await`, and code grew as 2^d with the nesting depth d.
- The declaration makes one task. The first force runs it and makes `Some o`; a force after that is only a `match`, so it makes nothing, in a loop or not.
- A task, not a function: `Await` of a task is the IR's own run of an `Eff`.
- The task is immutable, so the model threads only data through the `let mut`. With a callable in the variable, the model rejects the program under `try` and in loops.

The model translates the same IR, as for any `let mut`.

Lean: the forms are blocks of `Formal/LetMut/Core.lean` (`Formal/LetMut/Lazy.lean`):
- `lzDecl run b` is a `let` of the task `run = task (lzAttempt e)` (`LzSig.typ_run`) and a `var` of `None`;
- `lzAwait x f k` is a `br` on the state whose other arm runs `await f`, `set`s the state to `Some o` and gives `lzOut o`;
- `lzWrite x v k` is a `set` of `Some (Ok v)`.

They are derived rules of `MutOk` (`MutOk.lzDecl`, `MutOk.lzAwait`, `MutOk.lzWrite`), given the Prelude's `Option` and `Result` in the signature (`LzSig`, which the witness signature has: `Elab.Witness.lzSig`). So T7, `T7_pure` and `T7_regions` hold of a block written with them. The task is a value made at the declaration and bound by a `let`, which no `set` changes, so the initializer reads the declaration's values.

Checked by:
- `tests/check/fail/lazy/let_lazy.fl` (G005, T003, S016);
- on JS, TS, Go, the rt runtime and the model: `tests/run/runtime/lazy_let.fl`; `lazy_let_state.fl` (a kept error, `await` in a loop, in branches and in a `scope`, a `let lazy` in another's initializer, a write after a read); and `lazy_abort.fl` (an `abort` through the initializer);
- the IR snapshot `ir::tests::a_let_lazy_lowers_to_a_variable_of_its_function`;
- `ir::tests::a_let_lazy_initializer_is_lowered_once`: initializers nested three deep, the inner two each `await`ed twice, are each written once;
- `ir::tests::an_await_of_a_let_lazy_makes_no_task`: the only task is the declaration's, with `await`s in a loop and in a branch.

## 5. λRT: many fibers (law R9)

A `Lazy` cell is shared between fibers. Its states are refined to `Pending e | Running(π, n, W) | Done o`, where `π` is the owner fiber, `n` the attempt and `W` the waiters. Its transitions are atomic, like `Ref.modify` (R7).

**R9 (single flight).** For every lazy cell `ℓ`:

- (a) at most one attempt of `ℓ` is running at any time;
- (b) `Done o` is final, and every force after it returns `o`;
- (c) the interruption of a waiter removes only that waiter: the cell's state and the attempt are unchanged;
- (d) the interruption of the owner, or an unwind of its handler, resets the cell to `Pending e` and wakes the waiters. The first waiter to retry becomes the owner of attempt `n + 1`. The interrupted owner gets its interruption; the waiters do not;
- (e) a force by the owner of a `Running` cell is a defect ("a lazy forced during its own initialization"). A cycle through other fibers deadlocks, as with `Deferred` (documented, not detected).

Typed failures and defects are cached (b). Interruption is not cached (d).

Lean: `formal/Formal/RT/LazyCell.lean` proves R9 as a property of the protocol over atomic transitions, alongside R7: `R9`, over every interleaving `LReach` of the steps `LStep` of a cell `LSt`, with the bookkeeping of attempts `LInv`; its parts are `LInv.runs` (a), `R9b`, `R9c`, `R9d`, `R9_cached` and `R9e`. `Step` and `Inv` gain no constructor, and T5 does not include R9. A running cell keeps its initializer `e`, which the reset of (d) gives back; an outcome is cached only without an interruption (`Cached`). The one-thread core of §3 resets a cell in the same way when a signal passes `settle ℓ`.

Runtimes:

- JS: `runtime/src/rt/effect/Lazy.ts`, waiters as in `Deferred.ts`.
- Effect adapter: `packages/effect-ts/src/effect/Lazy.ts`.
- Go: `runtime-go/flickrt/lazycell.go`. A mutex guards the state, never the run, and waiters use `wait(fb, done)`.

Conformance: `tests/run/runtime/lazy_*.fl` on JS, rt and Go.

## 6. Optimizations, as equivalences

Each is a contextual equivalence of the core of §3, plus a pass of the release optimizer (`flick-backend-support/src/opt/lazy.rs`) with regression tests and the output identity gate.

**Contextual equivalence** (`formal/Formal/Equiv/Context.lean`). A context is a term of the source language with any number of holes, anywhere (`Ctx`). Filled with `e₁` and with `e₂`, it gives two programs; a filling stays under the binders of the context, which bind its free variables. `e₁ ≅ e₂` (`CtxEquiv`) when, for every context whose two programs are typed (T1's hypotheses, under typed roots of the source language: `Prog`), the main threads of the two programs may end with the same literals (`Ends`). The equivalence is of these may-results. It is not one of every run for O1b: the core of §3 has no waiters, unlike λRT (§5, R9), so a force that meets its cell `Running` is a defect, and two threads that force one `lazy v` at once may die where `born v`, done before anyone can reach it, does not.

**Equivalence of every run** (`formal/Formal/Equiv/All.lean`). A run of a configuration ends in one of four outcomes (`Outcome`, `Formal/Equiv/Every.lean`): the main thread ends with a literal, the main thread steps to a defect, the run never ends, or the run gets stuck on anything else (no thread steps, the main thread does not die and is no literal). `e₁ ≅* e₂` (`CtxEquivAll`) when, for every context whose two programs are typed, the two programs have the same outcomes. It implies `CtxEquiv` (`CtxEquivAll.ctxEquiv`). Where it holds, a rewrite changes nothing a run of the program can do, not only which literals it may give.

Where a pattern reads a term without running it (the top of a field of a dictionary record, of the value a handler matches, of a list of alternatives: `Pos.top` of `Formal/Equiv/Sim.lean`), a pattern could tell `force (lazy e)` from `e`, so the two sides of a rewrite are no pair of `Sim` there. Typing keeps them away (`Typ.noLzTop`, `Formal/Equiv/TopTyped.lean`): such a place has the pure stage, or no term judgment, while a term that makes or forces a cell (`Tm.LzShape`) has the stage `rt` at least. The one place typing does not reach is the body of a dead arm of a GADT. An arm whose body has such a term is related whatever the bodies (`Sim.aconsD`), and no run takes it: every configuration either run reaches is typed (`Prog.reach`), so what a step gives has no such term (`Tm.LzTop`), and a step that takes the arm would give one. So a hole may stand anywhere, and the restriction loses nothing.

The proof relates the runs of two programs by a simulation each way (`Sim`, `Formal/Equiv/Sim.lean`): a congruence up to a partial bijection of the locations of their stores, and the pairs of the rewrites and of the states their runs pass through. `Sim` is kept by substitution (`Subst.lean`), by a step of a thread (`thread_sim`, `ThreadSim.lean`), by a step of a configuration, a child in its slot included (`cstep_sim`, `CfgStep.lean`), and by a run (`csteps_sim`). The right side answers with steps of its own: it allocates or settles the cells the left side does not have (`RJ`, `CatchUp.lean`), and it catches up a child the left side finishes, as a region ends only once its children are finished. A left main thread that ends with a literal is answered by a right one that ends with it (`lit_sim`). Typing gives the closed threads the proof needs, on both sides (`Prog.reach`), and `ctxEquiv_of_sim` turns two simulations into `CtxEquiv`.

For every run, the same simulations must answer each outcome, which `EverySim` (`Formal/Equiv/Every.lean`) states once for any relation of configurations; `EverySim.has` derives that every outcome of the left run is one of the right run, and a later rewrite reuses it with a relation of its own:

- **A step** of the left side is answered by steps of the right side, or by none, and then a potential of the left configuration drops (`cstep_every`, `cpot`, `CfgStep.lean`), so a left run that never ends is answered by a right one that never ends. A step answered by none consumes a form only the left side has: it makes a cell, binds it, forces the cell it has just made, or settles one (`Tm.pot`, `PotTm.lean`). The initializer of a cell a thread has just made waits in the store until the thread forces it (`OneShot`), so the potential of a thread counts it there (`xpot`, `Pot.lean`); a thread related by `Sim` forces no such cell (`Sim.xpot`), and the cells reserved for other threads stay pending (`ThR.xpot_le`, `CPot.lean`).
- **A main thread that steps to a defect** is answered by a right one that does, after it caught up (`die_sim`, `dies_sim`, `Die.lean`): `todo` is `todo`, and a `resume` the right side could take is one the left side could. This needs the pairs of O1b absent: `dead` relates a left thread that forced its own running cell with a right one that goes on. A world records whether they may stand (`World.B`); O1a and O1c start without them, and no step adds them.
- **A stuck configuration** is answered by a right run to a stuck one (`stuck_sim`, `StuckCfg.lean`). While the right side steps, the left one does not, so every right step is reflected on the left (`thread_reflect`, `Stuck.lean`; `reflect_focus`, `Refl.lean`): the left thread takes the same step, read through the relation, or one of its children steps alone, or the right side catches up alone, which lowers its potential (`rpot`). The reflection reads the parked bodies of handlers and the absence of lists of alternatives at a focus from the typing of the left thread (`TypInv.lean`).

`everySim` (`All.lean`) instantiates these for the relation of two typed programs' runs without the pairs of O1b, and `ctxEquivAll_of_sim` turns two simulations into `CtxEquivAll`.

| # | Rewrite | Side condition | Why it is sound | Proved |
| - | - | - | - | - |
| O1a | `let l = lazy e in b` ≅ `b` | `l ∉ fv(b)` | allocation is unobservable: `Lazy` has no identity, and `e` never runs | every run |
| O1b | `lazy v` ≅ `born v` = `let l = lazy v in force l; l`, a cell born `Done (ok v)` | `v` a value | `force` of either returns `v` (L2) | literals (may) |
| O1c | `force (lazy e)` ≅ `e` | — | a fresh cell has no other owner or waiter | every run |
| O2a | `let y = await l in C[await l]` ≅ `let y = await l in C[y]` | the second force runs in the same fiber after the first; `C` does not bind `l` | L2: the first force left `Done o`; if `o` failed, `C` is not reached | not yet |
| O2b | `let lazy x = e` with one `await x`, not under a loop or a closure ≅ `let x = e` at that `await` | the force runs at most once | the state variable is read once (§4) | not yet |
| O3 | an unsynchronized cell | the lazy does not escape into a `task`, a closure, a fork or a field | no second fiber can reach `ℓ`, so R9 holds trivially; Perceus frees the initializer's captures on `Pending → Running` | not yet |
| O4 | hoist `lazy e` out of a loop | `e : Lazy none none τ` (pure, no service, no error), loop-invariant `fv(e)` | sharing is unobservable for a pure initializer; it runs at most once either way | not yet |

O1a, O1b and O1c are `Equiv.o1a`, `Equiv.o1b` and `Equiv.o1c` (`formal/Formal/Equiv/Context.lean`), for source terms `e`, `b` and values `v`, all three of may-results. O1a and O1c hold of every run as well: `Equiv.o1a_all` and `Equiv.o1c_all` (`formal/Formal/Equiv/All.lean`). O1b holds of no more than the literals (above). The special pairs of `Sim` are their rewrites both ways: `dropA` and `dropC` (O1a, with `dropB` once the left cell is allocated), `bornA` and `bornB` (O1b, with `bornS`, `bornV` and `dead` for the run of `born v` against the allocated `lazy v`), `forceA` and `forceC` (O1c, with `OneShot` for a thread about to force the cell it just allocated, which only it can). No `sorry`, no new axioms (`Formal/Axioms.lean` pins the five, and `EverySim.has`).

The pass rewrites the IR of a release build, where `lazy e` is `lazy_make (task e)` and a force `lazy_force`. O1a drops `let l = lazy_make t` that nothing reads, and a cell made only to be dropped, for `t` a task or a name. O1b writes `lazy_make (task v)` as `lazy_done v` for `v` a literal, a name, a constructor, a function or a tuple, list or record of them. O1c writes `lazy_force (lazy_make t)` as `await t`, which runs `t` where the cell would have run it; in the core, `await (task e)` steps to `e`. Both at once, `lazy_force (lazy_done v)` is `v`. Checked by: the tests of `flick-core/src/opt/lazy_tests.rs`, and `tests/run/runtime/lazy_opt.fl` on JS, TS, Go, the rt runtime and the model.

## 7. Suspended parameters `~T` (#548)

A parameter marked `~` takes its argument as a computation. At a call whose argument is not already an `Eff`, the argument is wrapped in `task`:

```text
or_die : ~(Eff r e a)^c ->{r} a
let rows = Effect.or_die (Database.query q)     -- elaborates to Effect.or_die (task { Database.query q })
let job = task { Database.query q }
Effect.or_die job                               -- an `Eff` passes as before
```

- **Where.** `~T` is allowed only as the parameter of an arrow in a signature, and `T` must be `Eff r e a`, optionally qualified (`^c`). Elsewhere it is an error (new code). The marker is not part of the type: unification, subtyping and `.iface` type equality ignore it. `.iface` records it beside the signature.
- **Elaboration** (`elaboration.md` §3.7): `f e₁ … eᵢ …`, where `f` is a value called by its name, its signature marks parameter `i`, and `eᵢ` runs a runtime call where it is written (an application through a runtime arrow, an operation of a service, or `await`), becomes `f e₁ … (task eᵢ) …`. The argument is checked as the body of a `task`: a deferred stage (`Apart::Task`), its captures in `^c`, its rows in the `Eff`. An `Eff` value, or a pure call that builds one, is passed as it is. An unknown callee type means no suspension. A pipe `x |> f` evaluates `x` before `f` is typed, as ever, so it does not suspend `x` (`types/suspended.rs`).
- **Not changed.** The core, because IR already has `Task`, and the model's typing rules. `FLICK-E003` ("argument runs before the function that takes an `Eff`") is not reported for a marked parameter; for unmarked ones it stays, with its fix.
- **std.** `Effect.or_die`, `retry*`, `timeout`, `timeout_option`, `Effect.timeout_or_else`, `exit`, `ignore_` are marked; the rest of the list is reviewed with the implementation.

## 8. Open points left for review

1. The defect message and code for self-force (§3, R9e).
2. Whether `Lazy` gets an `Inspectable`-style debug `Show` (shows only the state, not the value).
3. Whether `~` is also allowed on parameters of class methods.
4. `Ty.elide` (the reading of a signature in the live discipline) leaves `Lazy` as it is: a `Lazy` in a signature gets no `Live` of the parameters before it, as `Eff` does.
5. O1b holds of the literals only (§6): the core of §3 has no waiters, so two threads that force one `lazy v` at once may die where `born v` does not. Whether to give the core the waiters of λRT (§5, R9), under which this run would not die, so O1b could hold of every run as well.
