import Formal.Elab.Source

/-!
# Elaboration: value bounds and stage applicability under substitution

`Ty.Shown` and `Ty.Keeps` commute with renaming and substitution (`Variance.lean`), so a stage
that keeps a qualifier live keeps it live at every instance (`Stage.Keeps.subst`): closing the
premises of an elaboration keeps (App) typed.
-/

namespace Flick.Elab

/-- Value bounds that hold hold at every instance. -/
theorem Bounds.Hold.subst {bd : List (Ty × Ty)} (h : Bounds.Hold bd) (σ : Nat → Ty) :
    Bounds.Hold (bd.map (Bound.subst σ)) := by
  intro b hb n hn
  obtain ⟨b₀, hb₀, rfl⟩ := List.mem_map.1 hb
  obtain ⟨m, hm, hσ⟩ := (Ty.shown_subst_iff σ _ n).1 hn
  exact (Ty.shown_subst_iff σ _ n).2 ⟨m, h b₀ hb₀ m hm, hσ⟩

theorem Bound.subst_comp (bd : List (Ty × Ty)) (σ θ : Nat → Ty) :
    (bd.map (Bound.subst σ)).map (Bound.subst θ) = bd.map (Bound.subst fun n => (σ n).subst θ) := by
  simp [Bound.subst, Ty.subst_comp]

/-- A stage that keeps a qualifier live keeps it live at every instance. -/
theorem Stage.Keeps.subst {φ : Stage} {q : Ty} (σ : Nat → Ty) (h : φ.Keeps q) :
    (φ.subst σ).Keeps (q.subst σ) := by
  cases φ with
  | P => exact h.elim
  | R r e =>
    intro m hm
    obtain ⟨n, hn, hm⟩ := (Ty.shown_subst_iff σ q m).1 hm
    exact (h n hn).subst hm

/-- The premise of (App) at an instance. -/
theorem Stage.subst_keeps_or {ψ φ : Stage} {q : Ty} (θ : Nat → Ty) (h : ψ.Applies φ q) :
    (ψ.subst θ).Applies (φ.subst θ) (q.subst θ) := by
  rcases h with rfl | ⟨hk, rfl | ⟨C, r, e, rfl, rfl, hC⟩⟩
  · exact .inl rfl
  · exact .inr ⟨hk.subst θ, .inl rfl⟩
  · exact .inr ⟨hk.subst θ, .inr ⟨C.subst θ, r.subst θ, e.subst θ, rfl, rfl, hC.subst θ⟩⟩

end Flick.Elab
