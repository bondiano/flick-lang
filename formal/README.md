# λFlick in Lean 4

A mechanization of the core calculus in [`docs/formal/core.md`](../docs/formal/core.md),
extended in place with the regions and captures of
[`docs/formal/regions.md`](../docs/formal/regions.md); separately, of the runtime
contract λRT in [`docs/formal/runtime.md`](../docs/formal/runtime.md)
(`Formal/RT/`, see [λRT and T5](#λrt-and-t5)); and of the elaboration of classes into
dictionaries in [`docs/formal/elaboration.md`](../docs/formal/elaboration.md)
(`Formal/Elab/`, see [Elaboration and T6](#elaboration-and-t6)). Lean 4.29.1
(`lean-toolchain`), no Mathlib.

## Building

```sh
cd formal
lake build --wfail      # or: nix shell nixpkgs#lean4 -c lake build --wfail
```

`--wfail` makes a `sorry` (reported as a warning) fail the build. CI runs the same build
in the `lean` job (`leanprover/lean-action` with `build-args: --wfail`).

`Formal/Elab/PreludeSchemes.lean` is generated from the checker:
`just formal-schemes` rewrites it, and a golden test of `cargo test -p flick-model`
(`flick-model/src/translate/lean.rs`) fails when it is stale.

## What is proved

- **The core** (`Formal/`, the top-level modules), with rows of services and errors that
  take variables, generalized and instantiated as variables of the other kinds: type soundness `T1` (Progress and
  Preservation over every reachable configuration), `T2a` (a pure computation makes only
  pure steps) and `T2b` (a runtime computation asks only the services and raises only the
  errors of its row), `lemma_3_1` and `T3` (generalization and the store typing), `T4` (T4.1, T4.2: live work reaches only live
  regions), `T4_3` (no service of an ended `provide` is used) and `T1_regions` (`T1` with
  `op_H`, without the blocked outcome of a step that needs a region that is not live or a
  service of an ended `provide`, which `T4` and `T4_3` rule out).
- **T5, the runtime contract** (`Formal/RT/`, `RT/T5.lean`): for any core and any
  interleaving, every reachable configuration of λRT satisfies R1–R4, R6 and R7, and R5 and
  R8 hold of the cause algebra, under the hypotheses on the libraries (`Hyp`) and `G11`.
- **T6, the elaboration** (`Formal/Elab/`), with runtime stages `R r e` whose rows take
  variables: `T6a` (`Elab/T6a.lean`), an elaborated term is
  typed in the target, with `T6a_default_instance` (`Elab/Defaults.lean`) for what an instance
  takes from a default, its default signature's dictionaries derived under its context, and `T6b` (`Elab/Coherence.lean`), two elaborations that differ only
  in the derivations of their dictionaries give the same result. Both are stated for an
  abstract target and instantiated on the core with regions and dictionary records as
  `T6a_core` (`Elab/CoreT6a.lean`) and `T6b_core` (`Elab/CoreRun.lean`), the latter with the
  run of the core itself.
- **Call sites** (`Formal/Elab/CallSite.lean`): `site_given`, a `CallSite` at a place that is
  no literal comes only from a given, and `site_caller`, a function under that given passes on
  its caller's place; (Site) is (Prim) at a literal place, so T6 covers it unchanged.
- **The hypotheses of T6 are satisfiable** (`Formal/Elab/Witness/`): one concrete program
  meets all of them, and `witness_T6a`, `witness_T6b` (`Witness/Main.lean`),
  `witness_T6b_example`, `witness_T6b_closed` and `witness_T6b_open` (`Witness/T6b.lean`)
  are T6 on it, the last three without premises of typing.

`#print axioms` of `T1`, `T2a`, `T2b`, `lemma_3_1`, `T3`, `lemma_4`, `lemma_6`, `T4`,
`T4_3`, `T1_regions`, `T5`, `T6a`, `T6a_core`, `T6a_close`, `T6b`, `T6b_core`,
`witness_T6b_example`, `witness_T6b_closed` and `witness_T6b_open` lists only Lean's standard `propext`,
`Classical.choice` and `Quot.sound`; the hypotheses of T5 (`Hyp`, `G11`), of T6(a)
(`Target`; for the core, `Real`) and of T6(b) (`Coherent`, `Sim`) are their parameters.

T4.3 is the premise G11 of T5 in the form [the answering provide must be live](../docs/formal/runtime.md#the-provide-liveness-premise-is-stronger-than-typing-gives) gives it: an `ask_S` answered by a `with`
frame of the region `q` of a `provide`, inherited or not, happens only while `q` is live.
Its Lean statement (`T43Main.lean`), the counterpart of `G11` in `Formal/RT/Hyp.lean`:

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
| row variables, the row an instance stands for | `Elem.rv`, `Ty.row`, `Ty.asRow`, `Row.Named` (`Syntax.lean`); `Ty.asRow_substT`, `Ty.reach_asRow` (`TyOps.lean`) |
| `regions.md` §1.2 atoms, capture sets, `τ^C`, `H ϱ`, `Owner ϱ`, `Scope ϱ` | `Atom`, `Caps`, `Elem`; `Ty.arrow`/`Ty.eff` with `C`, `Ty.caps`, `Ty.rg`, `Ty.hnd`, `Ty.owner`; `Row.scope` (`Syntax.lean`) |
| `regions.md` §2.1 `⌈τ⌉`; its commutation with substitution | `Ty.reach`; `Ty.reach_substT`, `Scheme.reach_substT` (`TyOps.lean`, `Reach.lean`) |
| `regions.md` §2.4 `Θ ⊢ Outlives C ϱ` | `Facts`, `Outl` (`Syntax.lean`); dropping facts that follow: `Typ.cut` (`Cut.lean`) |
| type substitution, `Gen` and instantiation | `Ty.substT`, `Ty.up`, `liftT`, `instT` (`TyOps.lean`); `Scheme.substT`, `Scheme.inst` (`SchemeOps.lean`) |
| `core.md` §1.3 terms, patterns; §3.1 run-time forms; `regions.md` §1.3, §3.1 | `Tm`, `Pat` (`Syntax.lean`) |
| what a term holds: free variables, run-time regions, locations (the premise of (Lam)/(Task)) | `Tm.Free`, `Tm.Has`, `held` (`Reach.lean`) |
| §1.3, §3.1 values | `Value`, `IsEnv` (`Typing.lean`) |
| `core.md` §1.5, `regions.md` §1.5 constants | `Const`, `Const.ty`, `Const.ok` (`Syntax.lean`) |
| §2.1 `⊑`, §2.2 `≤`, (≤-Cap) | `Stage.le`, `Le` (`Syntax.lean`, `Typing.lean`, `Order.lean`); `Le.reach` |
| `core.md` §2.3, §3.6 rules; `regions.md` §2.3–§2.5, §3.4 | `Typ` (`Rules.lean`) |
| (Sub) | admissible: `Typ.sub` (`SubAdm.lean`) |
| §2.4 patterns, `Exh`; (Match-Val), [Match-Val: a value needs only its own coverage](../docs/formal/core.md#the-residual-match-after-a-false-guard-fails-exh) | `PatTy`, `Shp`, `Exh`, `Covers` (`Typing.lean`) |
| (Try), (Handle) | `HPat`, `Catches`, `HCov`; `tryR`, `tryP`, `handle` |
| §3.6 `⊢ μ : Ψ`; §2.6 `Σ₀` | `StoreTy`, `RootTy` (`Rules.lean`) |
| `core.md` §3.2 contexts, `lookup(E, S)`; the frame `inside π E` | `Frame`, `plug`, `lookupE` (`Semantics.lean`) |
| `core.md` §3.3 `→ₚ`; §3.4–3.5 steps; `regions.md` §3.2–§3.3 | `PHead`, `RHead`, `Step`, `Steps` (`Semantics.lean`) |
| `regions.md` §3.1 `Λ`, children step at any time | `Rgn`, `Cfg`, `CStep`, `CSteps` (`Semantics.lean`) |
| lemma about `build` | `canonical_layer` (`CanonLayer.lean`) |
| L1 | `canonical_arrow`, `canonical_ref`, `canonical_eff`, `canonical_owner`, `canonical_hnd`, `canonical_env`, `canonical_errors`, `Typ.valueAny` (`CanonForms.lean`, `CanonEnv.lean`, `Canonical.lean`) |
| (Let-Gen), (Let-Mono) | `Typ.letGen`, `Typ.let_` (`Rules.lean`) |
| type substitution lemma | `Typ.substT`, with `Fits` (`TySubst.lean`, `SubstPat.lean`) |
| L2 | `Typ.subst`, `Typ.instantiate`, `SubOk.one` (`Substitution.lean`) |
| L3 | `ctx_typing` (`Lemma3.lean`); its `Refill` holds after any substitution that fits the term (`Context.lean`) |
| L4 without the store: a value holds no more than its type shows | `Typ.holds` (`Held.lean`) |
| §5.2 `reach_μ`; L4 | `Reaches`; `lemma_4`, `lemma_4_cell` (`Lemma4.lean`) |
| regions not yet created, renamed away; facts of the order of creation | `kill`, `Ty.YF`, `Tm.Below`, `Facts.age` (`Fresh.lean`, `Rules.lean`) |
| (Scope) and (Inside) in a run-time configuration | `RtCtx`, `scope_body`, `inside_entails` (`ScopeStep.lean`) |
| a configuration writes only regions that exist | `CfgBelow`, `Step.below` (`Below.lean`, `BelowStep.lean`) |
| L6 | `lemma_6` (`Lemma4.lean`); the invariant of a configuration: `CfgTy` (`Soundness.lean`) |
| §3.4 `⊢ Λ`: where a region may be written, the ancestors of a region | `Tm.Within`, `LocOk`, `Nest` (`Within.lean`, `Nest.lean`); kept by every step: `Nest.step`, `Nest.cstep` (`NestMain.lean`, `Regions.lean`) |
| L5 | `Nest.nested` (`NestClose.lean`); in every reached configuration: `reach_nest`, `Nest.live` (`Regions.lean`) |
| §2.7 obligations `{Provided {S} ϱ}` and `{Provided r ϱ}` over a row variable, uses of a service of a `provide` | `Elem.ob`, `Elem.obv`, `Elem.need`; `Row.obs` in `Const.ty` of `spawn`/`addFin` (`Syntax.lean`); `Row.withP`, `Row.disch`, `Row.needs`, `Stage.ObOut` in `Typ.with_`, `Typ.scope`, `Typ.inside`, `Typ.provideL`/`provideE` (`Rules.lean`) |
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
| a polymorphic use; a use of a row-polymorphic function at a row of services; a `_` arm that removes a row variable of errors; a `provide` of a layer whose outputs are a row variable; a scope that acquires and uses, as checks | `Examples.lean` |

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
`⌈υ⌉`: one substitution serves all kinds. A row is a set of elements, a name, an atom or
a row variable `rv a`: in a row of services the atom `ϱ` is `Scope ϱ`, in a row of errors
the atoms bound what the errors reach (`⌈Errors ε⌉`), so rows are substituted too. A row
variable instantiated by `row r` is the names, atoms and variables of `r` (`Ty.asRow`;
obligations and uses do not come in through a variable), one instantiated by a type
variable is that variable, by any other type the atoms the type reaches. Then the row a
type stands for commutes with substitution (`Ty.asRow_substT`), and `⌈τ⌉` is what `τ`
stands for as atoms and variables (`Ty.reach_asRow`), which keeps `⌈τ[σ]⌉ = ⌈τ⌉[σ]` and
the composition of substitutions with kinds unchecked. A substitution of a term with
run-time forms brings no names into rows (`NoNames`, part of `Fits`): the renaming
`kill`, shifts and the region of a new scope are such, and only source code is
instantiated by rows of names.

Types have no binders; a scheme `⟨k, Θ, τ⟩` binds the indices `0 … k-1` of `τ` and of its
facts `Θ`. Terms carry no types (Curry style), so a type substitution changes the judgment
and not the term; constants carry no instance either, (Const) picks it (`CArg`).
(Let-Gen) types its right-hand side with `Γ`, `Θ` and `Ψ` shifted by `k` (`Ty.up k`) and
the facts of the scheme assumed: the generalized variables are fresh by construction,
which is `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)` of `Gen` and L2. (Scope) does the same with one variable,
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

Rows are sets, `Elem → Prop`, with variables among their elements; equality of rows is
extensional, which is `≡` of §1.2. Every rule types its premises at the stage of its conclusion, and the
subsumption `≤` is folded into the rules whose result may be an arrow or an `Eff`; (Sub)
is then admissible (`Typ.sub`). With (Sub) this is equivalent to the `⊔` form of §2.3,
and inversion stays a plain `cases`. (≤-Cap) is a premise of `Le.arrow` and `Le.eff`:
`⌈τ⌉ ⊆ ⌈τ'⌉` of the whole value.

The premise of (Lam), (Task) and the fixpoint is `held Ψ Γ e ⊆ ⌈τ^C⌉`: `⌈Γ(y)⌉` of each
free `y`, and also the run-time regions the body writes down and the cell types of its
locations ([Lam and Task count runtime forms](../docs/formal/regions.md#lam-and-task-count-runtime-forms)). The substitution lemma keeps it because a substituted value holds no
more than its scheme shows (`Typ.holds`, the part of L4 without the store).

A thread steps in `⟨Λ, μ, e⟩`. `Λ` is a list of regions: live or closed, children and
finalizers, both computations. A configuration is a main thread with `Λ` and `μ`, and
`CStep` steps the main thread or a child of a live region.

## Assumptions and simplifications

Each item narrows `core.md` or `regions.md` unless it says otherwise.

1. **Row variables are variables of the scheme.** A scheme abstracts a row variable as
   any other variable, and (Var) instantiates it by a row (`Ty.row`); constants are typed
   at any rows (`CArg`). Kinds are not checked. A row variable stands for names, atoms and
   variables, never for obligations or uses; the rows that must be named are the outputs of
   `Layer.merge`, and those of `provide` when its body has obligations (`Row.Named`, items 8
   and 21). `⌈Errors ε⌉` counts the row
   variables of `ε`, as `regions.md` §2.1 and the checker do (`regions/mod.rs`
   `error_row_captures`): an instance shows what its errors reach, and `⌈·⌉` must commute
   with it, so a closure over an error of an unknown row holds that row. What still keeps a
   region out of an error is (Err-Decl) and (Err-Raise) of `regions.md` §2.5 (`FLICK-E005`),
   of which the model needs only the second (item 18). Nominal types have parameters
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
   `True`/`False` patterns are literal patterns. The only primitive on them is `Int.eq`, the
   constant `intEq : Int →P Int →P Bool` with a pure step on two literals.
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
   what it catches (`Catches`): an `x`/`_` arm all of `ε_b`, its row variables and atoms
   too, a constructor arm the name of its error type. (Handle) uses the value form of the coverage
   premise ([Handle covers only the raised error](../docs/formal/core.md#handle-required-the-whole-error-row-after-a-failed-branch)); what the raised row has besides names is removed by its
   remaining arms or passes to the errors it lets through (`H ∪ ε'`): the arm that removed a row
   variable is an `x`/`_` arm, which matches every value, so no later arm and no re-raise needs
   it.
8. **Environments.** `Env ρ` means "binds exactly `ρ`", as (Env), (Bind) and (Join)
   give it: T4.3 needs that a `with` frame answers `S` exactly when its row has `S`. `with θ e` and `econs` require
   `θ` and the bound values to be values. The disjointness of `Layer.merge` is kept
   (`Const.ok`) though soundness does not need it. The outputs `Layer.merge` joins are
   named, without row variables (`Row.Named`): the checker compares the names it sees, and
   instantiating a row variable may make them overlap, so the condition is not kept by a
   substitution. The services a `provide` gives may be a row variable, which its body may
   require (`core.md`, [`provide` and output row variables](../docs/formal/core.md#provide-discharges-only-the-listed-outputs-of-a-layer-not-a-row-variable); `with_layer` in `Examples.lean`).
9. **Allocation is deterministic:** `Ref.make v` takes `ℓ = μ.length`, `scope` takes
   `π = Λ.length`.
10. **Programs** are closed terms typed in the empty store with no facts; modules
    (`core.md` §2.6, path 4 of `regions.md` §4) are not modeled. `Σ₀` and the bodies of the
    globals of λFlick⁺ are a parameter `root : Roots` with `RootTy`; G3 is the hypothesis of `T1` that `Σ₀` covers the services of the program's
    stage. Progress reports an `ask_S` that neither a `with` frame nor `Σ₀` answers
    (`AskStuck`) instead of assuming G3.
11. **L3** is stated as `ERel`: the hole's services are within those of the context, of
    its `with` frames and the `Scope π` of its `inside` frames, its errors grow only under
    a `try` frame, and a pure context has a pure hole. Under `K` right-hand sides of
    (Let-Gen) the hole is typed `K` variables deep, with `Ψ` and `Θ` shifted and the facts
    of the schemes assumed, and is pure.
12. **Steps carry labels** (`Lbl`): pure, runtime, or `root S` for an `ask_S` answered by
    `Σ₀`; T2(b) is about the `root` labels. `todo → die` is labelled pure, as T2(a) allows
    `die`; `bind_S` and `join` steps are pure ([bind_S and join steps are pure](../docs/formal/core.md#bind_s-and-join-steps-are-pure)).
13. **Cell types may be open** ([store types are ungeneralized monotypes](../docs/formal/core.md#a-store-typing-of-closed-types-is-not-preserved)). `Ψ(ℓ)` is a monotype that may mention free type
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
    (Proj) are (App) and (Match) at an instance ([Con field premise ignores field qualifier](../docs/formal/regions.md#the-field-premise-of-con-ignores-the-field-qualifier)).
16. **Capabilities and constants.** `H ϱ̄ τ̄` is one family `hnd ϱ` with one region;
    `open_H` takes the owner of its region and nothing else (G6: the region is explicit)
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
    an element `ob S ϱ` of a row of services, and `Provided r ϱ` over a row variable `r`
    an element `obv r ϱ`, which a substitution turns into the obligations of what `r` stands
    for; `Fiber.spawn` and `Scope.add_finalizer` put one for each service and each row
    variable of the work into the stage of the call (`Row.obs`). A
    `provide` in a program binds no region variable: an obligation about a region
    created in its body is discharged at the end of that region (`Row.disch`, the second
    case of §2.7, `S` or `r` coming from outside it), and one that reaches a `provide`
    giving `S` is rejected, which is the first case. Which services an instance of a row
    variable gives is unknown there, so `provide` asks more (`Stage.ObOut`): an obligation
    `ob S ϱ` passes only outputs without row variables, and `obv r ϱ` only outputs that give
    no service. An obligation not discharged stays in the row
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

Mechanizing the core found these errors in the calculus; each is fixed in the document
that states the rule.

- [a partial constructor application is a value](../docs/formal/core.md#a-partial-constructor-application-is-a-value): a partially applied constructor is a value, and L1 for an arrow type
  has to list it (`Value.ctorApp`, `canonical_arrow`).
- [bind_S and join steps are pure](../docs/formal/core.md#bind_s-and-join-steps-are-pure): `bind_S` and `join` steps were runtime steps while their rules are
  pure, so T2(a) failed for a bare `bind_S v`; they are pure steps.
- [Handle covers only the raised error](../docs/formal/core.md#handle-required-the-whole-error-row-after-a-failed-branch): (Handle) asked for the whole row `ε_b ⊆ H + ε'`, which a failed arm
  breaks; it asks for the error of the value only, as `flick-model` does.
- [store types are ungeneralized monotypes](../docs/formal/core.md#a-store-typing-of-closed-types-is-not-preserved): closed cell types and a fixed program type cannot both hold; cell types
  are monotypes with possibly free variables, and `Gen` excludes `ftv(Ψ)`.
- [Spawn bounds everything the task holds](../docs/formal/regions.md#spawn-bounds-everything-the-task-holds): (Spawn) bounded only the hidden qualifier `C` of the child; the child
  also holds what its result shows. The premise is `Outlives (C ∪ ⌈a⌉) ϱ` (`Const.ok`);
  the checker checks it (`regions.md` §6).
- [Lam and Task count runtime forms](../docs/formal/regions.md#lam-and-task-count-runtime-forms): the premise of (Lam)/(Task) must count the run-time regions and cell
  types written in the body, or L4 fails for `λx. (λy. ()) (cap π n)` (`held`,
  `regions.md` §6).

## λRT and T5

`Formal/RT/` is a model of its own: it imports nothing from the core above, and the core
is a parameter of it.

| `runtime.md` | Lean |
| - | - |
| §1.1 reasons, causes | `Reason`, `Cause` (`RT/Cause.lean`) |
| §2.1 `nub`, `⊕`, `⧺` | `nub`, `comb`, `++` (`RT/Cause.lean`) |
| §2.2 normal form, laws, `head`; Go's tree | `NF`, `head`, `head_comb`, `CTree` (`RT/Cause.lean`) |
| R8 | `R8` (`RT/Cause.lean`) |
| §2.3 `dies`, `elect`, `recover` | `dies`, `elect`, `recover`, `recover_die`, `head_elect` (`RT/Recover.lean`) |
| §2.4 I_ε and what keeps it | `Iε`, `Iε_comb`, `Iε_append`, `Iε_close`, `Iε_keep` (`RT/Recover.lean`) |
| R5 | `R5` (`RT/Recover.lean`) |
| §1.2–§1.4 fibers, owners, configurations, run-time forms | `Fiber`, `Code`, `Frame`, `Owner`, `Phase`, `Conf` (`RT/Config.lean`) |
| the core, a parameter | `Core`, `Act` (`RT/Config.lean`) |
| what the libraries decide | `Lib` (`RT/Config.lean`) |
| §3.1–§3.6 steps | `Local` (`RT/Local.lean`), `Step` (`RT/Step.lean`), `Steps`, `Reach` (`RT/Reach.lean`) |
| §3.3 C0–C6 | `Step.close`, `.hookCode`, `.hookStop`, `.stopDone`, `.kids`, `.kidsDone`, `.fins`, `.finCode`, `.finObs`, `.finObsLast`, `.closed`, `.absorbH`, `.absorbF` |
| §5 G7(a)–(g) but (c), G8, G9 | `Hyp` (`RT/Hyp.lean`) |
| invariants of reachable configurations | `Inv`, `Inv.step`, `Inv.reach` (`RT/Inv.lean`, `RT/Reach.lean`) |
| I1 | `Inv.stJ` and `Good` of `close`, `fins`, `closed` (`RT/Inv.lean`) |
| I2 | `Inv.empty` and `Good` of `spawn`, `fin`, `gkid`, `branch` |
| I3 | `Inv.spawnJ` |
| I5 | `Iε_comb`, `Iε_append`, `Iε_close`, `Iε_keep` (`RT/Recover.lean`) |
| R1, R2, R3, R4 | `R1`–`R4`, `Inv.r1`–`Inv.r4`, `Inv.r4_closed`, `parResult_keeps` (`RT/Props.lean`) |
| the sketch of R3: who waits for a running fiber | `Guard`, `Inv.noRun` (`RT/Inv.lean`, `RT/NoRun.lean`) |
| §3.5 memo maps, `build`, observers | `Memo`, `Ent`, `lookup`, `Step.provide`, `.provHit`, `.provBuild`, `.buildOk`, `.buildFail`, `.finObs`, `.finObsLast` |
| I6 | `Inv6.obsIn`, `Good6` of `release` (`RT/Layers.lean`) |
| R6; its premise from [provide services must not outlive their layer](../docs/formal/runtime.md#a-provide-service-outlives-the-release-of-its-layer) | `R6`, `Good6`, `GoodUse` (`RT/Layers.lean`, `RT/T5.lean`); `G11` (`RT/Hyp.lean`) |
| §3.7, R7 | `Step.modify`, `cell`, `R7` (`RT/RefLin.lean`) |
| T5 | `T5` (`RT/T5.lean`) |

Simplifications of the cause algebra:

1. `≈` is equality of reasons (`DecidableEq V`). A thrown `Error`, compared by identity in
   Effect, is a value of `V` that carries its identity.
2. A row `ε` and `· ⊢ v : Errors ε` are a predicate `Has : V → Prop`; the premise of (Try)
   is `Covers`. R5 is about one `recover`, as the sketch in §6 is; that the core's (Try)
   gives `Covers` is `T1` of the core development.
3. `pretty` and `report` (§2.5) are not defined: they read `Cause V`, a list, so the third
   part of R8 holds by their type. (Seq=Par) is `CTree.reasons`, where both node kinds are `⊕`.

Simplifications of λRT:

1. **The core is a parameter.** A fiber's code is a term of any type with a labelled
   transition relation `Core.step`; a label says what the code asks of the runtime, and
   `Core.plug` hands the result back. Every theorem holds for every core.
2. **Handles can be forged.** An action names owners, fibers and groups by number, and the
   rules check only their states (`spawn` needs `open`, and so on). So I4, which needs G6,
   is not modelled; R1–R4 do not use it. R3 is proved from `Guard` instead: a child is in
   `K`, a watcher in `W`, a group's child in an open group or one its owner's closing waits
   for, a branch in a `par` frame of a running fiber whose current owner, below that frame,
   started it (`Inv.noRun` is the induction of the sketch).
3. **No mask.** An interruption is delivered at any point of user code and of the waits that
   are interruption points (`Code.IntPoint`); a closing, a watcher's wait and a combinator's
   wait are not. That is more interleavings than §3.2, so safety carries over; G7(c) is about
   when an interruption arrives and is not needed. Requests to stop (Int-Req, C2, a group's
   hook, `parallel` stopping its branches) arrive at any time (`Step.req`): that they are
   sent is liveness.
4. **The closing** is run by the fiber that ran the body, through a `closing π` frame; its
   phase is in the owner (`Phase`). `hooks true` is both the hooks added since C1 ([hooks added during closing still run](../docs/formal/runtime.md#a-closing-hook-added-after-c1-does-not-run)) and
   C4: it runs hooks until `H = ∅` and then waits for `P = ∅` ([hooks added while foreign calls settle run](../docs/formal/runtime.md#a-hook-added-during-c4-does-not-run)).
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
    `release`). (c) No service is used after its build is released (`GoodUse`), under G11.
    `G11` is about the answering entry only: `Act.use π` names the `provide` owner whose
    environment entry answers the `ask_S`, and at such a step `π` is live. A child may carry
    an entry of a closed `provide` that it never asks ([the answering provide must be live](../docs/formal/runtime.md#the-provide-liveness-premise-is-stronger-than-typing-gives)). The core development gives it as `T4_3`: the
    region `q` of the `with` frame that `lookupE E N` finds for an `ask_N` is `Live`.
    A layer's build code runs in the providing fiber with the area `σ` as current owner; a
    failed build closes `σ` and makes the `provide` fail.
12. **R7** is `Hist GoodRef`: each `Ref.modify ℓ` read the value of the latest `make` or
    `modify` of `ℓ` (`cell`), and the memory is the replay of the journal. Allocation is
    deterministic, as in the core development.

## Elaboration and T6

`Formal/Elab/` proves T6 of [`docs/formal/elaboration.md`](../docs/formal/elaboration.md).
Its abstract part imports nothing from the core: λFlick⁺ is the structure `Target`, whose
typing, laws and run are fields, and T6(a) and T6(b) hold for every `Target`. The `Core*`
modules instantiate `Target` with the core of `Formal/`, so T6 goes through the core with
regions ([elaboration preserves captures and regions](../docs/formal/elaboration.md#t6a-without-captures-does-not-carry-over-to-the-core-with-regions)): λFlick⁺ is that core plus nominal dictionary records
`D_C ā` with `∀`-fields ([dictionaries need polymorphic fields](../docs/formal/elaboration.md#dictionaries-need-polymorphic-fields-and-higher-rank-parameters)), and captures enter the elaboration source
and `Target`. See [The core as the target](#the-core-as-the-target).

| `elaboration.md` | Lean |
| - | - |
| §1.1 classes, instances; §2.1 superclass slots | `Prog`, `Inst`, `Prog.super` (`Elab/Syntax.lean`) |
| §2.2 dictionary terms `d` | `Dict`, `apps`, `Dict.subst` (`Elab/Syntax.lean`) |
| §2.3 `Δ*`, (Given), (Prim), (Struct), (Inst) | `Clo`, `Ent` (`Elab/Syntax.lean`) |
| §4.1 K1–K8 | `Coherent` (`Elab/Hyp.lean`) |
| §4.2 `≈` | `Eqv` (`Elab/Syntax.lean`) |
| Lemma 6.1, Lemma 6.2, T6(b) | `lemma_6_1`, `lemma_6_2`, `T6b` (`Elab/Coherence.lean`) |
| §1.1 method schemes | `Scheme`, `Prog.methods` (`Elab/Syntax.lean`) |
| §2.2, §2.4 `Γ; Δ ⊢ e : τ ! φ ⇝ e'`: (Var-Dict), (Method), (Let-Sig-Dict) | `Exp`, `Tm`, `Elab` (`Elab/Source.lean`) |
| §0.1, §2.1 λFlick⁺, `D_Q`, `⟦σ⟧`, `⊢⁺` | `TTy`, `TSch`, `Scheme.tr`, `Target` (`Elab/Target.lean`) |
| `Δ ⊩ Q ⇝ d ⇒ ⟦Δ⟧ ⊢⁺ d : D_Q`; T6(a) | `ent_typed`, `T6a` (`Elab/T6a.lean`) |
| §3.1 `do` | `Stmt`, `Sugar.desugarDo`, `DoOk`, `do_elab`, `T6a_do` (`Elab/Sugar.lean`, `Elab/DoNotation.lean`) |
| §3.2 `for`, §3.3 `?`, §3.4 blocks | `Clause`, `Sugar.gen`, `Sugar.loop`, `GenOk`, `LoopOk`, `forSeq_elab`, `forList_elab`, `loop_elab`, `tryResult_elab`, `tryOption_elab`, `seqUnit_elab`, `T6a_sugar` (`Elab/Sugar.lean`, `Elab/Comprehension.lean`) |
| Prelude methods and constants of Γ₀ the sugar uses | `Sugar` (`Elab/Sugar.lean`); the schemes of the exports of `Prelude`, `Seq`, `Option`, `Result` and `List` generated from the checker, those the model cannot write listed as skipped with a reason, none today (`Elab/PreludeSchemes.lean`, [Prelude scheme generator with skip list](../docs/formal/elaboration.md#the-scheme-generator-translated-only-the-sugar-names)) |
| "≈ gives the same result" | the hypothesis `Sim` (`Elab/Hyp.lean`); from the steps of a target `Target.sim`, then `T6b_target` (`Elab/Run.lean`) |
| §0.1 `⟦·⟧` into the core with regions: types, stages with rows, schemes | `Names`, `trT`, `trTT`, `trD`, `trS`, `trStage`, `trT_arr`, `Stage.qual`, `trT_subst`, `trStage_subst`, `trT_reach` (`Elab/CoreTy.lean`) |
| §0.1 elaborated terms in the core | `Tr`, `TrD`, `Corr`, `Tr.free`, `corr_held` (`Elab/CoreTr.lean`) |
| §0.1 λFlick⁺ as `Target`, the declarations of the program in `Σ` | `Real`, `Has`, the laws `has_*` (`Elab/CoreLaws.lean`, `Elab/CoreDict.lean`), `typingTarget` (`Elab/CoreT6a.lean`) |
| T6(a) for the core | `T6a_core`, `T6a_closed`, `T6a_do_core`, `T6a_sugar_core` (`Elab/CoreT6a.lean`) |
| an elaboration with its premises closed by `θ`, `ρ` ([dictionaries typed closed in the run target](../docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context)) | `T6a_close`, `ent_close`, `Tm.dsubst`, `Tm.Skel`, `fill_dsubst` (`Elab/Close.lean`); `lift_inst`, `up_lift`, `heldS_subst`, `Cn.Ground`, `Prog.ConstsClosed` (`Elab/CloseTy.lean`) |
| the hypotheses of T6 hold together: a witness program | `P`, `S`, `N`, `Sg`, `root`, `coherent`, `real`, `rootTy`, `realRun`, `witness_T6a`, `witness_T6a_do`, `witness_T6b`, `witness_T6b_example`, `witness_T6b_closed`, `witness_T6b_open` (`Elab/Witness/`, `T6b.lean`), `consts_closed` (`Consts.lean`); the bodies `seqFlatMap`, `seqEach`, `seqToList`, `iterL` (`Seq.lean`), `uncB`, `frBody`, `eqRec` (`Unc.lean`); arms for every constructor are exhaustive, `exh_ctors` (`Elab/Exh.lean`) |
| the lazy `Monad Eff` in the core ([Prelude schemes translated with CRow qualifiers](../docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes)), `Eff q b` with the field `eff ∅ ∅ b {q}` | `effT`, `effField`, `effPure`, `effMap2`, `effBind`, `typ_effPure_tr`, `typ_effMap2_tr`, `typ_effBind_tr` (`Elab/LazyEff.lean`) |
| the run of dictionaries in the core: closed derivations evaluate, `≈` keeps values | `RealRun`, `ent_eval`, `eqv_sem` (`Elab/CoreDictSem.lean`); `Eval`, `SemEq` (`Elab/CoreEval.lean`); `PStep`, `decomp_eq` (`Decomp.lean`) |
| λFlick⁺ as `Target` with the run of the core: `Rel`, `load_rel`, `step_rel`, `final_rel`; T6(b) for the core | `Cg`, `DP`, `CfgR` (`Elab/CoreCg*.lean`, `Elab/CoreRel.lean`); `phead_sim`, `rhead_sim`, `thread_sim`, `cstep_sim`, `final_sim` (`Elab/CoreHead.lean`, `Elab/CoreStep.lean`, `Elab/CoreSim.lean`); `DSim`, `eqv_dsim`, `DTyp`, `dsim_cg` (`Elab/CoreLoad.lean`); `trE`, `RHas`, `runTarget`, `T6b_core` (`Elab/CoreRun.lean`) |

Simplifications:

1. Constraint types are first-order terms with binary application. K1 is semantic: heads
   that some substitutions make equal belong to one instance, which is what a failed
   first-order unification says.
2. (Improve) gives no dictionary, so `Ent` starts from improved constraints; K2 and K3 only
   make improvement order-independent and are not fields. K4 gives K1 for the whole program,
   which `Coherent` assumes directly. K8 makes premise names fresh; premises here are
   numbered.
3. K5 is a well-founded transitive order `lt` with the instance context, a superclass and
   a record's field below the constraint, plus "the variables of the context are in the head".
4. (Struct) and (Prim) are black boxes: `fields` and `prims`, which commute with
   substitution; K6 keeps the compiler's classes apart from the program's instances.
5. An expression is a skeleton with dictionary holes all under one `Δ`; `Sim` says filling
   them with `≈` dictionaries gives the same result. A hole under a `λδ̄` of (Let-Sig-Dict)
   sees a longer `Δ`; T6(b) over two whole `Elab` derivations, which may also pick different
   types, is not stated.
6. The source of T6(a) has variables, methods, constants of Γ₀ (`Prog.consts`, with the
   dictionaries of their schemes, as (Var-Dict)), application, `λ` over a monotype, `let`
   without generalization, `let` with a signature, `if` and a `match` on `Option`; no
   constructors, other `match` or higher-rank parameters. Its types are the first-order
   `Ty` with the arrow `a →φ^q b` (`Ty.arr`, [source arrow is a separate type form](../docs/formal/elaboration.md#the-source-arrow-was-a-reserved-constructor)), whose qualifier is a capture row
   written as a type (a variable, `cnil`, `cun`); `Bool` and `Option` are reserved
   constructors (`boolTy`, `optTy`). (Lam) and (Let-Sig-Dict) show what a closure holds,
   `heldS` ([elaboration preserves captures and regions](../docs/formal/elaboration.md#t6a-without-captures-does-not-carry-over-to-the-core-with-regions), [dictionary arrows carry the let qualifier](../docs/formal/elaboration.md#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds)); qualifiers match exactly, there is no (≤-Cap). (Sub) is kept for stages only: a value has any stage, an application,
   a `let` and a `match` run at one stage `φ`, the arrow applied and the right side of a
   `let` at `ψ ⊑ φ` ([constrained let keeps the right-hand side effect](../docs/formal/elaboration.md#let-sig-dict-lost-the-effect-of-the-right-hand-side)). A term at `P` elaborates at any stage (`Elab.anyStage`), and
   renaming term variables keeps an elaboration (`Elab.rename`, weakening `Elab.shift`).
7. `Target` is an interface, not λFlick⁺: its types are monotypes, stage arrows and `D_Q`,
   a context entry is `∀ᾱ. t`, and a `∀`-field of `D_Q` shows only through the law `meth`
   (projection with instantiation). Constants of Γ₀ are typed by their schemes (`const`).
   Arrows carry qualifiers: `lam` and `lamD` have the held premises of the source, and
   the arrows of methods, constants, instances and `struct_C` hold nothing (`cnil`), so the
   `q` of their schemes is never read.
   The types of instances (`inst`: `I : ∀β̄. D_ctx →P
   D_head`, by (Instance) of §2.5 and K7) and of (Struct)/(Prim) dictionaries are laws.
   `lamD l` binds `l` premises at once, the `i`-th argument being the premise `i`;
   λFlick⁺ curries it. The core instance proves each law from its own typing
   ([below](#the-core-instance-of-target)).
8. The run of the target is `Target.St`, `load`, `Step` and `obs`; a skeleton is a `Tm` with
   holes `hole i` and the type and stage of its fillings, and its result is the set of observations of the terminating runs of the
   filled term (divergence is not compared). `Sim` follows by induction on the run from
   three fields, which the core instance proves with its relation `Rel`
   ([below](#the-run-rel-and-sim)):
   `load_rel` (closed terms typed at one type that differ by `≈` in dictionary positions,
   `TmEqv`, load to related states), `step_rel` (a step of one side is matched by zero or more steps of the other:
   evaluating `(I d̄).$slotᵢ` and `sᵢ[δ̄ := d̄]` takes different numbers of steps) and
   `final_rel` (a result of one side is reached by the other). Only typed dictionaries
   evaluate, so `Sim` and `T6b` speak of fillings with `Exprs.Ok`, for a target the typing
   of the filled skeleton, and `T6b_target` takes it for both fillings.
9. The sugar (§3) desugars into `Exp`; `Sugar` names the Prelude methods (`flat_map`,
   `map2`, `pure`, `nothing`, `iter`) and the constants of Γ₀ with their schemes, as
   hypotheses on the program. The schemes of the Prelude names are generated from the
   checker (`PreludeSchemes.lean`), with the rows of their arrows: `->{r; e}` is the stage
   `R r e` and its row variables are binders of the scheme (`traverse`, `Seq.each_effect`). A
   row is a type: a variable, `cnil`, the row `{t}` of one service or error type `t` with
   constructor `c` (`rnm c t`) or a union `ρ₁ + ρ₂` (`run`), so `timeout` is at
   `R r ({TimeoutError} + e)`; a row may be the argument of a type (`FieldCodec (rd + r) a`,
   `Exit e a`), which the model writes as data. `pair`, `unc n` and `each_P` have no Prelude name and are
   written in `Sugar.lean`. `from_result : ∀e a. Result (Errors e) a →R⟨∅; e⟩ a` is the
   generated scheme, and `e?` takes it at the row of errors of `e` (`tryResult_elab`, at the
   stage `R⟨∅; ε⟩`): its `raise` of `Errors e` adds the atom `e` to the row, which the stage
   gives (simplification 10). Simplification 15 extends to the Prelude ([Prelude schemes translated with CRow qualifiers](../docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes)): a type with
   hidden storage (`Eff`, `Seq`) and the variable of a class over containers take the
   qualifier of the value as a first argument of kind `CRow`, `(T ā)^q ↦ T q ā` (`appQ`), and
   data without storage drops it. That argument is invariant, so a qualifier the caller
   supplies is a variable of the scheme, a result shows what the arguments it holds reach
   (`flat_map : (a →^c m e b) →P (m d a →^(c ∪ e) m ((c ∪ e) ∪ (d ∪ a)) b)`, `bindTy`), and
   a block of `do` or `for … yield` has the row its desugaring shows (`∃q` in `do_elab`,
   `gen_elab`, `T6a_do`). A lambda of the sugar takes as its qualifier a row that covers
   its context (`ctxRow`). The checker's rules are the judgements `DoOk`, `GenOk` and
   `LoopOk`; `?` and `s; rest` are premises of their lemmas. Patterns are variables (and `_`):
   a refutable pattern needs `match`, which the source lacks. The values of a group
   `p₁ <- e₁ and … and pₙ <- eₙ` form a left-nested tuple built by `map2 pair`, and
   `λ(p₁, …, pₙ). e` is `unc n (λp₁ … pₙ. e)` with a constant `unc n` per arity; the checker
   uses flat tuples with the same curried patterns. The clauses of `for … yield` run at `P`
   (Sync calls are not modelled) and the loop of `for … do` at its stage, with the constant
   `Seq.each_P` at `P` and the one `Seq.each_effect : ∀c d a r e. …` at every `R r e`, used at
   the rows of the loop ([for-do loops elaborate to Seq.each per stage](../docs/formal/elaboration.md#the-loop-rule-l-was-missing)). Each source of `for` goes through `iter` ([for sources elaborate through iter](../docs/formal/elaboration.md#g-elaborated-the-for-source-without-iter)). Only the
   last step of `?` on an `Option` is modelled, `let y = e? in b` ⇝ `match`; the A-normal
   form before it is not.
10. The core instance. `⟦R r e⟧ = rt ⟦r⟧ (⟦e⟧ ∪ ⌈⟦e⟧⌉ ∪ ε)` with `ε` every name (`allNames`): a row
    of the source is a type, and `⟦r⟧` is what `⟦r⟧` stands for as a row (`Ty.asRow`): a row
    variable `var n` is the row variable `n`, `cnil` the empty row, `rnm c t` the name `N.con c`
    with the atoms of `⌈⟦t⟧⌉` (`Row.err`, what an error of `t` reaches), `run r r'` the union of
    what `⟦r⟧` and `⟦r'⟧` stand for. The atoms `⌈⟦e⟧⌉` bound what the errors of `e` reach
    (`errRow`): a raised `Errors e`, data of the nominal type `Errors`, reaches the variable
    `e`. Every error name stays in the row, so a runtime body may raise any error type that
    reaches only what `e` reaches. The source compares stages
    by equality and its `⌈·⌉` counts the variables of the rows of an arrow, so the qualifier
    of a translated `R r e` arrow counts them too (`Stage.qual`); a `P` arrow translates as
    before (`trT_arr`). A substitution acts on the rows of a stage, and `T6a_close` closes the
    stage with the types (`φθ`). `Bool` is `base bool`, `Int` (`con 4`) is `base int`, other
    constructors are ADTs named by `Names.con`. `Real` is a hypothesis on the core's signature, not on the
    bodies of the globals (`RootTy`, part of the run). A class has a record type for each number of
    parameters (`Names.cls C n`), because the law `slot` holds for any constraint `q`;
    `Prog.super` pads missing parameters with `con 0`, and so does the slot's field. A method
    field binds the method's own variables first (`rot`) and needs the method scheme closed.
    `struct_C` and `prim_p` are families over the record type and the constraint, so their
    dictionary terms carry them, `Dict.strct C τ` and `Dict.prim p q`, and the translation
    `Tr` picks the global `N.strct C τ` (`N.prim p q`) from the term ([structural dictionaries are type-indexed families](../docs/formal/elaboration.md#structural-and-primitive-dictionaries-are-families-indexed-by-type), [structural dictionary terms carry their type](../docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type)). The laws
    `strct` and `prim` type them at any instance `θ` of the type, `struct_C^τ : D_{C τ₁θ} →P …
    →P D_{C τθ}` and `prim_q : D_{qθ}`: the global has a closed scheme and the run erases the
    type, and closing premises by types needs it ([structural dictionaries typed at any type instance](../docs/formal/elaboration.md#structural-dictionaries-were-typed-only-at-their-own-record-type)). `if` is `match c with true → a | _ → b`, and `match e with None → n |
    Some y → s` is `match e with Some y → s | _ → n`, exhaustive by `_` without declaring
    `None`. `typingTarget` has an empty run; `runTarget` has the run of the core,
    and its `Has` adds to the core's typing the typing `DTyp` of the dictionaries of the term
    by the laws (`RHas`, [dictionaries typed closed in the run target](../docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context)).
11. The hypotheses are satisfiable (`Elab/Witness/`): one program meets `Real`, `RealRun`,
    `RootTy`, `Coherent` and `Sugar` at once, with `Eq` on `Int` and structural on a record,
    the lazy `Monad Eff` ([Prelude schemes translated with CRow qualifiers](../docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes)), an eager `Option` monad `OptQ q a` and `Iterable` of
    `ListQ q a` (data without storage gets a phantom `CRow` parameter as an instance of a class
    over containers). Every global has a body typed by the rules of the core, none is `todo`;
    the bodies are honest but narrower than the checker's in two places. `Seq q a` is an
    eager list `SNil | SCons a (Seq q a)` with a phantom `q`: `Seq.flat_map`, `each_φ`,
    `to_list` and `iter` are fixpoints over it, and the laziness of the checker's `Seq` is not
    modelled (T6 needs only the types of [Prelude schemes translated with CRow qualifiers](../docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes), and the witness has no infinite or effectful
    sequence). `Rec` has one field, so the `&&` of the structural `eq` over its fields is one
    comparison by the slot dictionary `D_{Eq Int}`. `eq` on `Int` is the constant `intEq`, and `unc n` is
    built by a Lean function of `n` and typed by induction. A match has one arm per
    constructor and no `_` (`exh_ctors`, `Exh.lean`), also in the lazy `Monad Eff`.


### The core as the target

#### Captures in `Elab` and `Target`

- **Types** (`Elab/Syntax.lean`). `Ty` has `arr (φ : Stage) (a b q : Ty)`, `cnil` and
  `cun (q q' : Ty)` ([source arrow is a separate type form](../docs/formal/elaboration.md#the-source-arrow-was-a-reserved-constructor)); `Stage` is `P` or `R r e` with the rows
  written as types, mutual with `Ty` (`Ty.ind` is its induction): a row of services or errors
  is a variable, `cnil`, a named element `rnm c t` or a union `run r r'`, compared by equality. The qualifier `q` is a capture row written
  as a type: a variable, `cnil`, or a union. Substitution is structural, so `DecidableEq`,
  `Ty.agree_of_eq` and K1 work without semantic sets. `Ty.reach` is the occurrence of a
  variable: `⌈var n⌉ = {n}`, `⌈con⌉ = ⌈cnil⌉ = ∅`, `⌈rnm c t⌉ = ⌈t⌉`, `⌈app⌉ = ⌈cun⌉ = ⌈run⌉ = ∪` and
  `⌈arr φ a b q⌉ = ⌈a⌉ ∪ ⌈b⌉ ∪ ⌈q⌉` with the variables of the rows of `φ`. Capture and row variables are ordinary variables, so
  (Var), (Method) and (Const) instantiate them through `ts` with no new rule. Schemes
  have no `Outlives` facts: `⟦·⟧` drops `Fiber.Outlives` (§2.1).
- **Schemes.** `Scheme` has `q : Ty`, the qualifier of its dictionary arrows
  ([dictionary arrows carry the let qualifier](../docs/formal/elaboration.md#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds)); `Scheme.subst` lifts `q` like `ty`. The `q` of methods and of
  `Prog.consts` is never read: the laws `meth` and `const` use `cnil`, so `Prog` needs no
  well-formedness hypothesis for it. `Scheme.reach` is the reach of `Scheme.tr s` below its
  `k` binders, so it equals the core's `Scheme.reach` of the translation.
- **Judgement** (`Elab/Source.lean`). `Exp.Free` and `Tm.Free` are the free term variables,
  and `heldS Γ e n := ∃ x s, e.Free x ∧ Γ[x]? = some s ∧ s.reach n`. `Δ` adds nothing,
  because `⌈D_Q⌉ = ∅`.
  - (Lam) has the premise `heldS Γ (.lam b) ⊆ ⌈arr φ a t q⌉`.
  - (Let-Sig-Dict) has the premise `σ.ctx ≠ [] → heldS (Γ↑σ.k) e ⊆ ⌈σ.ty⌉ ∪ ⌈σ.q⌉`.
  - There is no (≤-Cap) in the source: qualifiers match exactly. This is a simplification;
    T6(a) does not need completeness.
  - Elaboration adds no term variables (`Elab.free`: `Elab … e' → e'.Free x → e.Free x`),
    so the held premises of the source give those of the target (`heldT_of`), and
    renaming keeps them (`heldS_rename`, `Weaken.lean`).
- **`Target`.** `TTy.arr φ a b q`. `dicts q qs t` puts `q` on every dictionary arrow, and
  `Scheme.tr s = ⟨s.k, dicts s.q s.ctx (.mono s.ty)⟩`.
  - The `lam` law has the held premise over `Tm.Free`.
  - The `lamD` law has the premise `qs ≠ [] → heldT Γ e ⊆ ⌈t⌉ ∪ ⌈q⌉`.
  - `meth`, `const`, `inst`, `strct` and `prim` use `q = cnil`.
  - The `Rel` fields take typing premises, because the run of the core relates closed typed
    terms. `load_rel` takes `Has [] [] e t φ` and `Has [] [] e' t φ`. `Exprs` has `Ok`, and
    `Sim` and `T6b` speak only of fillings with `Ok`. A skeleton of `Target.exprs` is
    `Tm × TTy × Stage`, `Ok` is the typing of the filled skeleton, and `T6b_target`
    takes it for both fillings. `step_rel` and `final_rel` take none: `Rel` carries them.

#### Dictionary records in the core

λFlick⁺ is the core of `Formal/` with dictionary records and globals; T1–T4, T4.3 and
`T1_regions` hold for it with the same statements, `RootTy` covering the globals. Four choices
keep the proofs of the core: projection is the core's `proj`, globals are a term form, fields
are schemes, and fields need not be values.

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
  `pure`. The bodies are a table next to `Σ₀`: `root : Roots` has `svc` (`Σ₀`) and
  `glob`. `RootTy` has `glob` (a body is closed source code without any `Site` and is
  typed at the scheme of its global) and `globs` (each declared global has a body).
  Instance bodies refer to other instances by name, so their typing is not circular.
  A global is not a constant: every constant is a value (`Value.const`), and a value
  must not step.
- **Proofs.** The `drec` and `dcons` cases are those of `letGen` (`TySubstBase.lean`:
  `Typ.drec_substT`, `Typ.dcons_substT`, `Typ.dproj_at`, `Typ.glob_substT`); a
  dictionary is canonical (`canonical_dict`), and its declared fields are there
  (`Typ.dfs_field`, `dict_field` with `Typ.instantiate`, `DictCanon.lean`); a global
  steps to a body typed by `Typ.instantiate` (`glob_preservation`); Progress takes
  `RootTy` for `globs`. `Value.field` takes the record typing, since a field of a
  dictionary need not be a value. `Examples.lean` projects a polymorphic method.
- **Lazy instances.** Fields are typed at their schemes exactly, and a dictionary holds
  nothing, so a lazy instance whose method result holds its continuation (`Monad (Eff r e)`)
  is typed in the core iff its method scheme shows that: the core has (≤-Cap) on arrows and
  `Eff`, but no `τ^q` on other types. The `CRow` parameter of simplification 15 is how the
  schemes show it ([Prelude schemes translated with CRow qualifiers](../docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes); see the core instance below).
- **Higher rank** ([no higher-rank parameters in the model](../docs/formal/elaboration.md#the-model-allows-polymorphic-types-only-in-dictionary-fields)). `elaboration.md` §0.1 also allows `∀` in an
  arrow parameter; the model has `∀` in dictionary fields only, as the source of `Elab` has
  no higher-rank parameters.

#### The core instance of `Target`

`typingTarget` is the typing of the core with dictionary records as the laws of
`Target`; `T6a_core`, `T6a_do_core` and `T6a_sugar_core` are T6(a) and its sugar forms
for the core, as instances of `T6a`, `T6a_do` and `T6a_sugar`.

- **Types** (`CoreTy.lean`). `var ↦ tvar`, `app ↦ tapp`, `Bool ↦ base bool`,
  other constructors `↦ adt (N.con c)`, `arr φ a b q ↦ arrow ⟦a⟧ ⟦φ⟧ ⟦b⟧ ⌈⟦q⟧⌉` (at `R r e` with
  the variables of the rows), `cnil ↦ caps ∅`, `cun ↦ caps (∪)`,
  `rnm c t ↦ row ({N.con c} ∪ ⌈⟦t⟧⌉)`, `run r r' ↦ row (⟦r⟧ ∪ ⟦r'⟧)` with each side as a row
  (`Ty.asRow`); `⟦R r e⟧ = rt ⟦r⟧ (⟦e⟧ ∪ ⌈⟦e⟧⌉ ∪ allNames)` (simplification 10).
  `trT_subst`/`trTT_subst`/`trS_subst` and `trT_reach` are the two commutations. The
  names of the core are a parameter `Names`, not a fixed encoding.
  `D_{C τ̄} ↦ dict (N.cls C |τ̄|) ⟦τ̄⟧`: the class name carries the number of parameters,
  because the law `slot` holds for any `q` and `Typ.dproj` needs `|τ̄| = k`.
- **Terms** (`CoreTr.lean`). `struct_C` and `prim_p` are families over the record type and
  the constraint (`struct_C : D_{C τ₁} →P … →P D_{C τ}` for each `τ`). The dictionary terms
  `Dict.strct C τ` and `Dict.prim p q` carry them ([structural dictionaries are type-indexed families](../docs/formal/elaboration.md#structural-and-primitive-dictionaries-are-families-indexed-by-type),
  [structural dictionary terms carry their type](../docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type)), so `Tr N γ δ e e'` is a function of the term: `meth`/`slot ↦ proj`,
  `inst`/`const ↦ glob`, `lamD l ↦ λ^l` (`lamsC`, the premise `k < l` at `l - 1 - k`, `dext`).
  `ite ↦ match c with true → a | _ → b` and `caseOpt ↦ match e with Some y → s | _ → n`:
  the last arm `_` makes `Covers` immediate, and `None` need not be declared.
- **Typing.** `Has Γ Δ e t φ := ∀ Ψ Θ Γc γ δ, Corr Γc Γ Δ γ δ → ∃ e', Tr γ δ e e' ∧
  Typ S Ψ Θ Γc e' (.ex ⟦t⟧ ⟦φ⟧)`. `Corr` says `Γc[γ x]? = ⟦Γ[x]?⟧` and
  `Γc[δ k]? = D_{Δ[k]?}`, `none` included, so the held premises transfer (`corr_held`).
- **Hypotheses** (`Real`), on the signature only: a record type per class and
  number of parameters with its slots (`N.slot i : D_{Sᵢ}`, `Prog.super` of `⟨C, vars n⟩`),
  for the class's own parameters a field per method at its scheme with the own variables
  first (`rot`), the method scheme closed and `s.ctx = ⟨C, vars p⟩ :: rest`; the closed
  schemes of the globals of instances (`instTy`), constants, structural and primitive
  dictionaries; `Some : ∀a. a → Option a`. The bodies of the globals (`RootTy`) and the
  slot fields of `struct_C` ([structural slot equals structural superclass dictionary](../docs/formal/elaboration.md#equivalence-did-not-relate-structural-slots-to-superclass-dictionaries)) are about the run: `RealRun` and
  `RootTy` below.
- **The laws** (`CoreLaws.lean`, `CoreDict.lean`), each one rule of the core:
  `var`, `dvar` — `Typ.var`; `const`, `inst`, `strct`, `prim` — `Typ.glob` (`has_glob`,
  `trTT_inst`); `app`, `dapp`, `dap` — `Typ.app`; `lam` — `Typ.lam`; `lamD` — `Typ.lam`
  `l` times (`lams_typ`); `let_` — `Typ.letGen` with `Φ = ∅`, and `Typ.let_` when `k = 0`
  and the right side runs at `φ`; `meth`, `slot` — `Typ.dproj` (`rot_inst`); `ite`,
  `caseOpt` — `Typ.match_`.
- **Two targets.** `typingTarget` has an empty run (`Rel = True`, no steps): T6(a) reads
  no run. `runTarget` has the run of the core with the same laws.
- **No value qualifier on other types.** `τ^q` on types other than arrows and `Eff` is not
  in the core: it would be a new type former with its (≤-Cap) in `Le`, and every case
  analysis on `Ty` and `Le` would change (substitution, `⌈·⌉`, canonical forms, shapes and
  patterns, `held`, preservation, T4 and T4.3). Instead the hidden qualifier of `Eff` and
  `Seq` is a parameter of kind `CRow` of the type (simplification 15), the schemes of
  `Sugar` show it (`elaboration.md` gives the translation of the checker's schemes,
  [Prelude schemes translated with CRow qualifiers](../docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes)), and a lazy body is `MkEff (task …)` of the nominal type `Eff q b`
  with the field `eff ∅ ∅ b {q}`: `typ_effPure_tr`, `typ_effMap2_tr` and `typ_effBind_tr`
  (`LazyEff.lean`) derive the bodies of `Monad Eff` at the schemes of `Sugar` with the class
  variable `Eff`.

#### The run: `Rel` and `Sim`

- **States.** `St := Cfg`, `load e := ⟨[], [], tr e⟩`, `Step := ∃ l, CStep root l`.
  `Obs := Lit`, observed when the main thread is a literal (ground observation).
- **Two properties of the model the run needs.** `Dict.strct C τ` and `Dict.prim p q` carry
  the type ([structural dictionary terms carry their type](../docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type)): a record type that the type of the whole term does not
  show is free, so a `Tr` that chose the structural (primitive) global by the type could run
  different globals for two `≈` terms. `Tr` is a function, and `≈` has
  `struct_C^τ ≈ struct_C^{τθ}`, `prim_q ≈ prim_{qθ}` (the run erases the type). The slot
  rules of `≈` take closed derivations, as `slot_ok` uses them ([slot equivalence over closed derivations only](../docs/formal/elaboration.md#slot-rules-of-equivalence-take-closed-derivations)):
  slot rules that drop arguments the slot does not mention make `≈` lose results, and the
  induction on `≈` ("≈ dictionaries give one value") breaks at `trans`.
- **The run of dictionaries.**
  - The core (`Decomp.lean`, `Closed.lean`, `SubstComp.lean`): `Redex`, `frame_inj`,
    `decomp_eq` (a term has one ordered decomposition `plug E r` with a redex `r`),
    `decomp_frame`; `PStep`, a pure step of one thread that reads neither the store nor
    the regions (`PHead` or a global), deterministic (`PStep.det`), and `PStep.step` into
    `Step`; `Tm.subst_fix`, `Tm.rename_fix` (a closed term is fixed); the four fusion laws
    `Tm.rename_rename`, `Tm.subst_rename`, `Tm.rename_subst`, `Tm.subst_subst`.
  - `CoreEval.lean`: `Eval root a v` (pure steps to a value), `SemEq` (same values),
    `Eval.det`, `Eval.of_step`, `eval_frame` (a frame is evaluated after its hole),
    `appsC`, `lamsC_subst`, `beta_chain` (`λ^n. D` applied to evaluated arguments is `D`
    with their values).
  - `CoreDictRun.lean`, `CoreDictSem.lean`: `trDt` (the translation of a dictionary,
    `TrD.eq`); `RealRun P N root`, the bodies of the dictionary globals (an instance is
    `λ^l. drec …` with the slot `N.slot i` the translation of `slots[i]`, `struct_C^τ` has
    the slot `struct_S^τ (δ̄.$slotᵢ)`, a primitive one is a value, `N.strct C (τθ)` and
    `N.strct C τ` have one body); `ent_eval` (a closed derivation evaluates, without any
    slot projection, so no induction on K5 is needed); `trDt_sem` (the values of a
    translated dictionary depend only on the values of its variables); **`eqv_sem`**: `≈`
    dictionaries have the same values, for every term, typed or not.
- **Alternatives that fail.** Inert dictionaries, where projection unfolds one slot, need
  Church–Rosser and standardisation of slot rewriting under call-by-value. A `Rel` built
  from translations of `Eqv` pairs is not a simulation: through `trans` a pair may hold a
  junk dictionary that is stuck. Choosing a typed translation in `load` is unsound.
- **`Rel`.**
  - `Cg B x a b` (`CoreCg.lean`): the congruence on core terms generated by base pairs `B`,
    one rule per term former. It has an index: inside a `drec` (down the spine and through
    `app` and `rcons`) no base pair may stand at the top, because `matchP` inspects the
    fields of a record without evaluating them (`rfield`); a value has no base pair at the
    top anyway, so it moves to either index (`Cg.value`). Closed base pairs make `Cg`
    closed under renaming and substitution (`Cg.rename`, `Cg.subst`, pointwise `SubR`).
  - No catching up: the left side of a base pair is never a value (`BNoVal`), so a value is
    related only to a value of the same shape (`Cg.value`), and the other side never has to
    evaluate a dictionary before the same rule applies. When the left side of a pair becomes
    a value, the right side reaches that value at once (`DP.step`), and the pair is gone.
  - The operations of the steps on related values (`CoreCgOps.lean`): `field`, `matchP`,
    `envLookup`, `envAppend`, `IsEnv`, `build`, `Final`; related contexts
    (`CoreCgFrames.lean`): `FrR`, plugging, order, `lookupE`, `withs`, and `Cg.decomp`, a
    related term around a hole is a base pair at some frame or the same frames around a
    related hole.
  - `DP S root a b` (`CoreRel.lean`), the base pairs: `a` is not a value and is typed in the
    empty context at the pure stage, `b` is closed, `SemEq root a b`. The typing of the core,
    not a syntactic class of dictionary terms, gives the open point: a step whose redex lies
    inside `a` is a `PStep` of `a` (`dp_calm`: a pure term has no runtime redex, `ask`,
    registration or `raise`), and a `PStep` keeps the typing (`pstep_typed`, with `RootTy`).
    No hypothesis on `Names` is needed.
  - `CfgR B`: related main threads, stores pointwise, regions pointwise (`RgR`: one `live`
    flag, related children and finalizers), with `RgR.setKid`.
- **The target with the run.**
  - `step_rel` (`CoreHead.lean`, `CoreStep.lean`, `CoreSim.lean`). For one thread
    (`thread_sim`): `Cg.decomp` at the redex of each `Step` rule; a base pair around it is
    `dp_calm` and `DP.step`, and the other side waits or reaches the value of the pair by pure
    steps (`inside_sim`); otherwise the hole is related by a congruence rule, and the other
    side takes the same `PHead` (`phead_sim`), `RHead` (`rhead_sim`, with the store and the
    regions) or `Step` rule. `cstep_sim` lifts it to `CStep.main` and `CStep.kid`; pure steps
    of a child are steps of the configuration (`kid_star`). `final_sim`: the result is the
    literal of the main thread (`obsC`), and a literal is related only to itself.
  - `load_rel` (`CoreLoad.lean`, `CoreRun.lean`). `≈` also relates open dictionaries (under a
    `λδ̄`), through `trans` and junk terms. Its congruence closure of closed pairs `DSim` is
    transitive, so `Eqv ⇒ DSim` (`eqv_dsim`) once the slots of an instance name only its
    premises (`SlotsBelow`); the bodies of the instances are closed, so `RootTy` and
    `RealRun` give it (`slots_below`, [equivalence as closure of closed pairs](../docs/formal/elaboration.md#equivalence-relates-open-dictionaries)). A closed pair of typed
    dictionaries is a base pair `DP`: `eqv_sem` gives `SemEq`, and `DTyp` the core's typing
    at the pure stage (`DTy.core`). `trE` is the function `Tr` is (`Tr.eq`);
    `load e = ⟨[], [], trE id id e⟩`.
  - The typing of the core of the whole term does not show its dictionaries in the empty
    context, so `runTarget` types terms with `RHas`, the core's `Has` together with `DTyp`
    of every dictionary of the term; the laws of `Target` give both
    ([dictionaries typed closed in the run target](../docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context)). Closing the premises of an elaboration gives `RHas` of the
    closed filling (`T6a_close`).
  - `runTarget hR hRun hroot : Target P` with `Real`, `RealRun` and `RootTy`; `T6b_core` is
    `T6b_target` for it: T6(b) for the core with its run, without the hypothesis `Sim`
    ([no base pairs inside dictionary records](../docs/formal/elaboration.md#record-patterns-read-dictionary-fields-without-evaluating-them)).

### The witness

`Formal/Elab/Witness/` is one program that meets every hypothesis of `T6a_core` and
`T6b_core` (simplification 11): `Eq` (an instance on `Int`, structural on a record),
`Applicative`/`Monad` of the lazy `Eff` and of an eager `OptQ`, `Alternative OptQ`,
`Iterable ListQ` and all of `Sugar`, with `Coherent` (`coherent`), `Real` (`real`), the
bodies of the globals (`rootTy`, `realRun`) and the constants of `Γ₀` closed
(`consts_closed`). Its own schemes are written with `bindTy`, `map2Ty` and `eachSch`, and `S`
equates them with the generated ones of `PreludeSchemes.lean` by `rfl`.

- `witness_T6a`, `witness_T6a_do` and `witness_T6b` (`Witness/Main.lean`) are `T6a_core`,
  `T6a_do_core` and `T6b_core` with every hypothesis discharged; `do { x <- pure (); pure x }`
  desugars and elaborates over `Eff` and over `OptQ` and is typed in the core (`do_eff`,
  `do_opt`).
- `witness_T6b_example` (`Witness/T6b.lean`) is that block over `Eff` under the premise
  `Monad Eff` closed by the instance `I2`, with `Applicative Eff` for `pure` from the instance
  `I1` in one filling and from the slot of `Monad Eff` in the other. Both typings `RHas` are
  proved by the laws of `runTarget` (`has_do`), and the observations of the runs are equal.
  It has no premises.
- `witness_T6b_closed` is T6(b) for any two elaborations of one source term in the empty
  contexts (`Δ = []`), typed by T6(a) for `runTarget`.
- `witness_T6b_open` is the same under premises `Δ` closed by any types `θ` and closed
  dictionaries `ρ` to ground constraints ([dictionaries typed closed in the run target](../docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context)): the typings `RHas` of
  the closed fillings `s[d̄ρ]` come from `T6a_close`, T6(a) with the premises closed, for
  `runTarget`. It needs the skeleton's holes outside the abstractions over dictionaries
  (`Tm.Skel`), the constants of `Γ₀` closed (`consts_closed`), and `struct_C^τ` typed at
  `τθ` ([structural dictionaries typed at any type instance](../docs/formal/elaboration.md#structural-dictionaries-were-typed-only-at-their-own-record-type)).
