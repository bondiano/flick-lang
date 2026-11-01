# λFlick in Lean 4

A mechanization of the core calculus in [`docs/formal/core.md`](../docs/formal/core.md)
and, separately, of the runtime contract λRT in [`docs/formal/runtime.md`](../docs/formal/runtime.md)
(`Formal/RT/`, see [λRT and T5](#λrt-and-t5)). Lean 4.29.1, no Mathlib.

```sh
cd formal
lake build --wfail      # or: nix shell nixpkgs#lean4 -c lake build --wfail
```

`--wfail` makes a `sorry` (reported as a warning) fail the build; CI runs the same
command in the `lean` job. `#print axioms` of `T1`, `T2a`, `T2b`, `lemma_3_1` and `T3` lists
only Lean's standard `propext`, `Classical.choice` and `Quot.sound`.

## Status

| Stage | Scope | State |
| - | - | - |
| P1a | λ, `let` without generalization, `let rec`, `match` with exhaustiveness and guards, records, `Ref` and the store | done |
| P1b | stages `P`/`R⟨r;ε⟩` and subtyping, closed rows, `raise`/`try`/`handle`, `task`/`await`, `ask`/`with`/`provide` and layers; Л1–Л3, Progress, Preservation, T1, T2(a), T2(b) | done |
| P1c | type variables and schemes, (Let-Gen) with the premise `P`, Л2 for schemes, lemma 3.1, T3 | done |

## Map to `core.md`

| `core.md` | Lean |
| - | - |
| §1.2 types, stages, rows; `σ` | `Ty`, `Stage`, `Row`, `Scheme` (`Syntax.lean`) |
| type substitution, `Gen` and instantiation | `Ty.substT`, `Ty.up`, `liftT`, `instT` (`TyOps.lean`) |
| §1.3 terms, patterns; §3.1 run-time forms | `Tm`, `Pat` (`Syntax.lean`) |
| §1.3, §3.1 values | `Value`, `IsEnv` (`Typing.lean`) |
| §1.5 constants `Ref.*`, `ask_S`, `Layer.*` | `Const`, `Const.ty` (`Syntax.lean`) |
| §2.1 `⊑`, §2.2 `≤` | `Stage.le`, `Le` (`Syntax.lean`, `Typing.lean`, `Order.lean`) |
| §2.3 rules, §3.6 run-time rules | `Typ` (`Rules.lean`) |
| (Sub) | admissible: `Typ.sub` (`SubAdm.lean`) |
| §2.4 patterns, `Exh`; (Match-Val), Р29 | `PatTy`, `Shp`, `Exh`, `Covers` (`Typing.lean`) |
| (Try), (Handle) | `HPat`, `Catches`, `HCov`; `tryR`, `tryP`, `handle` |
| §3.6 `⊢ μ : Ψ`; §2.6 `Σ₀` | `StoreTy`, `RootTy` (`Rules.lean`) |
| §3.2 contexts, `lookup(E, S)` | `Frame`, `plug`, `lookupE` (`Semantics.lean`) |
| §3.3 `→ₚ`; §3.4–3.5 steps | `PHead`, `RHead`, `Step`, `Steps` (`Semantics.lean`) |
| lemma about `build` | `canonical_layer` (`CanonLayer.lean`) |
| Л1 | `canonical_arrow`, `canonical_ref`, `canonical_eff`, `canonical_env`, `canonical_errors`, `Typ.valueAny` (`CanonForms.lean`, `CanonEnv.lean`, `Canonical.lean`) |
| (Let-Gen), (Let-Mono) | `Typ.letGen`, `Typ.let_` (`Rules.lean`) |
| type substitution lemma | `Typ.substT` (`TySubst.lean`) |
| Л2 | `Typ.subst`, `Typ.inst`, `SubOk.one` (`Substitution.lean`) |
| Л3 | `ctx_typing` (`Lemma3.lean`) |
| Progress | `progress` (`ProgressMain.lean`) |
| Preservation | `preservation` (`Preservation.lean`) |
| T1 | `T1` (`Soundness.lean`) |
| T2(a), T2(b) | `T2a`, `T2b`, `T2b_await` (`Effects.lean`) |
| lemma 3.1, T3 | `lemma_3_1`, `T3` (`Generalization.lean`) |
| a polymorphic use, as a check | `Examples.lean` |

## Representation

Terms use **de Bruijn indices** with parallel substitution (`Tm.rename`, `Tm.subst`,
`inst`). Binders that bind several variables at once are common here: a pattern binds
`p.binds` variables, and `let rec`/`fix` bind two. With de Bruijn indices a branch body
is simply under `p.binds` binders, `e[δ]` is `inst δ`, and the substitution lemma is
the usual "a well-typed substitution maps a well-typed term to a well-typed term".
Locally nameless would need opening by a list of fresh names and cofinite
quantification in every binding rule, which costs more without a library.
Typing contexts are lists of schemes, newest variable at index 0; a pattern's `Δ` is
prepended in binding order as monotypes.

Type variables are de Bruijn indices too (`tvar n`); types have no binders, a scheme
`⟨k, τ⟩` binds the indices `0 … k-1` of `τ`. Terms carry no types (Curry style), so a type
substitution changes the judgment and not the term. (Let-Gen) types its right-hand side
with `Γ` and `Ψ` shifted by `k` (`Ty.up k`): the generalized variables are fresh by
construction, which is `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)` of `Gen` and Л2. A variable instantiates its
scheme in (Var); a substituted value is typed at its scheme's own level and instantiated by
the type substitution lemma.

Lists inside terms are spines, so `Tm` is one plain inductive type and `induction`
works on it and on the typing derivation:

- a constructor application `K v₁ … vₙ` is `app (… (app (ctor K) v₁) …) vₙ`, as in `core.md`;
  constructor patterns are spines too (`Pat.app`);
- a record is `rcons ℓ₁ e₁ (… rnil)`, a record type `Ty.rcons ℓ₁ τ₁ (… Ty.rnil)`;
- an environment `env θ` is `econs S₁ v₁ (… enil)`;
- match and `try` arms are `acons p b rest`, `aguard p g b rest`, ending in `anil`, with
  their own judgment forms `J.alts` and `J.halts` in the same inductive `Typ`.

Rows are closed sets of names, `Name → Prop`; equality of rows is extensional, which is
`≡` of §1.2. Every rule types its premises at the stage of its conclusion, and the
subsumption `≤` is folded into the rules whose result may be an arrow or an `Eff`; (Sub)
is then admissible (`Typ.sub`). With (Sub) this is equivalent to the `⊔` form of §2.3,
and inversion stays a plain `cases`.

## Assumptions and simplifications

Each item narrows `core.md` unless it says otherwise.

1. **Polymorphism is over types only.** Rows are closed sets of names, so row variables
   are neither generalized nor instantiated: `raise`, `Layer.effect_S` and `Layer.merge`
   carry their rows (`Const.effect S r ε`). `Ref.*` are polymorphic in their cell type,
   instantiated by (Const). ADTs, errors and services have no type parameters
   (`Sig.ctor_closed`, `Sig.impl_closed`). (Let-Sig) is (Let-Gen) or (Let-Mono) with the
   annotation erased: terms carry no types, and the premise `ᾱ ≠ ∅ ⇒ φ₁ = P` is the
   premise `P` of (Let-Gen). `let rec` is monomorphic in its body and its scope
   (`σ° = τ_f` and `σ = τ_f`). Higher-rank arguments are out of scope, as in `core.md`.
2. **Declarations** are a parameter `S : Sig`: constructor ↦ argument types and type
   name, service ↦ `S.Impl`. No well-formedness of `S` is needed. An error type `E` is an
   ADT whose constructors are the error's constructors.
3. **`Unit`, `Bool`, `Int`** are base types with literals (`Lit`), not nominal ADTs;
   `True`/`False` patterns are literal patterns.
4. **Records are closed with a fixed field order.** No row tail and no equality up to
   permutation: the elaborator is assumed to order labels; (Rec) requires the tail to be a
   record type (`Ty.IsRec`). Record patterns are always open (`{…, ..}`), as in `core.md`.
5. **`Exh` over shapes.** `Exh(τ, Π)` quantifies over `Shp τ v`, the closed values of `τ`
   as far as patterns see them (type variables, functions, references, computations,
   layers, errors and environments are opaque), not over typed values: the typed version would put `Typ`
   in a negative position of its own definition. Patterns only inspect literals,
   constructors and record fields, so the two agree. The exhaustiveness algorithm
   (`exhaust.rs`) is not modeled.
6. **`raise` is a term former**, not a first-class constant; `λx. raise x` recovers the
   constant. It takes `Errors ε` (with stage `R⟨none; ε⟩`) or an error type `E` (with
   `R⟨none; {E}⟩`), which is the `Errors {E} ≡ E` of §1.5 at the only place the core
   needs it. (Errors-Val) types values only, as in §3.6; the other direction of
   `Errors {E} ≡ E` and `Errors none ≡ Never` are not modeled.
7. **`try` arms** are `x`/`_` at `Errors ε_b` or a constructor pattern `E p̄` at the error
   type `E`, as in (Try). `H` is a judgment index: each arm without `when` may add what it
   catches (`Catches`). (Handle) uses the value form of the coverage premise (Р102).
8. **Environments.** `Env ρ` means "binds at least `ρ`" (an environment may be typed with
   a smaller row). `with θ e` and `econs` require `θ` and the bound values to be values,
   as `env θ` is a value form in §3.1. The disjointness of `Layer.merge` is kept
   (`Const.wf`) though soundness does not need it.
9. **Allocation is deterministic:** `Ref.make v` takes `ℓ = μ.length`.
10. **Programs** are closed terms typed in the empty store; modules (§2.6) are not
    modeled. `Σ₀` is a parameter with `RootTy`; Г3 is the hypothesis of `T1` that `Σ₀`
    covers the services of the program's stage. Progress reports an `ask_S` that
    neither a `with` frame nor `Σ₀` answers (`AskStuck`) instead of assuming Г3.
11. **Л3** is stated as `ERel`: the hole's services are within those of the context and
    of its `with` frames, its errors grow only under a `try` frame, and a pure context
    has a pure hole. `core.md` names the errors each `try` frame removes; the weaker form
    suffices for T2. Under `K` right-hand sides of (Let-Gen) the hole is typed `K` type
    variables deep, with `Ψ` shifted, and is pure.
12. **Steps carry labels** (`Lbl`): pure, runtime, or `root S` for an `ask_S` answered by
    `Σ₀`; T2(b) is about the `root` labels. `todo → die` is labelled pure, as T2(a) allows
    `die`; `bind_S` and `join` steps are pure (Р101).
13. **Cell types may be open** (Р103). `Ψ(ℓ)` is a monotype that may mention free type
    variables of the configuration's derivation; (Let-Gen) shifts `Ψ`, so it never
    generalizes them. `core.md` asked for closed `Ψ(ℓ)` without ever closing them.
14. **T3** is stated as: reachable configurations are typed at the program's type, the
    store typing only grows (a cell keeps its allocation type), and a redex under the
    right-hand side of a (Let-Gen) with `ᾱ ≠ ∅` is pure while the cell types it sees
    avoid `ᾱ` (`Ty.Avoids`). "Without the premise `P` the first case breaks" is not
    formalized as a counterexample.

## Findings

- **Р100** (P1a): a partially applied constructor is a value, and Л1 for an arrow type
  has to list it (`Value.ctorApp`, `canonical_arrow`).
- **Р101** (P1b): `bind_S` and `join` steps were runtime steps while their rules are
  pure, so T2(a) failed for a bare `bind_S v`; they are pure steps now.
- **Р102** (P1b): (Handle) asked for the whole row `ε_b ⊆ H + ε'`, which a failed arm
  breaks; it now asks for the error of the value only, as `flick-model` already did.
- **Р103** (P1c): closed cell types and a fixed program type cannot both hold; cell types
  are monotypes with possibly free variables, and `Gen` excludes `ftv(Ψ)`.

## λRT and T5

`Formal/RT/` is a model of its own: it imports nothing from the core above, and the core
is a parameter of it.

| Stage | Scope | State |
| - | - | - |
| P3a | causes: `⊕`, `⧺`, `head`, `recover`, I_ε; R8, R5 | done |
| P3b | configurations, steps §3.2–§3.6, the closing C0–C6 by phases; И1–И3; R1–R4 | done |

| `runtime.md` | Lean |
| - | - |
| §1.1 reasons, causes | `Reason`, `Cause` (`RT/Cause.lean`) |
| §2.1 `nub`, `⊕`, `⧺` | `nub`, `comb`, `++` (`RT/Cause.lean`) |
| §2.2 normal form, laws, `head`; Go's tree | `NF`, `head`, `head_comb`, `CTree` (`RT/Cause.lean`) |
| R8 | `R8` (`RT/Cause.lean`) |
| §2.3 `dies`, `recover` | `dies`, `recover` (`RT/Recover.lean`) |
| §2.4 I_ε and what keeps it | `Iε`, `Iε_comb`, `Iε_append`, `Iε_close`, `Iε_keep` (`RT/Recover.lean`) |
| R5 | `R5` (`RT/Recover.lean`) |
| §1.2–§1.4 fibers, owners, configurations, run-time forms | `Fiber`, `Code`, `Frame`, `Owner`, `Phase`, `Conf` (`RT/Config.lean`) |
| the core, a parameter | `Core`, `Act` (`RT/Config.lean`) |
| what the libraries decide | `Lib` (`RT/Config.lean`) |
| §3.1–§3.6 steps | `Local` (`RT/Local.lean`), `Step` (`RT/Step.lean`), `Steps`, `Reach` (`RT/Reach.lean`) |
| §3.3 C0–C6 | `Step.close`, `.hookCode`, `.hookStop`, `.stopDone`, `.kids`, `.kidsDone`, `.fins`, `.finCode`, `.finObs`, `.finObsLast`, `.closed`, `.absorbH`, `.absorbF` |
| §5 Г7(а)–(ж) but (в), Г8, Г9 | `Hyp` (`RT/Hyp.lean`) |
| invariants of reachable configurations | `Inv`, `Inv.step`, `Inv.reach` (`RT/Inv.lean`, `RT/Reach.lean`) |
| И1 | `Inv.stJ` and `Good` of `close`, `fins`, `closed` (`RT/Inv.lean`) |
| И2 | `Inv.empty` and `Good` of `spawn`, `fin`, `gkid`, `branch` |
| И3 | `Inv.spawnJ` |
| И5 | `Iε_comb`, `Iε_append`, `Iε_close`, `Iε_keep` (`RT/Recover.lean`) |
| R1, R2, R3, R4 | `R1`–`R4`, `Inv.r1`–`Inv.r4`, `Inv.r4_closed`, `parResult_keeps` (`RT/Props.lean`) |
| the sketch of R3: who waits for a running fiber | `Guard`, `Inv.noRun` (`RT/Inv.lean`, `RT/NoRun.lean`) |

Simplifications of the cause algebra:

1. `≈` is equality of reasons (`DecidableEq V`). A thrown `Error`, compared by identity in
   Effect, is a value of `V` that carries its identity.
2. A row `ε` and `· ⊢ v : Errors ε` are a predicate `Has : V → Prop`; the premise of (Try)
   is `Covers`. R5 is about one `recover`, as the sketch in §6 is; that the core's (Try)
   gives `Covers` is T1 of P1.
3. `pretty` and `report` (§2.5) are not defined: they read `Cause V`, a list, so the third
   part of R8 holds by their type. (Seq=Par) is `CTree.reasons`, where both node kinds are `⊕`.

Simplifications of λRT:

1. **The core is a parameter.** A fiber's code is a term of any type with a labelled
   transition relation `Core.step`; a label says what the code asks of the runtime, and
   `Core.plug` hands the result back. Every theorem holds for every core.
2. **Handles can be forged.** An action names owners, fibers and groups by number, and the
   rules check only their states (`spawn` needs `open`, and so on). So И4, which needs Г6,
   is not modelled; R1–R4 do not use it. R3 is proved from `Guard` instead: a child is in
   `K`, a watcher in `W`, a group's child in an open group or one its owner's closing waits
   for, a branch in a `par` frame of a running fiber whose current owner, below that frame,
   started it (`Inv.noRun` is the induction of the sketch).
3. **No mask.** An interruption is delivered at any point of user code and of the waits that
   are interruption points (`Code.IntPoint`); a closing, a watcher's wait and a combinator's
   wait are not. That is more interleavings than §3.2, so safety carries over; Г7(в) is about
   when an interruption arrives and is not needed. Requests to stop (Int-Req, C2, a group's
   hook, `parallel` stopping its branches) arrive at any time (`Step.req`): that they are
   sent is liveness.
4. **The closing** is run by the fiber that ran the body, through a `closing π` frame; its
   phase is in the owner (`Phase`). `hooks true` is both the hooks added since C1 (Р38) and
   C4: it runs hooks until `H = ∅` and then waits for `P = ∅` (Р160).
5. **Groups**: the hook `stop γ` closes the group; `FiberSet.make` in an owner past (C5)
   gives a closed group; `join` of a group is not modelled.
6. **Combinators**: only `parallel`. `race_*` and `timeout` wait the same way and differ in
   the result, which R3 does not see; the result of `parallel` takes the first failure in
   input order rather than in the order of ending.
7. The flag `sd` of `Failure c sd` is not modelled; the root owner never closes, and the
   runner reading `D` is outside the model (R4 shows `c_g ∈ D`).
8. One counter gives fresh fibers, owners, finalizers, Promises, groups and maps; finalizer
   ids follow registration, so R2's "reverse order of addition" is "decreasing ids".
9. A Promise settles at any time with any outcome. Go puts a call into `P` only after it is
   interrupted (§1.3); the model does it at the call, as JS does.
10. A memo map is forked by every `provide`, and a layer is built in the map of the
    `provide` that asked for it, reading the maps of the enclosing ones (§3.5).
