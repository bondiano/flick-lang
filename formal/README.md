# λFlick in Lean 4

A mechanization of the core calculus in [`docs/formal/core.md`](../docs/formal/core.md).
Lean 4.29.1, no Mathlib.

```sh
cd formal
lake build --wfail      # or: nix shell nixpkgs#lean4 -c lake build --wfail
```

`--wfail` makes a `sorry` (reported as a warning) fail the build; CI runs the same
command in the `lean` job. `#print axioms Flick.T1` lists only `propext` and `Quot.sound`.

## Status

| Stage | Scope | State |
| - | - | - |
| P1a | λ, `let` without generalization, `let rec`, `match` with exhaustiveness and guards, records, `Ref` and the store; Progress, Preservation, T1 without effects | done |
| P1b | stages `P`/`R⟨r;ε⟩`, rows, `raise`/`try`, `task`/`await`, `ask`/`with`/`provide`; T2(a), T2(b), Л3 | to do |
| P1c | generalization with the premise `P`, lemma 3.1, T3 | to do |

## Map to `core.md`

| `core.md` | Lean |
| - | - |
| §1.2 types | `Ty` (`Syntax.lean`) |
| §1.3 terms, patterns | `Tm`, `Pat` (`Syntax.lean`) |
| §1.3, §3.1 values | `Value` (`Typing.lean`) |
| §1.5 constants `Ref.*` | `Const` (`Syntax.lean`) |
| §2.3 rules, §3.6 (Loc), (Fix) | `Typ` (`Typing.lean`) |
| §2.4 patterns, `Exh` | `PatTy`, `Shp`, `Exh` (`Typing.lean`) |
| §3.6 (Match-Val), Р29 | second disjunct of `Covers` (`Typing.lean`) |
| §3.6 `⊢ μ : Ψ` | `StoreTy` (`Typing.lean`) |
| §3.2 contexts | `Frame`, `plug` (`Semantics.lean`) |
| §3.3 pure steps, §3.4 `Ref` steps, `match(p, v)` | `Head`, `matchP` (`Semantics.lean`, `Typing.lean`) |
| §3.4, §3.5 configuration steps, `die` | `Step`, `Steps` (`Semantics.lean`) |
| Л1 canonical forms | `canonical_arrow`, `canonical_ref`, `Value.shape` (`Canonical.lean`) |
| Л2 substitution (monotypes) | `Typ.subst`, `Typ.inst` (`Substitution.lean`) |
| Л3 without effects | `ctx_typing` (`Context.lean`) |
| Progress | `progress` (`Progress.lean`) |
| Preservation | `preservation` (`Preservation.lean`) |
| T1 | `T1` (`Soundness.lean`) |

## Representation

Terms use **de Bruijn indices** with parallel substitution (`Tm.rename`, `Tm.subst`,
`inst`). The reason is binders that bind several variables at once: a pattern binds
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
- match arms are `acons p b rest`, `aguard p g b rest`, ending in `anil`, with their own
  judgment form `J.alts τ τ'` in the same inductive `Typ`.

## Assumptions and simplifications

Each item narrows `core.md`, except the last, which widens the syntax harmlessly.

1. **No stages or rows in P1a.** The judgment is `Γ;Ψ ⊢ e : τ` without `! φ`; `Ref.*`
   are plain arrows. Without stages there is no subtyping, so there is no (Sub).
2. **Monomorphic.** Types have no variables and no `∀`; constants carry their instance
   (`Const.refMake τ`). ADTs have no type parameters. `let` is (Let-Mono); `let x : τ = e`
   is the same term without the annotation; `let rec` is monomorphic. (Let-Gen),
   (Let-Sig) and polymorphic constants come with P1c.
3. **ADT declarations** are a parameter `S : Sig` (constructor ↦ argument types, type
   name). No well-formedness of `S` is needed.
4. **`Unit`, `Bool`, `Int`** are base types with literals (`Lit`), not nominal ADTs;
   `True`/`False` patterns are literal patterns. The guard step builds
   `match g[δ] with True → … | False → …` from literal patterns.
5. **Records are closed with a fixed field order.** No row tail and no equality up to
   permutation: the elaborator is assumed to order labels. Distinct labels are
   `ℓ ∉ R.labels`. Record patterns are always open (`{…, ..}`), as in `core.md`.
6. **`Exh` over shapes.** `Exh(τ, Π)` quantifies over `Shp τ v`, the closed values of `τ`
   as far as patterns see them (functions and references are opaque), not over typed
   values: the typed version would put `Typ` in a negative position of its own
   definition. Patterns only inspect literals, constructors and record fields, so the
   two agree. The exhaustiveness algorithm (`exhaust.rs`) is not modeled.
7. **Constructor patterns** at a function type are not accepted: `PatTy` and `Shp` carry
   a flag that is `false` only for an unsaturated constructor spine.
8. **Allocation is deterministic:** `Ref.make v` takes `ℓ = μ.length`, one choice of
   `ℓ ∉ dom μ`.
9. **Programs** are closed terms typed in the empty store; modules (§2.6) and `Σ₀` are
   not modeled. Without `raise`, case 2 of T1 is absent; `die` comes only from `todo`.
10. **(Match-Val)** is the second disjunct of the coverage premise `Covers` of the one
    match rule, not a separate rule.
11. **Record tails.** `rcons ℓ e r` types any `r : R`, so `R` need not be a record type
    (`{ℓ = 1 | 2}` is typable). Such terms do not arise from the surface; field lookup
    and update treat a non-record tail as having no fields, so the theorems still hold.

## Findings

- **Р100** (`core.md` §5): a partially applied constructor is a value, and Л1 for an arrow
  type has to list it (`Value.ctorApp`, `canonical_arrow`).
