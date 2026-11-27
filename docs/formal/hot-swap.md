# Hot swap and typed state-cell migrations

This page specifies the live-session extension of λFlick for hot-swapping global
cells and migrating declared state cells, implemented in
[`flick-model/src/check/compat.rs`](../../flick-model/src/check/compat.rs) and
[`flick-model/src/hotswap/`](../../flick-model/src/hotswap/), and proved in
[`formal/Formal/HotSwap.lean`](../../formal/Formal/HotSwap.lean).

## 1. Live session model

A live session holds:

- a signature `Sig` (`Decls`) declaring the schemes of global cells `g : σ`;
- a root table `Roots` (`Program`) mapping each global cell `g` to a closed pure
  source value `v`;
- a running configuration `⟨Λ, μ, e⟩` (regions, store, and active frames) typed
  at `CfgTy S Ψ c t φ`;
- a table of **declared state cells**, each pointing to a store location
  `ℓ < |μ|` holding a value of its declared type `τ` (`Ψ[ℓ] = τ`), together with
  its version history.

A live session evolves by three transitions:

1. **Execution (`CStep`)**: ordinary small-step execution of the main thread and
   live region children.
2. **Hot swap (`swap`)**: replaces a global cell `g` (previously of scheme
   `σ_old`) with a closed source value `v_new` of scheme `σ_new` when
   `σ_new` subsumes `σ_old`.
3. **Migration (`migrate`)**: applies a batch of closed pure functions
   `m_i : Old_i -> New_i` to declared state cells `ℓ_i` atomically.

## 2. Compatibility rule (scheme subsumption)

A replacement scheme `σ_new = ∀ β̄. Θ_new ⇒ τ_new` is compatible with
`σ_old = ∀ ᾱ. Θ_old ⇒ τ_old` (`SchemeSubsumes S σ_new σ_old`, Rust
`check_scheme_compatibility`) iff for every fact context `Θ` and valid
instantiation `ts` of `σ_old` (`|ts| = σ_old.k`, `σ_old.Holds Θ ts`), there
exists an instantiation `us` of `σ_new` (`|us| = σ_new.k`, `σ_new.Holds Θ us`)
such that:

```text
LeLive S (σ_new.inst us) (σ_old.inst ts)
```

This single judgment enforces all hot-swap variance rules:

| Change | Verdict | Why |
| - | - | - |
| Generalize scheme (`Int -> Int` to `∀ a. a -> a`) | accepted | choose `us = [Int]` |
| Narrow scheme (`∀ a. a -> a` to `Int -> Int`) | rejected | fails at `ts = [Bool]` |
| Narrow effect row (`Int ->{Log; Oops} Int` to `Int -> Int`) | accepted | `Pure ⊑ Runtime {Log; Oops}` |
| Widen effect row (`Int -> Int` to `Int ->{Log; none} Int`, or adding a service/error) | rejected | violates `Stage.use` monotonicity in `LeLive.arrow` / `LeLive.eff` |
| Widen parameter capture/region qualifier (`((Unit ->{} Unit)^∅) ->{} Unit` to `∀ c. ((Unit ->{} Unit)^c) ->{} Unit`) | accepted | arrow parameter position is contravariant |
| Narrow parameter capture/region qualifier (`∀ s. ((Unit ->{} Unit)^{s}) ->{} Unit` to `((Unit ->{} Unit)^∅) ->{} Unit`) | rejected | contravariant `LeLive.arrow_use` fails |
| Widen returned/callable capture qualifier (`^∅` to `^{s}`) | rejected | covariant `LeLive.arrow_use` / `LeLive.eff_use` fails |

In `flick-model`, `Checker::check_scheme_subsumes` decides this by converting
both schemes through `live_scheme`, skolemizing `σ_old` (adding its bounds and
`Outlives` facts as givens), instantiating `σ_new` with fresh metas (posting its
bounds and facts as obligations), checking `subsume(new_body, old_body)`, and
settling all row and region obligations (`finish`).

## 3. Two-version execution and atomic migrations

- **Old frames keep captured code**: frames and closures that already unfolded
  `g` retain `v_old` by substitution and stay well typed under the updated
  signature `S'`. Any subsequent evaluation of `.glob g` steps via `Step.glob` to
  `v_new`.
- **Two-version store typing on migration**: when every pure migration function
  `m_i (μ[ℓ_i])` evaluates to a value `v_i'` within fuel without mutating `μ` or
  `Λ`, committing appends `[v_1', …, v_n']` to `μ` and extends the store typing
  from `Ψ` to `Ψ ++ [New_1, …, New_n]`. Pre-migration cells `ℓ_i` remain intact
  at their original indices with type `Old_i` so suspended frames referencing
  `Loc ℓ_i` remain well typed; newly migrated state reads use `ℓ_i' = |μ| + i`
  at type `New_i`. (When `New_i = Old_i`, in-place update `μ[ℓ_i] := v_i'` also
  preserves `CfgTy S Ψ`.)
- **Atomicity**: if any migration in a batch fails type checking, raises,
  panics (`Die`), gets stuck, or diverges (exhausts fuel), the entire batch
  rolls back and leaves `(Sig, Roots, Λ, μ, e, state_cells)` unchanged.

## 4. What Lean proves and its limits

[`formal/Formal/HotSwap.lean`](../../formal/Formal/HotSwap.lean) proves the
following theorems (all pinned in [`formal/Formal/Axioms.lean`](../../formal/Formal/Axioms.lean)
with zero `sorry` and only standard Lean axioms `propext`, `Classical.choice`,
and `Quot.sound`):

| Guarantee | Lean theorem |
| - | - |
| Scheme subsumption witnesses (generalization accepted; narrowing, effect widening, parameter capture narrowing rejected) | `subsumes_generalize_mono`, `not_subsumes_narrow_poly`, `not_subsumes_widen_effect`, `not_subsumes_narrow_param_cap` |
| Preservation of configuration typing under a compatible swap | `swap_preserves_cfgTy`, `SessionOk.swap` |
| Two-version dispatch (old frame keeps captured code; fresh `.glob g` enters new code) | `swap_old_frame_and_new_call` |
| Preservation under a well-typed migration (store extended with both old and new versions; plus same-type in-place corollary) | `migration_preserves_cfgTy`, `migration_in_place_preserves_cfgTy`, `SessionOk.migrate` |
| Atomicity on failing or diverging migration | `migration_rollback_atomic` |
| Progress unaffected; old frames remain well typed against the extended store typing | `old_frame_well_typed_under_extended_store`, `hotSwap_progress_unaffected` |

### Out of scope

- Differential `.fvm` image save/load and runtime VM hot-swap execution (tracked
  separately in `#990`; `flick-model` exports `check_scheme_compatibility`,
  `generate_session`, and `run_session_oracle` so `#990` can mirror the exact
  compatibility rule and replay the same generated sessions on `flick-vm`).
- Migrations of values outside declared state cells (e.g. arbitrary heap
  closures or non-state `Ref` cells).
- Like the rest of `formal/`, these theorems prove properties of the formal
  calculus and reference model, not of the Rust compiler or VM implementation.
