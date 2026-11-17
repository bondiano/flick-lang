# Lazy computations: `Lazy r e a`, `let lazy`, and suspended parameters

Status: **contract** (RFC #563, #548). The core primitive of §3 and R9 of §5 are proved in Lean; the rest is not yet implemented. This page fixes the semantics that the checker, the model, the Lean core, both runtimes and the optimizer must agree on. Each rule lists where it will be checked.

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
μ(ℓ) ::= … | Pending e | Running | Done o
o  ::= ok v | err v | defect
E  ::= … | force E | settle ℓ E     -- settle is a runtime-only frame
```

Steps (`core.md` §3.4):

```text
⟨μ, E[lazy e]⟩                        →  ⟨μ[ℓ ↦ Pending e], E[ℓ]⟩          ℓ ∉ dom μ
⟨μ, E[force ℓ]⟩, μ(ℓ) = Pending e     →  ⟨μ[ℓ ↦ Running],   E[settle ℓ e]⟩
⟨μ, E[settle ℓ v]⟩                    →  ⟨μ[ℓ ↦ Done (ok v)],  E[v]⟩           if μ(ℓ) = Running
⟨μ, E[settle ℓ (raise v)]⟩            →  ⟨μ[ℓ ↦ Done (err v)], E[raise v]⟩     if μ(ℓ) = Running
⟨μ, E[settle ℓ die]⟩                  →  ⟨μ[ℓ ↦ Done defect],  E[die]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done (ok v)   →  ⟨μ, E[v]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done (err v)  →  ⟨μ, E[raise v]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done defect   →  ⟨μ, E[die]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Running       →  ⟨μ, E[die]⟩            -- self-force: one thread forcing its own initializer
⟨μ, E[Ref.get ℓ]⟩, ℓ a lazy cell      →  ⟨μ, E[die]⟩            -- not reachable: no `Ref` term reaches a lazy cell
```

- `settle ℓ □` is transparent to operations: a `perform` inside the initializer is answered by the handlers of `E`, i.e. of the fiber that forces it (no fork, spec/02).
- `raise` and `die` propagate *through* `settle` after recording the outcome. A `try` inside `e` is inside `settle`, so a caught error is not an outcome.
- An outcome is written only into a `Running` cell, which is the state of the cell at every `settle ℓ` a program reaches. With the condition, L1 and L2 hold of every typed configuration, not only of the reachable ones. In one thread `die` ends the thread, so the `Done defect` that `settle ℓ die` writes is not observed; the step is the step of `die` through the frame.
- A force of a cell that holds neither `Pending` nor `Done` is a defect: `Running` above, or a cell that holds no lazy, which no typed term forces. A `Ref.get` of a lazy cell is a defect. The last two keep progress independent of the typing of the store and preservation independent of a premise on (Loc), which also types the location of a lazy cell as a `Ref`.
- A signal of a handler (an `abort`, or a `control` arm that lets the body go) passes `settle ℓ` and leaves the cell `Running` (§8).
- Store typing: `Σ(ℓ) = Cell ρ ε τ`, a type of the store only, and `Σ ⊢ ℓ : Lazy ρ ε τ`. No value has the type `Cell ρ ε τ`, so no `Ref` cell has it and `Ref.set` never writes a lazy cell; with `Σ(ℓ) = Lazy ρ ε τ` a `Ref (Lazy ρ ε τ)` cell and a lazy cell would share the type. A cell in `Running` has the type of its frame.

**Laws** (core, one thread):

- **L1 (once).** On any path, `settle ℓ` occurs at most once for each `ℓ`.
- **L2 (final).** After `μ(ℓ) = Done o`, `μ(ℓ)` never changes, and every later `force ℓ` reduces to the term of `o` without changing `μ`.

Lean: the core gains `Ty.lazy` and the store type `Ty.lazyCell` (`formal/Formal/TyBase.lean`; `Lazy ρ ε τ` carries the hidden qualifier `C` of its initializer, as `Eff`), `Tm.lazy`, `Tm.force`, `Tm.settle` and the states of a cell `Tm.running`, `Tm.done` (`Syntax.lean`), the frames `Frame.force`, `Frame.settle` (`EvalCtx.lean`), (T-Lazy) `Typ.lazy`, (T-Force) `Typ.force`, (LazyLoc) `Typ.lazyLoc` and (Settle) `Typ.settle` (`Rules.lean`), the store typing `CellTy`, `StoreTy` (`StoreTy.lean`), and the steps `RHead.lazy`, `.forcePending`, `.forceDone`, `.forceRunning`, `.settleVal`, `.settleRaise`, `.settleUnw`, `.refGetCell` with `settleCell` (`Semantics.lean`). Every lemma over terms, types, frames and steps covers them, `Subst*`, `Reach`, `Regions`, `Live` and `Elab` included; preservation of the new steps is `lazy_new`, `force_pending`, `force_done`, `settle_val`, `settle_raise`, `settle_unw` (`PresLazy.lean`), progress is `progress` (`ProgressMain.lean`), and `T1`, `T2a`, `T2b`, `T3`, `T4'`, `T1_regions` hold with them. L1 is `Lazy.L1` and L2 is `Lazy.L2` (`formal/Formal/Lazy.lean`). No `sorry`, no new axioms (`lake build --wfail`; `Formal/Axioms.lean` pins them).

## 4. `let lazy` and `let lazy mut`

A `let lazy` binding belongs to its function, as `let mut` does (`mutation.rs`, `Binding`):

- using `x` from a `fun`, a local function, a `task`, a lazy `for` or a handler arm is `FLICK-G005`. Share a lazy with `lazy { … }` instead (the help says so);
- `x` is used only as `await x`; a bare `x` is `FLICK-T003` ("a `let lazy` is forced with `await`").

Elaboration (`elaboration.md` new §3.6), with the SSA threading of `let mut` (§3.5, Lean T7):

```text
let lazy x = e in b       ⇝  let mut x$ = Unforced in b′
await x                   ⇝  match x$ with
                               | Forced o → out o
                               | Unforced → let o = attempt e in x$ = Forced o; out o
x = v      (lazy mut)     ⇝  x$ = Forced (ok v)          -- before or after the first force
```

- `attempt e` runs `e` and gives its outcome. `out o` gives back the value, re-raises the error or the defect. Lexical values of `e` are those at the declaration, as for `task`.
- No cell, no `Async`, no synchronization: the state is a local variable, threaded as the state of `let mut` is.
- `let lazy mut` written before its first read never runs `e` (RFC). A write after a read replaces the cached outcome.

The compiler does not thread `x$` yet. It lowers `let lazy x = e` to a cell that only its function reaches: `lazy_make` at the declaration, `await x` to `lazy_read` (the force of `Lazy.force` with the row `{r}; e`, without `Async`), and `x = v` to a new cell `lazy_done v`. G005 keeps the cell from a second fiber, which is the condition of O3 (§6), so it has the outcomes of the threaded form: `e` runs at the first `await x`, once, and a write before it means `e` never runs. The model runs the same lowering.

Checked by: `tests/check/fail/lazy/let_lazy.fl` (G005, T003, S016) and `tests/run/runtime/lazy_let.fl` on JS, TS, Go, the rt runtime and the model. The elaboration to `x$` and Lean T7 for it are open (§8).

## 5. λRT: many fibers (law R9)

A `Lazy` cell is shared between fibers. Its states are refined to `Pending e | Running(π, n, W) | Done o`, where `π` is the owner fiber, `n` the attempt and `W` the waiters. Its transitions are atomic, like `Ref.modify` (R7).

**R9 (single flight).** For every lazy cell `ℓ`:

- (a) at most one attempt of `ℓ` is running at any time;
- (b) `Done o` is final, and every force after it returns `o`;
- (c) the interruption of a waiter removes only that waiter: the cell's state and the attempt are unchanged;
- (d) the interruption of the owner, or an unwind of its handler, resets the cell to `Pending e` and wakes the waiters. The first waiter to retry becomes the owner of attempt `n + 1`. The interrupted owner gets its interruption; the waiters do not;
- (e) a force by the owner of a `Running` cell is a defect ("a lazy forced during its own initialization"). A cycle through other fibers deadlocks, as with `Deferred` (documented, not detected).

Typed failures and defects are cached (b). Interruption is not cached (d).

Lean: `formal/Formal/RT/LazyCell.lean` proves R9 as a property of the protocol over atomic transitions, alongside R7: `R9`, over every interleaving `LReach` of the steps `LStep` of a cell `LSt`, with the bookkeeping of attempts `LInv`; its parts are `LInv.runs` (a), `R9b`, `R9c`, `R9d`, `R9_cached` and `R9e`. `Step` and `Inv` gain no constructor, and T5 does not include R9. A running cell keeps its initializer `e`, which the reset of (d) gives back; an outcome is cached only without an interruption (`Cached`).

Runtimes:

- JS: `runtime/src/rt/effect/Lazy.ts`, waiters as in `Deferred.ts`.
- Effect adapter: `packages/effect-ts/src/effect/Lazy.ts`.
- Go: `runtime-go/flickrt/lazycell.go`. A mutex guards the state, never the run, and waiters use `wait(fb, done)`.

Conformance: `tests/run/runtime/lazy_*.fl` on JS, rt and Go.

## 6. Optimizations, as equivalences

Each is a contextual equivalence (`formal/Formal/ContextEquivalence.lean`) proved for the core of §3, plus an IR pass in `flick-core/src/opt/lazy.rs` with regression tests and the output identity gate.

| # | Rewrite | Side condition | Why it is sound |
| - | - | - | - |
| O1a | `let l = lazy e in b` ≅ `b` | `l ∉ fv(b)` | allocation is unobservable: `Lazy` has no identity, and `e` never runs |
| O1b | `lazy v` ≅ `done v` (a cell born `Done (ok v)`) | `v` a value | `force` of either returns `v` (L2) |
| O1c | `await (lazy e)` ≅ `e` | — | a fresh cell has no other owner or waiter |
| O2a | `let y = await l in C[await l]` ≅ `let y = await l in C[y]` | the second force runs in the same fiber after the first; `C` does not bind `l` | L2: the first force left `Done o`; if `o` failed, `C` is not reached |
| O2b | `let lazy x = e` with one `await x`, not under a loop or a closure ≅ `let x = e` at that `await` | the force runs at most once | the state variable is read once (§4) |
| O3 | an unsynchronized cell | the lazy does not escape into a `task`, a closure, a fork or a field | no second fiber can reach `ℓ`, so R9 holds trivially; Perceus frees the initializer's captures on `Pending → Running` |
| O4 | hoist `lazy e` out of a loop | `e : Lazy none none τ` (pure, no service, no error), loop-invariant `fv(e)` | sharing is unobservable for a pure initializer; it runs at most once either way |

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
4. In the core of §3 a signal of a handler passes `settle ℓ` and leaves the cell `Running`, so a later force in the same thread is a defect; (d) of §5 resets the cell to `Pending e` instead. The core would need `Running` to keep `e` to match (d).
5. `Ty.elide` (the reading of a signature in the live discipline) leaves `Lazy` as it is: a `Lazy` in a signature gets no `Live` of the parameters before it, as `Eff` does.
6. `let lazy` is lowered to a confined cell (§4), not to the threaded state `x$`; T7 does not cover it yet.
