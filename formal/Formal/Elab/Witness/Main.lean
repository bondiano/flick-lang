import Formal.Elab.Witness.Root
import Formal.Elab.CoreRun

/-!
# Witness: T6 on a concrete program

The hypotheses of T6(a) and T6(b) for the core — `Real`, `RealRun`, `RootTy`, `Coherent` and
`Sugar` — hold together for the program of `Prog.lean`: `witness_T6a`, `witness_T6a_do` and
`witness_T6b` are `T6a_core`, `T6a_do_core` and `T6b_core` with every hypothesis discharged.
`do { x <- pure (); pure x }` desugars and elaborates over the lazy `Eff` and over the eager
`OptQ`, and is typed in the core (`do_eff`, `do_opt`); the direct dictionary of
`Applicative Eff` and the slot of `Monad Eff` are `≈` (`slot_eqv`).
-/

namespace Flick.Elab.Witness

open Flick.Elab

/-- **T6(a) on the witness.** -/
theorem witness_T6a {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') {Ψ Θ Γc γ δ}
    (hc : Corr N Γc (Γ.map Scheme.tr) Δ γ δ) :
    ∃ e'', Tr N γ δ e' e'' ∧ Typ Sg Ψ Θ Γc e'' (.ex (trT N τ) (trStage N φ)) :=
  T6a_core real h hc

/-- **T6(a) for `do` on the witness.** -/
theorem witness_T6a_do {Γ Δ ss r m b φ} (h : DoOk S Γ Δ ss r m b φ) :
    ∃ q e', Elab P Γ Δ (S.desugarDo ss r) (appQ m q b) φ e' ∧
      Has Sg N (Γ.map Scheme.tr) Δ e' (.mono (appQ m q b)) φ :=
  T6a_do_core real S h

/-- **T6(b) on the witness**, with the run of the core. -/
theorem witness_T6b (s : Tm) {t : TTy} {φ : Stage} {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    (hρ : Closes P Δ θ ρ) {qs : List Cn} {ds₁ ds₂ : List Dict} (h₁ : Ent P Δ qs ds₁)
    (h₂ : Ent P Δ qs ds₂) (o₁ : RHas P Sg N [] [] (s.fill (ds₁.map (·.subst ρ))) t φ)
    (o₂ : RHas P Sg N [] [] (s.fill (ds₂.map (·.subst ρ))) t φ) :
    All2 (Eqv P) (ds₁.map (·.subst ρ)) (ds₂.map (·.subst ρ)) ∧
      (runTarget real realRun rootTy).exprs.run (s, t, φ) (ds₁.map (·.subst ρ)) =
        (runTarget real realRun rootTy).exprs.run (s, t, φ) (ds₂.map (·.subst ρ)) :=
  T6b_core real realRun rootTy coherent s hρ h₁ h₂ o₁ o₂

/-- `do { x <- pure (); pure x }` over the container `con c` with the instances `I` of
`Applicative` and `J` of `Monad`. -/
theorem doOk_pure (c I J : Nat) (hI : insts I = some ⟨cApp, [.con c], [], []⟩)
    (hJ : ∃ sl, insts J = some ⟨cMonad, [.con c], [], sl⟩) :
    DoOk S [] [] [.bind (.app (.meth 1) (.const 8))] (.app (.meth 1) (.var 0)) (.con c) (.con 1) .P := by
  obtain ⟨sl, hJ⟩ := hJ
  have hA : ∀ Γ, Ent P Γ [⟨cApp, [.con c]⟩] [.inst I] := fun _ =>
    .inst (σ := Ty.var) (ds := []) hI rfl rfl .nil
  have hp : ∀ Γ (x : Exp) a (x' : Tm), Elab P Γ [] x a .P x' →
      Elab P Γ [] (.app (.meth 1) x) (appQ (.con c) .cnil a) .P (.app (.meth (.dict (.inst I)) 1) x') :=
    fun Γ x a x' hx => .app (elab_meth (Γ := Γ) S.hPure rfl (ts := [.con c, .cnil, a]) rfl
      (by simpa [Cn.subst, Ty.subst, inst] using hA [])
      (τ := arrP a (appQ (.con c) .cnil a)) rfl) hx (.inl rfl)
  exact .bind (.inst (σ := Ty.var) (ds := []) hJ rfl rfl .nil)
    (hp [] _ _ _ (elab_const S.hUnitV (ts := []) rfl rfl))
    (.ret (hp _ _ _ _ (.var (σ := ⟨0, [], .con 1, .cnil⟩) (ts := []) rfl rfl .nil)))

/-- The block over the lazy `Eff` is typed in the core. -/
theorem do_eff : ∃ q e', Elab P [] [] (S.desugarDo [.bind (.app (.meth 1) (.const 8))]
    (.app (.meth 1) (.var 0))) (appQ (.con 5) q (.con 1)) .P e' ∧
    Has Sg N [] [] e' (.mono (appQ (.con 5) q (.con 1))) .P :=
  witness_T6a_do (doOk_pure 5 1 2 rfl ⟨_, rfl⟩)

/-- The block over the eager `OptQ` is typed in the core. -/
theorem do_opt : ∃ q e', Elab P [] [] (S.desugarDo [.bind (.app (.meth 1) (.const 8))]
    (.app (.meth 1) (.var 0))) (appQ (.con 6) q (.con 1)) .P e' ∧
    Has Sg N [] [] e' (.mono (appQ (.con 6) q (.con 1))) .P :=
  witness_T6a_do (doOk_pure 6 3 4 rfl ⟨_, rfl⟩)

/-- Two derivations of `Applicative Eff` under the premise `Monad Eff`: the slot of the premise
and the instance. Closed by `I2`, they are `≈`, so T6(b) gives one result for both. -/
theorem slot_eqv : All2 (Eqv P) ([Dict.slot (.var 0) 0].map (·.subst fun _ => .inst 2))
    ([Dict.inst 1].map (·.subst fun _ => .inst 2)) :=
  .cons (.instSlot (I := 2) (i := 0) (s := .inst 1) (σ := Ty.var) (ds := []) rfl rfl .nil) .nil

end Flick.Elab.Witness
