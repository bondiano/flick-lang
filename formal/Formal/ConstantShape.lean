import Formal.ShapeEquivalence

/-!
# Shape of a partially applied constant

A binary constant still returns an arrow after one argument, including after
value type conversion and the intermediate error-value typing rule.
-/

namespace Flick

variable {S : Sig}

theorem Const.ty_kind (c : Const) (ι : CArg) : (c.ty ι).kind = 1 := by
  obtain ⟨_, _, _, _, h⟩ := Const.ty_isArrow c ι
  rw [h]; rfl

/-- The result of a binary constant after one argument. -/
theorem Const.ty_res {c : Const} {ι : CArg} (h : c.arity = 2) :
    ∃ x y, c.ty ι = .arrow x .pure y Caps.none ∧ y.kind = 1 := by
  cases c <;> simp [Const.arity] at h <;> exact ⟨_, _, rfl, rfl⟩

theorem Const.res_kind {c : Const} {ι a ψ t C T'} (hc : c.arity = 2)
    (h : LeLive S (c.ty ι) (.arrow a ψ t C)) (hT : LeLive S t T') : T'.kind = 1 := by
  obtain ⟨x, y, hxy, hk⟩ := Const.ty_res (ι := ι) hc
  rw [hxy] at h
  obtain ⟨_, _, _, _, h', _, _, lt, -⟩ := h.arrow_left
  cases h'; rw [← hT.kind, ← lt.kind, hk]


theorem Typ.constArrow_kind {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ c a ψ t C φ, e = .const c → c.arity = 2 → J = .ex (.arrow a ψ t C) φ → t.kind = 1 := by
  induction h with
  | conv _ he ih =>
    intro c a ψ t C φ hc hArity hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | arrow _ _ ht => rw [ht.kind]; exact ih c _ _ _ _ _ hc hArity rfl
  | const _ hT =>
    intro c a ψ t C φ hc hArity hJ; cases hc; cases hJ
    exact Const.res_kind hArity hT .refl
  | perform => intro c a ψ t C φ hc hArity _; cases hc; simp [Const.arity] at hArity
  | allI | errVal | funcRef => intro c a ψ t C φ _ _ hJ; cases hJ
  | _ => intro c a ψ t C φ hc _ _; cases hc

private theorem Typ.constApp_kindCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ c w T φ, e = .app (.const c) w → c.arity = 2 → J = .ex T φ → ¬ T.isAll → T.kind ≠ 17 →
      T.kind = 1 := by
  induction h with
  | conv _ he ih =>
    intro c w T φ hc hArity hJ hA hk; cases hJ
    rw [← he.kind]
    exact ih c w _ _ hc hArity rfl (fun h => hA (he.isAll.mp h)) (by rw [he.kind]; exact hk)
  | app hf _ _ hT =>
    intro c w T φ hc hArity hJ _ _; cases hc; cases hJ
    rw [← hT.kind]
    exact Typ.constArrow_kind hf _ _ _ _ _ _ rfl hArity rfl
  | allI => intro c w T φ _ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | funcRef => intro c w T φ _ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | errVal _ hU _ ih =>
    intro c w T φ hc hArity hJ _ _; cases hJ
    have hk := ih c w _ _ hc hArity rfl (Ty.not_all_of_hd hU) (by rw [Ty.kind_of_hd hU]; decide)
    rw [Ty.kind_of_hd hU] at hk
    cases hk
  | _ => intro c w T φ hc _ _ _ _; cases hc

theorem Typ.constApp_kind {Ψ Θ Γ c w T φ} (h : Typ S Ψ Θ Γ (.app (.const c) w) (.ex T φ))
    (hc : c.arity = 2) (hA : ¬ T.isAll) (hk : T.kind ≠ 17) : T.kind = 1 :=
  h.constApp_kindCore c w T φ rfl hc rfl hA hk

end Flick
