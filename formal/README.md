# λFlick in Lean 4

A mechanization of the core calculus in [`docs/formal/core.md`](../docs/formal/core.md),
extended in place with the regions and captures of
[`docs/formal/regions.md`](../docs/formal/regions.md), and, separately, of the runtime
contract λRT in [`docs/formal/runtime.md`](../docs/formal/runtime.md)
(`Formal/RT/`, see [λRT and T5](#λrt-and-t5)). Lean 4.29.1, no Mathlib.

```sh
cd formal
lake build --wfail      # or: nix shell nixpkgs#lean4 -c lake build --wfail
```

`--wfail` makes a `sorry` (reported as a warning) fail the build; CI runs the same
command in the `lean` job. `#print axioms` of `T1`, `T2a`, `T2b`, `lemma_3_1`, `T3`, `lemma_4`, `lemma_6`, `T4`,
`T4_3`, `T1_regions`, `T5`, `T6a` and `T6b` lists only Lean's standard `propext`,
`Classical.choice` and `Quot.sound`; the hypotheses of T5 (`Hyp`, `G11`), of T6(a)
(`Target`) and of T6(b) (`Coherent`, `Sim`) are their parameters.

## Status

| Stage | Scope | State |
| - | - | - |
| P1a | λ, `let` without generalization, `let rec`, `match` with exhaustiveness and guards, records, `Ref` and the store | done |
| P1b | stages `P`/`R⟨r;ε⟩` and subtyping, closed rows, `raise`/`try`/`handle`, `task`/`await`, `ask`/`with`/`provide` and layers; Л1–Л3, Progress, Preservation, T1, T2(a), T2(b) | done |
| P1c | type variables and schemes, (Let-Gen) with the premise `P`, Л2 for schemes, lemma 3.1, T3 | done |
| P2a | regions and capture sets as atoms, `⌈·⌉` and its commutation with substitution, qualifiers on arrows and `Eff`, (≤-Cap), nominal types with parameters, schemes over variables of every kind with facts `Θ`, (Lam)/(Task) with what the body holds, (Scope), the run-time forms `inside`/`cap`/`own`, `open_H`, `op_H`, `Scope.add_finalizer`, `Fiber.spawn`; regions `Λ` with children and finalizers, closing; Л4 without the store, T1–T3 again | done |
| P2b | Л3 under a substitution that fits the term (`Refill`), the renaming of regions not yet created (`kill`), `reach_μ` and Л4 through the store | done |
| P2b′ | the freshness premises of (Inside) with its own facts, the facts of the order of creation (`Facts.age`), the renaming in every step, a configuration writes only regions that exist; Л6 | done |
| P2c | Л5, the invariant `⊢ Λ` (live work reaches only live regions), T4.1, T4.2, T1 with `op_H` | done |
| P2c′1 | Р37 in the model: the region of `provide`, `with` frames with it, children and finalizers inheriting the frames, the use of a service of an ended `provide` stuck; obligations `Provided {S} ϱ` in rows, discharged by `with` frames, by the end of a region and by `provide`, deferred in rows; T1–T3, T4.1, T4.2 again | done |
| P2c′2 | T4.3: no use of a service of an ended `provide` (`T4_3`), T1 with `op_H` without that stuck state | done |
| P4a2 | λFlick⁺: dictionary records `D_C ā` with `∀`-fields, projection with instantiation, globals with their bodies in `root`; T1–T4, T4.3 again | done |

T4.3 is the premise Г11 of T5 in the form Р142 gives it: an `ask_S` answered by a `with`
frame of the region `q` of a `provide`, inherited or not, happens only while `q` is live.
Its Lean statement (`T43Main.lean`), which `G11` in `Formal/RT/Hyp.lean` is to match:

```lean
theorem T4_3 {root e₀ t φ₀ c} (hroot : RootTy S root) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ∀ i e E N q v, c.at i = some e → OkE E → e = plug E (.app (.const (.ask N)) (.lit .unit)) →
      lookupE E N = some (q, v) → Live c.rgs q
```

Here `lookupE E N` is the region and the value of the innermost `with` frame of `E` that
provides `N`, and `c.at i` ranges over the main thread and the children of live regions.

## Map to `core.md` and `regions.md`

| Document | Lean |
| - | - |
| `core.md` §1.2 types, stages, rows; `σ` | `Ty`, `Stage`, `Row`, `Scheme` (`Syntax.lean`) |
| `regions.md` §1.2 atoms, capture sets, `τ^C`, `H ϱ`, `Owner ϱ`, `Scope ϱ` | `Atom`, `Caps`, `Elem`; `Ty.arrow`/`Ty.eff` with `C`, `Ty.caps`, `Ty.rg`, `Ty.hnd`, `Ty.owner`; `Row.scope` (`Syntax.lean`) |
| `regions.md` §2.1 `⌈τ⌉`; its commutation with substitution | `Ty.reach`; `Ty.reach_substT`, `Scheme.reach_substT` (`TyOps.lean`, `Reach.lean`) |
| `regions.md` §2.4 `Θ ⊢ Outlives C ϱ` | `Facts`, `Outl` (`Syntax.lean`); dropping facts that follow: `Typ.cut` (`Cut.lean`) |
| type substitution, `Gen` and instantiation | `Ty.substT`, `Ty.up`, `liftT`, `instT` (`TyOps.lean`) |
| `core.md` §1.3 terms, patterns; §3.1 run-time forms; `regions.md` §1.3, §3.1 | `Tm`, `Pat` (`Syntax.lean`) |
| what a term holds: free variables, run-time regions, locations (the premise of (Lam)/(Task)) | `Tm.Free`, `Tm.Has`, `held` (`Reach.lean`) |
| §1.3, §3.1 values | `Value`, `IsEnv` (`Typing.lean`) |
| `core.md` §1.5, `regions.md` §1.5 constants | `Const`, `Const.ty`, `Const.ok` (`Syntax.lean`) |
| §2.1 `⊑`, §2.2 `≤`, (≤-Cap) | `Stage.le`, `Le` (`Syntax.lean`, `Typing.lean`, `Order.lean`); `Le.reach` |
| `core.md` §2.3, §3.6 rules; `regions.md` §2.3–§2.5, §3.4 | `Typ` (`Rules.lean`) |
| (Sub) | admissible: `Typ.sub` (`SubAdm.lean`) |
| §2.4 patterns, `Exh`; (Match-Val), Р29 | `PatTy`, `Shp`, `Exh`, `Covers` (`Typing.lean`) |
| (Try), (Handle) | `HPat`, `Catches`, `HCov`; `tryR`, `tryP`, `handle` |
| §3.6 `⊢ μ : Ψ`; §2.6 `Σ₀` | `StoreTy`, `RootTy` (`Rules.lean`) |
| `core.md` §3.2 contexts, `lookup(E, S)`; the frame `inside π E` | `Frame`, `plug`, `lookupE` (`Semantics.lean`) |
| `core.md` §3.3 `→ₚ`; §3.4–3.5 steps; `regions.md` §3.2–§3.3 | `PHead`, `RHead`, `Step`, `Steps` (`Semantics.lean`) |
| `regions.md` §3.1 `Λ`, children step at any time | `Rgn`, `Cfg`, `CStep`, `CSteps` (`Semantics.lean`) |
| lemma about `build` | `canonical_layer` (`CanonLayer.lean`) |
| Л1 | `canonical_arrow`, `canonical_ref`, `canonical_eff`, `canonical_owner`, `canonical_hnd`, `canonical_env`, `canonical_errors`, `Typ.valueAny` (`CanonForms.lean`, `CanonEnv.lean`, `Canonical.lean`) |
| (Let-Gen), (Let-Mono) | `Typ.letGen`, `Typ.let_` (`Rules.lean`) |
| type substitution lemma | `Typ.substT`, with `Fits` (`TySubst.lean`, `SubstPat.lean`) |
| Л2 | `Typ.subst`, `Typ.instantiate`, `SubOk.one` (`Substitution.lean`) |
| Л3 | `ctx_typing` (`Lemma3.lean`); its `Refill` holds after any substitution that fits the term (`Context.lean`) |
| Л4 without the store: a value holds no more than its type shows | `Typ.holds` (`Held.lean`) |
| §5.2 `reach_μ`; Л4 | `Reaches`; `lemma_4`, `lemma_4_cell` (`Lemma4.lean`) |
| regions not yet created, renamed away; facts of the order of creation | `kill`, `Ty.YF`, `Tm.Below`, `Facts.age` (`Fresh.lean`, `Rules.lean`) |
| (Scope) and (Inside) in a run-time configuration | `RtCtx`, `scope_body`, `inside_entails` (`ScopeStep.lean`) |
| a configuration writes only regions that exist | `CfgBelow`, `Step.below` (`Below.lean`, `BelowStep.lean`) |
| Л6 | `lemma_6` (`Lemma4.lean`); the invariant of a configuration: `CfgTy` (`Soundness.lean`) |
| §3.4 `⊢ Λ`: where a region may be written, the ancestors of a region | `Tm.Within`, `LocOk`, `Nest` (`Within.lean`, `Nest.lean`); kept by every step: `Nest.step`, `Nest.cstep` (`NestMain.lean`, `Regions.lean`) |
| Л5 | `Nest.nested` (`NestClose.lean`); in every reached configuration: `reach_nest`, `Nest.live` (`Regions.lean`) |
| §2.7 obligations `{Provided {S} ϱ}`, uses of a service of a `provide` | `Elem.ob`, `Elem.need`; `Row.obs` in `Const.ty` of `spawn`/`addFin` (`Syntax.lean`); `Row.withP`, `Row.disch`, `Row.needs` in `Typ.with_`, `Typ.scope`, `Typ.inside`, `Typ.provideL`/`provideE` (`Rules.lean`) |
| the region of `provide`; a service of an ended `provide` | `RHead.provideEnv`, `Tm.with_ q θ e`, the premise `Live Λ q` of `Step.askWith` (`Semantics.lean`); `DeadAsk` (`StepFrame.lean`) |
| runtime.md §3.5: a child and a finalizer keep the `with` frames | `withs`, `Step.spawn`, `Step.addFin` (`Semantics.lean`); their typing `withs_typing`, `fin_await` (`KidTy.lean`) |
| an `inside` form is younger than the `with` frames around it | `Tm.Asc`, `CfgAsc`, `Step.asc` (`Asc.lean`, `AscStep.lean`) |
| T4.1, T4.2 | `T4` (`Regions.lean`) |
| T4.3: a child and a finalizer of a live region `ϱ` use services only of `provide` regions that are ancestors of `ϱ`; an `ask_S` leaves its use at the top of the thread or at the `inside` form of the region of its frame | `KidRow`, `withs_typing` (`KidTy.lean`); `KidNeeds`, `KidNeeds.step` (`T43.lean`); `env_dom`, `need_out` (`NeedOut.lean`); `cstep_all`, `T4_3` (`T43Main.lean`) |
| T1 with `op_H` | `T1_regions` (`T43Main.lean`) |
| `elaboration.md` §0.1 λFlick⁺: `D_C ā`, dictionary records, projection with `∀`-instantiation, globals | `Ty.dict`, `Ty.spine`, `Sig.dict`, `Sig.glob` (`Syntax.lean`); `Tm.drec`, `Tm.glob`, `J.dfs`, `Typ.drec`, `Typ.dcons`, `Typ.dproj`, `Typ.glob`, `Roots`, `RootTy` (`Rules.lean`); `Step.glob` (`Semantics.lean`); `canonical_dict`, `dict_field` (`DictCanon.lean`); `glob_preservation` (`Preservation.lean`) |
| Progress | `progress` (`ProgressMain.lean`) |
| Preservation | `preservation` (`Preservation.lean`); of a configuration: `cstep_typed` (`Soundness.lean`) |
| T1 | `T1` (`Soundness.lean`) |
| T2(a), T2(b) | `T2a`, `T2b`, `T2b_await` (`Effects.lean`) |
| lemma 3.1, T3 | `lemma_3_1`, `T3` (`Generalization.lean`) |
| a polymorphic use; a scope that acquires and uses, as checks | `Examples.lean` |

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

Variables of every kind — types, regions, capture rows — are de Bruijn indices too
(`tvar n`), and kinds are not checked; a run-time region `π` is `rg π`. An **atom** is a
variable or a run-time region, and a capture set is a set of atoms (`Atom → Prop`).
`⌈τ⌉` is a capture set, and a substitution `σ : Atom → Ty` maps a set by replacing each
atom `a` with `⌈σ a⌉`; then `⌈τ[σ]⌉ = ⌈τ⌉[σ]` holds by definition of the substitution on
sets (`Ty.reach_substT`). A capture row variable instantiated by `caps C` is the set `C`,
a region variable instantiated by `rg π` is `{π}`, a type variable instantiated by `υ` is
`⌈υ⌉`: one substitution serves all kinds. A row is a set of elements, a name or an atom:
in a row of services the atom `ϱ` is `Scope ϱ`, in a row of errors the atoms bound what
the errors reach (`⌈Errors ε⌉`), so rows are substituted too.

Types have no binders; a scheme `⟨k, Θ, τ⟩` binds the indices `0 … k-1` of `τ` and of its
facts `Θ`. Terms carry no types (Curry style), so a type substitution changes the judgment
and not the term; constants carry no instance either, (Const) picks it (`CArg`).
(Let-Gen) types its right-hand side with `Γ`, `Θ` and `Ψ` shifted by `k` (`Ty.up k`) and
the facts of the scheme assumed: the generalized variables are fresh by construction,
which is `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)` of `Gen` and Л2. (Scope) does the same with one variable,
its region `s`, so `s ∉ fr(Γ)`, `s ∉ fr(τ)`, `s ∉ fr(r)` and `s ∉ fr(ε)` hold by
construction; its body has `s : Owner s` and the fact that everything outside outlives
`s`. A step of `scope` substitutes `rg π` for `s` and `own π` for the owner.

Lists inside terms and types are spines, so `Tm` and `Ty` are plain inductive types and
`induction` works on them and on the typing derivation:

- a constructor application `K v₁ … vₙ` is `app (… (app (ctor K) v₁) …) vₙ`, as in `core.md`;
  constructor patterns are spines too (`Pat.app`), and so is a nominal type `T ā`
  (`Ty.tapp`, `Ty.tcon`);
- a record is `rcons ℓ₁ e₁ (… rnil)`, a record type `Ty.rcons ℓ₁ τ₁ (… Ty.rnil)`;
- an environment `env θ` is `econs S₁ v₁ (… enil)`;
- match and `try` arms are `acons p b rest`, `aguard p g b rest`, ending in `anil`, with
  their own judgment forms `J.alts` and `J.halts` in the same inductive `Typ`.

Rows are closed sets, `Elem → Prop`; equality of rows is extensional, which is
`≡` of §1.2. Every rule types its premises at the stage of its conclusion, and the
subsumption `≤` is folded into the rules whose result may be an arrow or an `Eff`; (Sub)
is then admissible (`Typ.sub`). With (Sub) this is equivalent to the `⊔` form of §2.3,
and inversion stays a plain `cases`. (≤-Cap) is a premise of `Le.arrow` and `Le.eff`:
`⌈τ⌉ ⊆ ⌈τ'⌉` of the whole value.

The premise of (Lam), (Task) and the fixpoint is `held Ψ Γ e ⊆ ⌈τ^C⌉`: `⌈Γ(y)⌉` of each
free `y`, and also the run-time regions the body writes down and the cell types of its
locations (Р141). The substitution lemma keeps it because a substituted value holds no
more than its scheme shows (`Typ.holds`, the part of Л4 without the store).

A thread steps in `⟨Λ, μ, e⟩`. `Λ` is a list of regions: live or closed, children and
finalizers, both computations. A configuration is a main thread with `Λ` and `μ`, and
`CStep` steps the main thread or a child of a live region.

## Assumptions and simplifications

Each item narrows `core.md` or `regions.md` unless it says otherwise.

1. **Rows are closed**, so row variables are neither generalized nor instantiated:
   constants are typed at any rows (`CArg`), but schemes do not abstract over them.
   Polymorphism is over variables of the other kinds. Nominal types have parameters
   (`Sig.ctor` gives their number); their fields mention only the parameters
   (`Sig.ctor_closed`, `Sig.ctor_reach`); services have no parameters (`Sig.impl_closed`).
   (Let-Sig) is (Let-Gen) or (Let-Mono) with the annotation erased: terms carry no types,
   and the premise `ᾱ ≠ ∅ ⇒ φ₁ = P` is the premise `P` of (Let-Gen). `let rec` is
   monomorphic in its body and its scope (`σ° = τ_f` and `σ = τ_f`). Higher-rank
   arguments are out of scope, as in `core.md`.
2. **Declarations** are a parameter `S : Sig`: constructor ↦ the number of parameters,
   argument types and type name; service ↦ `S.Impl`. An error type `E` is an ADT whose
   constructors are the error's constructors.
3. **`Unit`, `Bool`, `Int`** are base types with literals (`Lit`), not nominal ADTs;
   `True`/`False` patterns are literal patterns.
4. **Records are closed with a fixed field order.** No row tail and no equality up to
   permutation: the elaborator is assumed to order labels; (Rec) requires the tail to be a
   record type (`Ty.IsRec`). Record patterns are always open (`{…, ..}`), as in `core.md`.
5. **`Exh` over shapes.** `Exh(τ, Π)` quantifies over `Shp τ v`, the closed values of `τ`
   as far as patterns see them (variables, functions, references, computations,
   layers, errors, environments, capabilities and owners are opaque), not over typed
   values: the typed version would put `Typ` in a negative position of its own
   definition. The fields of a constructor have the types of its declaration, so a field
   of a parameter type is opaque: exhaustiveness through type arguments is not modeled.
   The exhaustiveness algorithm (`exhaust.rs`) is not modeled.
6. **`raise` is a term former**, not a first-class constant; `λx. raise x` recovers the
   constant. It takes `Errors ε` (with stage `R⟨none; ε⟩`) or a value of an error type
   `E ῡ` (with `R⟨none; {E} + ⌈E ῡ⌉⟩`, `Row.err`), which is the `Errors {E ῡ} ≡ E ῡ` of
   §1.5 at the only place the core needs it. (Errors-Val) types values only, as in §3.6.
7. **`try` arms** are `x`/`_` at `Errors ε_b` or a constructor pattern `E p̄` at an error
   type `E` without parameters. `H` is a judgment index: each arm without `when` may add
   what it catches (`Catches`), names only. (Handle) uses the value form of the coverage
   premise (Р102); what the caught errors reach passes to the errors it lets through.
8. **Environments.** `Env ρ` means "binds exactly `ρ`", as (Env), (Bind) and (Join)
   give it: T4.3 needs that a `with` frame answers `S` exactly when its row has `S`. `with θ e` and `econs` require
   `θ` and the bound values to be values. The disjointness of `Layer.merge` is kept
   (`Const.ok`) though soundness does not need it.
9. **Allocation is deterministic:** `Ref.make v` takes `ℓ = μ.length`, `scope` takes
   `π = Λ.length`.
10. **Programs** are closed terms typed in the empty store with no facts; modules
    (`core.md` §2.6, path 4 of `regions.md` §4) are not modeled. `Σ₀` and the bodies of the
    globals of λFlick⁺ are a parameter `root : Roots` with `RootTy`; Г3 is the hypothesis of `T1` that `Σ₀` covers the services of the program's
    stage. Progress reports an `ask_S` that neither a `with` frame nor `Σ₀` answers
    (`AskStuck`) instead of assuming Г3.
11. **Л3** is stated as `ERel`: the hole's services are within those of the context, of
    its `with` frames and the `Scope π` of its `inside` frames, its errors grow only under
    a `try` frame, and a pure context has a pure hole. Under `K` right-hand sides of
    (Let-Gen) the hole is typed `K` variables deep, with `Ψ` and `Θ` shifted and the facts
    of the schemes assumed, and is pure.
12. **Steps carry labels** (`Lbl`): pure, runtime, or `root S` for an `ask_S` answered by
    `Σ₀`; T2(b) is about the `root` labels. `todo → die` is labelled pure, as T2(a) allows
    `die`; `bind_S` and `join` steps are pure (Р101).
13. **Cell types may be open** (Р103). `Ψ(ℓ)` is a monotype that may mention free type
    variables of the configuration's derivation; (Let-Gen) shifts `Ψ`, so it never
    generalizes them.
14. **T3** is stated as: reachable configurations are typed at the program's type, the
    store typing only grows (a cell keeps its allocation type), and a redex under the
    right-hand side of a (Let-Gen) with `ᾱ ≠ ∅` is pure while the cell types it sees
    avoid `ᾱ` (they are shifted past it, `Ty.Avoids`).
15. **Qualifiers are hidden only.** The value bound `α^{≤C}` and the refinement of
    (App-Pure) are not modeled: both let the checker accept more programs, and `Ref.*`
    instantiate the cell type directly, which keeps it invariant. The hidden qualifier of
    data with hidden storage is a parameter of kind `CRow` of its type, so (Con) and
    (Proj) are (App) and (Match) at an instance (Р61).
16. **Capabilities and constants.** `H ϱ̄ τ̄` is one family `hnd ϱ` with one region;
    `open_H` takes the owner of its region and nothing else (Г6: the region is explicit)
    and registers the use that releases it; `op_H` returns `()`; `cap π` has no index `n`.
    `Fiber.spawn` has no supervisor and returns `()`: there is no `join`; a child does
    not inherit the services of the context that spawns it (λRT's). A finalizer runs as a
    child once the body of its region is finished and the earlier children are, the last
    one first; the region closes when none is left. Closing waits for every child.
17. **Facts are not chained.** `Θ ⊢ k ≥ ϱ` is reflexive or a fact; a scope gives one fact
    for everything outside it, which is what transitivity would derive for a body
    (`regions.md` §2.4, rules 1 and 2). A run-time configuration is typed with the
    facts of the order of creation (`Facts.age`: a region outlives every region created
    after it); (Inside) adds its own facts about `π`, which name only older regions.
18. **(Err-Raise) is checked where errors leave a scope**: the error row of the body of
    `scope` does not mention its region by construction. The stricter rule of the spec
    (also for errors caught inside) is not needed by the model; (Err-Decl) is not either,
    since a row of errors carries what its errors reach.
19. **Regions not yet created are renamed away in every step.** A derivation may name
    a region before it exists (the type of a hole is chosen by it). Each step renames
    the derivation by `kill |Λ|`, which sends regions `≥ |Λ|` to `Unit`; this fits the
    term because a configuration writes only regions that exist (`CfgBelow`). Then a new
    scope gets a region no type mentions, and (Inside) has its freshness premise
    `π ∉ fr(τ) ∪ fr(ε)`. Hence T1 and T3 assume of the program that it writes no
    run-time region and that its type and stage name none (`hp`, `ht`, `hφ`): true of
    every program, which has no run-time forms; T2 assumes the same of its configuration
    (`hb`, `hΨ`). T1 is stated with two blocked outcomes: a finished region body
    waiting for a child (deadlock freedom is λRT's), and a step that needs a region
    that is not live or a service of an ended `provide`; `T1_regions` drops the second
    by T4 and T4.3.
20. **`⊢ Λ` is an invariant with ghost data.** `Nest` relates a configuration to the
    ancestors `anc π` of each region, the regions `B ℓ` each cell may hold and the
    thread `home π` of the `inside π` form; T4 needs no other change to the model.
    A place in a thread names the regions of its base and of the `inside` forms
    around it; a location sees through its cell exactly what its type names there.
    `inside` forms stand only where the term evaluates, so a step neither copies nor
    drops one, and only `scope` and closing change them. T4.2 is stated for every
    reached configuration (live work reaches only live regions), which after a step
    that closes `π` says that nothing that can step reaches `π`.
21. **Obligations of `provide` (regions.md §2.7) are row elements.** `Provided {S} ϱ` is
    an element `ob S ϱ` of a row of services; `Fiber.spawn` and `Scope.add_finalizer`
    put one for each service of the work into the stage of the call (`Row.obs`). A
    `provide` in a program binds no region variable: an obligation about a region
    created in its body is discharged at the end of that region (`Row.disch`, the second
    case of §2.7, `S` coming from outside it), and one that reaches a `provide` giving `S`
    is rejected, which is the first case. An obligation not discharged stays in the row
    of the stage and of the arrows that carry it, which is the deferred element and the
    given `Provided` of a signature. At run time a `with` frame carries the region of its
    `provide`, records the uses `need S q` of its services and discharges an obligation
    when its region is older (`Row.withP`). A child and a finalizer keep the `with` frames
    of the place that registered them (`withs`); asking a service of an ended `provide`
    is stuck (`DeadAsk`). Typing a new child needs `Tm.Asc`: a region is younger than the
    `with` frames around its `inside` form. T4.3 adds the invariant `KidNeeds`: a child
    or a finalizer of a live region `ϱ` has a typing whose uses `need S q` name ancestors
    of `ϱ`, which the obligation `Provided {S} ϱ` of its registration gives
    (`withs_typing`). The row of the work `spawn` and `addFin` take has no uses (`Const.ok`):
    uses come only from `with` frames, so the derivations `regions.md` gives have none there.

## Findings

- **Р100** (P1a): a partially applied constructor is a value, and Л1 for an arrow type
  has to list it (`Value.ctorApp`, `canonical_arrow`).
- **Р101** (P1b): `bind_S` and `join` steps were runtime steps while their rules are
  pure, so T2(a) failed for a bare `bind_S v`; they are pure steps now.
- **Р102** (P1b): (Handle) asked for the whole row `ε_b ⊆ H + ε'`, which a failed arm
  breaks; it now asks for the error of the value only, as `flick-model` already did.
- **Р103** (P1c): closed cell types and a fixed program type cannot both hold; cell types
  are monotypes with possibly free variables, and `Gen` excludes `ftv(Ψ)`.
- **Р140** (P2a): (Spawn) bounded only the hidden qualifier `C` of the child; the child
  also holds what its result shows. The premise is `Outlives (C ∪ ⌈a⌉) ϱ` (`Const.ok`);
  the checker already checks it (`regions.md` §6).
- **Р141** (P2a): the premise of (Lam)/(Task) must count the run-time regions and cell
  types written in the body, or Л4 fails for `λx. (λy. ()) (cap π n)` (`held`,
  `regions.md` §6).

## λRT and T5

`Formal/RT/` is a model of its own: it imports nothing from the core above, and the core
is a parameter of it.

| Stage | Scope | State |
| - | - | - |
| P3a | causes: `⊕`, `⧺`, `head`, `recover`, I_ε; R8, R5 | done |
| P3b | configurations, steps §3.2–§3.6, the closing C0–C6 by phases; И1–И3; R1–R4 | done |
| P3c | layers and memo maps, `Ref`; И6, R6 (with Г11), R7; T5 | done |

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
| §3.5 memo maps, `build`, observers | `Memo`, `Ent`, `lookup`, `Step.provide`, `.provHit`, `.provBuild`, `.buildOk`, `.buildFail`, `.finObs`, `.finObsLast` |
| И6 | `Inv6.obsIn`, `Good6` of `release` (`RT/Layers.lean`) |
| R6; its premise from Р37 | `R6`, `Good6`, `GoodUse` (`RT/Layers.lean`, `RT/T5.lean`); `G11` (`RT/Hyp.lean`) |
| §3.7, R7 | `Step.modify`, `cell`, `R7` (`RT/RefLin.lean`) |
| T5 | `T5` (`RT/T5.lean`) |

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
11. **R6** is stated over the journal. (a) A map builds a layer again only after its earlier
    build of that layer was released (`Good6` of `build`): within one execution of the
    outer `provide`, whose owner observes the build until its own (C5), that is "at most
    once". (b) A build is released only after every consumer's observer ran (`Good6` of
    `release`). (c) No service is used after its build is released (`GoodUse`), under Г11.
    `G11` is about the answering entry only: `Act.use π` names the `provide` owner whose
    environment entry answers the `ask_S`, and at such a step `π` is live. A child may carry
    an entry of a closed `provide` that it never asks (Р142). P2 gives it as `T4_3`: the
    region `q` of the `with` frame that `lookupE E N` finds for an `ask_N` is `Live`.
    A layer's build code runs in the providing fiber with the area `σ` as current owner; a
    failed build closes `σ` and makes the `provide` fail.
12. **R7** is `Hist GoodRef`: each `Ref.modify ℓ` read the value of the latest `make` or
    `modify` of `ℓ` (`cell`), and the memory is the replay of the journal. Allocation is
    deterministic, as in P1.

## Elaboration and T6

`Formal/Elab/` is another model of its own; it imports nothing from the core. It covers
T6 of [`docs/formal/elaboration.md`](../docs/formal/elaboration.md) relative to an abstract
target: λFlick⁺ is the structure `Target`, whose typing and laws are fields; P4a is to
instantiate it.

| Stage | Scope | State |
| - | - | - |
| P4a | λFlick⁺ (Р54) as an instance of `Target`; T1–T3 for it | open |
| P4b | the source, `Γ; Δ ⊢ e : τ ! φ ⇝ e'` (§2.4), dictionaries typed, T6(a); `Sim` from the steps of the target | done |
| P4b′ | the sugar of §3 (`do`, `for`, `?`, blocks) as source terms; each elaborates with the checker's type, T6(a) covers it | done |
| P4c | `≈`, К1–К8, lemmas 6.1 and 6.2, T6(b) | done |

| `elaboration.md` | Lean |
| - | - |
| §1.1 classes, instances; §2.1 superclass slots | `Prog`, `Inst`, `Prog.super` (`Elab/Syntax.lean`) |
| §2.2 dictionary terms `d` | `Dict`, `apps`, `Dict.subst` (`Elab/Syntax.lean`) |
| §2.3 `Δ*`, (Given), (Prim), (Struct), (Inst) | `Clo`, `Ent` (`Elab/Syntax.lean`) |
| §4.1 К1–К8 | `Coherent` (`Elab/Hyp.lean`) |
| §4.2 `≈` | `Eqv` (`Elab/Syntax.lean`) |
| Lemma 6.1, Lemma 6.2, T6(b) | `lemma_6_1`, `lemma_6_2`, `T6b` (`Elab/Coherence.lean`) |
| §1.1 method schemes | `Scheme`, `Prog.methods` (`Elab/Syntax.lean`) |
| §2.2, §2.4 `Γ; Δ ⊢ e : τ ! φ ⇝ e'`: (Var-Dict), (Method), (Let-Sig-Dict) | `Exp`, `Tm`, `Elab` (`Elab/Source.lean`) |
| §0.1, §2.1 λFlick⁺, `D_Q`, `⟦σ⟧`, `⊢⁺` | `TTy`, `TSch`, `Scheme.tr`, `Target` (`Elab/Target.lean`) |
| `Δ ⊩ Q ⇝ d ⇒ ⟦Δ⟧ ⊢⁺ d : D_Q`; T6(a) | `ent_typed`, `T6a` (`Elab/T6a.lean`) |
| §3.1 `do` | `Stmt`, `Sugar.desugarDo`, `DoOk`, `do_elab`, `T6a_do` (`Elab/Sugar.lean`, `Elab/DoNotation.lean`) |
| §3.2 `for`, §3.3 `?`, §3.4 blocks | `Clause`, `Sugar.gen`, `Sugar.loop`, `GenOk`, `LoopOk`, `forSeq_elab`, `forList_elab`, `loop_elab`, `tryResult_elab`, `tryOption_elab`, `seqUnit_elab`, `T6a_sugar` (`Elab/Sugar.lean`, `Elab/Comprehension.lean`) |
| Prelude methods and constants of Γ₀ the sugar uses | `Sugar` (`Elab/Sugar.lean`) |
| "≈ gives the same result" | the hypothesis `Sim` (`Elab/Hyp.lean`); from the steps of a target `Target.sim`, then `T6b_target` (`Elab/Run.lean`) |

Simplifications:

1. Constraint types are first-order terms with binary application. К1 is semantic: heads
   that some substitutions make equal belong to one instance, which is what a failed
   first-order unification says.
2. (Improve) gives no dictionary, so `Ent` starts from improved constraints; К2 and К3 only
   make improvement order-independent and are not fields. К4 gives К1 for the whole program,
   which `Coherent` assumes directly. К8 makes premise names fresh; premises here are
   numbered.
3. К5 is a well-founded transitive order `lt` with the instance context, a superclass and
   a record's field below the constraint, plus "the variables of the context are in the head".
4. (Struct) and (Prim) are black boxes: `fields` and `prims`, which commute with
   substitution; К6 keeps the compiler's classes apart from the program's instances.
5. An expression is a skeleton with dictionary holes all under one `Δ`; `Sim` says filling
   them with `≈` dictionaries gives the same result. A hole under a `λδ̄` of (Let-Sig-Dict)
   sees a longer `Δ`; T6(b) over two whole `Elab` derivations, which may also pick different
   types, is not stated.
6. The source of T6(a) has variables, methods, constants of Γ₀ (`Prog.consts`, with the
   dictionaries of their schemes, as (Var-Dict)), application, `λ` over a monotype, `let`
   without generalization, `let` with a signature, `if` and a `match` on `Option`; no
   constructors, other `match` or higher-rank parameters. Its types are the first-order
   `Ty` with the arrow `a →φ^q b` (`Ty.arr`, Р186), whose qualifier is a capture row
   written as a type (a variable, `cnil`, `cun`); `Bool` and `Option` are reserved
   constructors (`boolTy`, `optTy`). (Lam) and (Let-Sig-Dict) show what a closure holds,
   `heldS` (Р185, Р187); qualifiers match exactly, there is no (≤-Cap). (Sub) is kept for stages only: a value has any stage, an application,
   a `let` and a `match` run at one stage `φ`, the arrow applied and the right side of a
   `let` at `ψ ⊑ φ` (Р182). A term at `P` elaborates at any stage (`Elab.anyStage`), and
   renaming term variables keeps an elaboration (`Elab.rename`, weakening `Elab.shift`).
7. `Target` is an interface, not λFlick⁺: its types are monotypes, stage arrows and `D_Q`,
   a context entry is `∀ᾱ. t`, and a `∀`-field of `D_Q` shows only through the law `meth`
   (projection with instantiation). Constants of Γ₀ are typed by their schemes (`const`).
   Arrows carry qualifiers: `lam` and `lamD` have the held premises of the source, and
   the arrows of methods, constants, instances and `struct_C` hold nothing (`cnil`), so the
   `q` of their schemes is never read.
   The types of instances (`inst`: `I : ∀β̄. D_ctx →P
   D_head`, by (Instance) of §2.5 and К7) and of (Struct)/(Prim) dictionaries are laws.
   `lamD l` binds `l` premises at once, the `i`-th argument being the premise `i`;
   λFlick⁺ curries it. P4a has to prove each law of its own typing.
8. The run of the target is `Target.St`, `load`, `Step` and `obs`; a skeleton is a `Tm` with
   holes `hole i` and the type and stage of its fillings, and its result is the set of observations of the terminating runs of the
   filled term (divergence is not compared). `Sim` follows by induction on the run from
   three fields, which P4a has to prove for λFlick⁺ with a relation `Rel` of its choice:
   `load_rel` (closed terms typed at one type that differ by `≈` in dictionary positions,
   `TmEqv`, load to related states), `step_rel` (a step of one side is matched by zero or more steps of the other:
   evaluating `(I d̄).$slotᵢ` and `sᵢ[δ̄ := d̄]` takes different numbers of steps) and
   `final_rel` (a result of one side is reached by the other). Only typed dictionaries
   evaluate, so `Sim` and `T6b` speak of fillings with `Exprs.Ok`, for a target the typing
   of the filled skeleton, and `T6b_target` takes it for both fillings.
9. The sugar (§3) desugars into `Exp`; `Sugar` names the Prelude methods (`flat_map`,
   `map2`, `pure`, `nothing`, `iter`) and the constants of Γ₀ with their schemes, as
   hypotheses on the program. The schemes have capture variables; qualifiers of non-arrow
   types are dropped (Р188). A lambda of the sugar takes as its qualifier a row that covers
   its context (`ctxRow`). The checker's rules are the judgements `DoOk`, `GenOk` and
   `LoopOk`; `?` and `s; rest` are premises of their lemmas. Patterns are variables (and `_`):
   a refutable pattern needs `match`, which the source lacks. The values of a group
   `p₁ <- e₁ and … and pₙ <- eₙ` form a left-nested tuple built by `map2 pair`, and
   `λ(p₁, …, pₙ). e` is `unc n (λp₁ … pₙ. e)` with a constant `unc n` per arity; the checker
   uses flat tuples with the same curried patterns. The clauses of `for … yield` run at `P`
   (Sync calls are not modelled) and the loop of `for … do` at its stage, with one constant
   `Seq.each_φ` per stage (Р184). Each source of `for` goes through `iter` (Р183). Only the
   last step of `?` on an `Option` is modelled, `let y = e? in b` ⇝ `match`; the A-normal
   form before it is not.

### P4a design

The owner's decision on Р185 is that T6 goes through the core with regions. λFlick⁺ is
this Lean core plus nominal dictionary records `D_C ᾱ` with `∀`-fields, and captures
enter the elaboration source and `Target`. This section is the plan for P4a. Each piece
is committed without `sorry` and builds from a clean `git archive`. The next free Р
number is Р189.

**Status.** Р185–Р187 are recorded in `elaboration.md` §5, and §0.1 and §2.1 are
updated. P4a1 is done; Р188 records the Prelude schemes. P4a2 is done (Р194). P4a3 and
P4a4 are open. The pieces below are in their intended order.

#### P4a1: captures in `Elab` and `Target`

Only `Formal/Elab/` changes; the core is untouched.

- **Types** (`Elab/Syntax.lean`). `Ty` gets `arr (φ : Stage) (a b q : Ty)`, `cnil` and
  `cun (q q' : Ty)`. The qualifier `q` is a capture row written as a type: a variable,
  `cnil`, or a union. Substitution stays structural, and `DecidableEq`,
  `Ty.agree_of_eq` and К1 keep working without semantic sets. `Stage` moves to
  `Syntax.lean`. `arrTy` becomes `arr` (Р186), and `Ty.reach : Ty → Nat → Prop` is
  defined with `⌈var n⌉ = {n}`, `⌈con⌉ = ⌈cnil⌉ = ∅`, `⌈app⌉ = ⌈cun⌉ = ∪` and
  `⌈arr φ a b q⌉ = ⌈a⌉ ∪ ⌈b⌉ ∪ ⌈q⌉`. Capture variables are ordinary variables, so
  (Var), (Method) and (Const) instantiate them through `ts` with no new rule. Schemes
  have no `Outlives` facts: `Fiber.Outlives` is dropped by `⟦·⟧` (§2.1).
- **Schemes.** `Scheme` gets `q : Ty`, the qualifier of its dictionary arrows (Р187).
  `Scheme.subst` lifts `q` like `ty`. The `q` of methods and of `Prog.consts` is never
  read: the laws `meth` and `const` use `cnil`, so `Prog` needs no well-formedness
  hypothesis for it. `Scheme.reach` is the reach of `Scheme.tr s` below its `k`
  binders, so it equals the core's `Scheme.reach` of the translation.
- **Judgement** (`Elab/Source.lean`). Add `Exp.Free` and `Tm.Free`, and
  `heldS Γ e n := ∃ x s, e.Free x ∧ Γ[x]? = some s ∧ s.reach n`. `Δ` adds nothing,
  because `⌈D_Q⌉ = ∅`.
  - (Lam) gets the premise `heldS Γ (.lam b) ⊆ ⌈arr φ a t q⌉`.
  - (Let-Sig-Dict) gets the premise `σ.ctx ≠ [] → heldS (Γ↑σ.k) e ⊆ ⌈σ.ty⌉ ∪ ⌈σ.q⌉`.
  - No (≤-Cap) in the source: qualifiers match exactly. This is a simplification;
    T6(a) does not need completeness.
  - New lemma: elaboration adds no term variables
    (`Elab … e' → e'.Free x → e.Free x`).
- **`Target`.** `TTy.arr φ a b q`. `dicts q qs t` puts `q` on every dictionary arrow.
  `Scheme.tr s = ⟨s.k, dicts s.q s.ctx (.mono s.ty)⟩`.
  - The `lam` law gets the held premise over `Tm.Free`.
  - The `lamD` law gets the premise `qs ≠ [] → heldT Γ e ⊆ ⌈t⌉ ∪ ⌈q⌉`.
  - `meth`, `const`, `inst`, `strct` and `prim` use `q = cnil`.
  - Decided: the `Rel` fields take typing premises, because P4a4 needs closed typed terms.
    `load_rel` gets `Has [] [] e t φ` and `Has [] [] e' t φ`. `Exprs` gets `Ok`, and
    `Sim` and `T6b` speak only of fillings with `Ok`. A skeleton of `Target.exprs` is
    `Tm × TTy × Stage`, `Ok` is the typing of the filled skeleton, and `T6b_target`
    takes it for both fillings. `step_rel` and `final_rel` take none: `Rel` carries them.
- **Re-proofs.** `T6a` needs the held lemma in the `lam` and `letSig` cases
  (`Elab.free`, `heldT_of`). `Weaken.lean` renames the held premises (`heldS_rename`);
  `Coherence.lean` needs only new `Ty` cases and the `Ok` premises of `T6b`.
- **Sugar.** Each higher-order parameter of a Prelude scheme gets a capture variable,
  and the arrows after it show that variable:
  `flat_map : ∀m a b c. (a →^c m b) →P (m a →^c m b)`. Checked against `Prelude.fl` and
  `Seq.fl`: they are the checker's types with the qualifiers of non-arrow types dropped
  (Р188). A lambda of the sugar takes `ctxRow Γ`, a row that covers its context.

#### P4a2: dictionary records in the core

Done. λFlick⁺ is the core of `Formal/` with dictionary records and globals; T1–T4,
T4.3 and `T1_regions` hold again with the same statements, `RootTy` now covering the
globals. It differs from the plan above in four places, each to keep the existing
proofs: projection is the core's `proj`, globals are a term form, fields are schemes,
and fields need not be values.

- **Type.** `Ty.dict c args` is `D_C ā` with `args = Ty.spine ā`, a `tapp` spine on
  `rnil`; `⌈dict c args⌉ = ∅` (§0.1), `substT` is structural (`Ty.spine_substT`,
  `Ty.spine_inj`, `DictOps.lean`).
- **Signature.** `Sig.dict c = some (k, fs)`: `k` class parameters and fields
  `(label, s)` with a scheme `s` whose type is over `s.k + k` variables, its own binders
  first (`dict_closed`); `Sig.glob g` is the closed scheme of a global (`glob_closed`).
  A field scheme may have facts, as a (Let-Gen) scheme; the elaboration gives none.
- **Terms.** `Tm.drec c r` with `r` an `rcons` spine is a value (`Value.drec`) and has
  no frame. Projection is `proj e l`: `Tm.field` reads through `drec`, so the existing
  pure step `proj v l → w` serves both. Fields are any pure terms without run-time
  forms; they need not be values, so slot fields may be globals (`glob I`) or let-bound
  variables, and `let s̄ = slots in drec …` is not required.
- **Typing** (`Rules.lean`). A judgment form `J.dfs fs` types the spine field by field:
  `Typ.dcons` types a field at its scheme as (Let-Gen) types its right-hand side, at `P`.
  `Typ.drec`: `S.dict c = some (k, fs)`, `us.length = k`, the spine at
  `fs` instantiated by `us`, `r.NoIns` and `held Ψ Γ (drec c r) ⊆ ⌈dict c ūs⌉ = ∅`
  (it holds nothing); conclusion `drec c r : dict c ūs` at any stage. `Typ.dproj`:
  `e : dict c ūs`, `fs.lookup l = some s`, and (Var) at the instantiated scheme of
  `s`: `ts`, its facts, `Le`. `Typ.glob` is (Var) at `S.glob g`.
- **Globals.** `Tm.glob g` steps to its body in a pure step, `Step.glob`, with label
  `pure`. The bodies are a table next to `Σ₀`: `root : Roots` has `svc` (the old
  `Σ₀`) and `glob`. `RootTy` gains `glob` (a body is closed source code without any
  `Site` and is typed at the scheme of its global) and `globs` (each declared global has
  a body). Instance bodies refer to other instances by name, so their typing is not
  circular. Not `Const.glob`: every constant is a value (`Value.const`), and a value
  must not step.
- **Proofs.** The `drec` and `dcons` cases are those of `letGen` (`TySubstBase.lean`:
  `Typ.drec_substT`, `Typ.dcons_substT`, `Typ.dproj_at`, `Typ.glob_substT`); a
  dictionary is canonical (`canonical_dict`), and its declared fields are there
  (`Typ.dfs_field`, `dict_field` with `Typ.instantiate`, `DictCanon.lean`); a global
  steps to a body typed by `Typ.instantiate` (`glob_preservation`); Progress takes
  `RootTy` for `globs`. `Value.field` takes the record typing, since a field of a
  dictionary need not be a value. `Examples.lean` projects a polymorphic method.
- **Р188.** Fields are typed at their schemes exactly, and a dictionary holds nothing,
  so a lazy instance whose method result holds its continuation (`Monad (Eff r e)`) is
  typed in the core iff its method scheme shows that: the core has (≤-Cap) on arrows and
  `Eff`, but no `τ^q` on other types. P4a2 does not decide Р188; P4a3 does.
- **Р194.** §0.1 also allows `∀` in an arrow parameter; the model has `∀` in dictionary
  fields only, as the source of `Elab` has no higher-rank parameters.

#### P4a3: the instance of `Target`

- **Types.** The translation `⟦·⟧ : Elab.Ty → Flick.Ty` maps
  - `var ↦ tvar`, `app ↦ tapp`;
  - `con c` to `adt` names, with `Bool` to `base bool` and `Option` to the ADT of `Sig`;
  - `arr φ a b q ↦ arrow ⟦a⟧ ⟦φ⟧ ⟦b⟧ ⌈⟦q⟧⌉`;
  - `cnil ↦ caps ∅` and `cun ↦ caps (∪)`.

  Stages map `⟦P⟧ = pure` and `⟦R⟧ = rt R₀ E₀`, with fixed name-only rows. Prove
  `⟦t.subst σ⟧ = ⟦t⟧.substT ⟦σ⟧` (with `Ty.reach_substT`) and `⌈⟦t⟧⌉ = ⟦⌈t⌉⌉`.
  `TTy.dict q ↦ Ty.dict`.
- **Terms.** `tr γ δ` maps `Γ`-variables and `Δ`-variables into one core context:
  `lam` pushes `γ`, `lamD l` pushes `l` entries of `δ`.
  - `meth`/`slot ↦ proj` (on a dictionary, `Typ.dproj`).
  - `inst`/`strct`/`prim`/`const ↦ glob _`, and `ap ↦ app`.
  - `ite`/`caseOpt ↦ match_` on literals and on the `Option` constructors.
- **Typing.** `Has Γ Δ e t φ := ∀ Ψ Θ Γc γ δ, Corr Γc Γ Δ γ δ → Typ S Ψ Θ Γc (tr γ δ e) (.ex ⟦t⟧ ⟦φ⟧)`,
  where `Corr` says `Γc[γ x] = ⟦Γ[x]⟧` and `Γc[δ k] = mono (dict …)`.
- **Hypotheses.** A bundle `Real P S` covers:
  - `Sig.dict` realises each class: slots, then methods, with `s.ctx = ⟨C, vars p⟩ :: rest`;
  - the globals: instance bodies built from `ins.slots` and method bodies, and
    `struct_C` with slots `struct_S (δ̄.$slotᵢ)` (Р180);
  - `RootTy` for the bodies of the globals (`root.glob`);
  - the `Option` constructors.
- **The laws.** Each law is one core rule plus the commutation lemmas.
  - `var`: `Typ.var`.
  - `lam`: `Typ.lam`; only `Γ`-variables have non-empty reach.
  - `let_`: `Typ.letGen` with `Φ = Facts.none`; `k = 0` uses `Typ.let_`.
  - `lamD`: `Typ.lam`, `l` times.
  - `meth`, `slot`: `Typ.dproj`.
  - `dvar`: `Typ.var`.
  - `app`, `dapp`, `dap`: `Typ.app`.
  - `inst`, `strct`, `prim`, `const`: `Typ.glob`.
  - `ite`, `caseOpt`: `Typ.match_` with `Covers`.

#### P4a4: `Rel` and `Sim`

- **States.** `St := Cfg`, `load e := ⟨[], [], tr e⟩`, `Step := ∃ l, CStep root l`.
  `Obs := Lit`, observed when the main thread is a literal (ground observation).
- **Dictionary evaluation.** A closed typed dictionary term evaluates in pure,
  deterministic main-thread steps to a `drec`. The proof is well-founded induction on
  К5's `lt`: the slots of an instance derive smaller constraints (К7 with `k5super`),
  and method fields are already values. Compositionality: for pure terminating `d`,
  `t[d] ⇓ v ↔ t[val d] ⇓ v`.
- **The relation.** `Rel` is the congruence on configurations generated by two kinds of
  pairs: closed pure dictionary computations with the same value, and translations of
  `Eqv`-related open dictionaries, closed under substitution (`Eqv.subst`). Two
  lemmas:
  - `Eqv d d'` with a closing `ρ` gives one value. For `instSlot` this follows from the
    instance's body, for `strctSlot` from Р180's `struct_C`, and for the congruences
    from compositionality.
  - `step_rel`. If the redex lies inside a related dictionary computation, `b` takes 0
    steps. Otherwise `b` first evaluates its dictionary at the redex position to the
    same value, then takes the same step.
- **Rejected alternative.** Inert dictionaries, where projection unfolds one slot,
  need Church–Rosser and standardisation of slot rewriting under call-by-value, which
  is more work.

**Order.** P4a1 and P4a2 are independent; they can run in parallel in separate
worktrees, and both touch this README. P4a3 needs both, and P4a4 needs P4a3. Record
where each piece stops in **Status** above.
