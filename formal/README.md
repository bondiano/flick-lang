# λFlick in Lean 4

A mechanization of the core calculus in [`docs/formal/core.md`](../docs/formal/core.md).
Lean 4.29.1, no Mathlib.

```sh
cd formal
lake build --wfail      # or: nix shell nixpkgs#lean4 -c lake build --wfail
```

`--wfail` makes a `sorry` (reported as a warning) fail the build; CI runs the same
command in the `lean` job. `#print axioms Flick.T1` (and `T2a`, `T2b`) lists only Lean's
standard `propext`, `Classical.choice` and `Quot.sound`.

## Status

| Stage | Scope | State |
| - | - | - |
| P1a | λ, `let` without generalization, `let rec`, `match` with exhaustiveness and guards, records, `Ref` and the store | done |
| P1b | stages `P`/`R⟨r;ε⟩` and subtyping, closed rows, `raise`/`try`/`handle`, `task`/`await`, `ask`/`with`/`provide` and layers; Л1–Л3, Progress, Preservation, T1, T2(a), T2(b) | done |
| P1c | generalization with the premise `P`, lemma 3.1, T3 | to do |

## Map to `core.md`

| `core.md` | Lean |
| - | - |
| §1.2 types, stages, rows | `Ty`, `Stage`, `Row` (`Syntax.lean`) |
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
| Л2 (monotypes) | `Typ.subst`, `Typ.inst` (`Substitution.lean`) |
| Л3 | `ctx_typing` (`Lemma3.lean`) |
| Progress | `progress` (`ProgressMain.lean`) |
| Preservation | `preservation` (`Preservation.lean`) |
| T1 | `T1` (`Soundness.lean`) |
| T2(a), T2(b) | `T2a`, `T2b`, `T2b_await` (`Effects.lean`) |

## Representation

Terms use **de Bruijn indices** with parallel substitution (`Tm.rename`, `Tm.subst`,
`inst`). Binders that bind several variables at once are common here: a pattern binds
`p.binds` variables, and `let rec`/`fix` bind two. With de Bruijn indices a branch body
is simply under `p.binds` binders, `e[δ]` is `inst δ`, and the substitution lemma is
the usual "a well-typed substitution maps a well-typed term to a well-typed term".
Locally nameless would need opening by a list of fresh names and cofinite
quantification in every binding rule, which costs more without a library.
Typing contexts are lists, newest variable at index 0; a pattern's `Δ` is prepended
in binding order.

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

1. **Monomorphic.** Types have no variables and no `∀`; rows are closed; constants carry
   their instance (`Const.refMake τ`, `Const.effect S r ε`). ADTs and errors have no
   type parameters. `let` is (Let-Mono); `let x : τ = e` is the same term without the
   annotation; `let rec` is monomorphic. (Let-Gen), (Let-Sig) and polymorphic
   constants come with P1c.
2. **Declarations** are a parameter `S : Sig`: constructor ↦ argument types and type
   name, service ↦ `S.Impl`. No well-formedness of `S` is needed. An error type `E` is an
   ADT whose constructors are the error's constructors.
3. **`Unit`, `Bool`, `Int`** are base types with literals (`Lit`), not nominal ADTs;
   `True`/`False` patterns are literal patterns.
4. **Records are closed with a fixed field order.** No row tail and no equality up to
   permutation: the elaborator is assumed to order labels. Record patterns are always
   open (`{…, ..}`), as in `core.md`.
5. **`Exh` over shapes.** `Exh(τ, Π)` quantifies over `Shp τ v`, the closed values of `τ`
   as far as patterns see them (functions, references, computations, layers, errors and
   environments are opaque), not over typed values: the typed version would put `Typ`
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
    suffices for T2.
12. **Steps carry labels** (`Lbl`): pure, runtime, or `root S` for an `ask_S` answered by
    `Σ₀`; T2(b) is about the `root` labels. `todo → die` is labelled pure, as T2(a) allows
    `die`; `bind_S` and `join` steps are pure (Р101).
13. **Record tails** (widening). `rcons ℓ e r` types any `r : R`, so `R` need not be a
    record type. Such terms do not arise from the surface; field lookup and update
    treat a non-record tail as having no fields.

## Findings

- **Р100** (P1a): a partially applied constructor is a value, and Л1 for an arrow type
  has to list it (`Value.ctorApp`, `canonical_arrow`).
- **Р101** (P1b): `bind_S` and `join` steps were runtime steps while their rules are
  pure, so T2(a) failed for a bare `bind_S v`; they are pure steps now.
- **Р102** (P1b): (Handle) asked for the whole row `ε_b ⊆ H + ε'`, which a failed arm
  breaks; it now asks for the error of the value only, as `flick-model` already did.
