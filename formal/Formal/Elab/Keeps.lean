import Formal.Elab.Source

/-!
# Elaboration: what a type shows and a row keeps live, under substitution

`Ty.Shown` and `Ty.Keeps` (`Source.lean`) commute with renaming and substitution as
`Ty.Occurs` does (`TyRen.lean`), so a stage that keeps a qualifier live keeps it live at every
instance (`Stage.Keeps.subst`): closing the premises of an elaboration keeps (App) typed.
-/

namespace Flick.Elab

theorem Ty.shown_ren (t : Ty) (ξ : Nat → Nat) (n : Nat) :
    (t.ren ξ).Shown n ↔ ∃ m, t.Shown m ∧ ξ m = n := by
  induction t using Ty.ind generalizing ξ n with
  | var m => simp [Ty.ren, Ty.Shown]
  | con | cnil => simp [Ty.ren, Ty.Shown]
  | app t u iht ihu | cun t u iht ihu | run t u iht ihu =>
    simp only [Ty.ren, Ty.Shown, iht, ihu]; grind
  | rnm c t iht => simp only [Ty.ren, Ty.Shown, iht]
  | arr φ a b q iha ihb ihq _ =>
    cases φ <;> (simp only [Ty.ren, Ty.Shown, iha, ihb, ihq]; grind)
  | all k t ih =>
    simp only [Ty.ren, Ty.Shown, ih]
    constructor
    · rintro ⟨m, hm, he⟩
      simp only [liftN] at he
      split at he
      · omega
      · exact ⟨m - k, by rwa [Nat.sub_add_cancel (by omega)], by omega⟩
    · rintro ⟨m, hm, rfl⟩
      exact ⟨m + k, hm, by simp [liftN, show ¬ (m + k < k) by omega]⟩

theorem liftTy_shown (k : Nat) (σ : Nat → Ty) (m n : Nat) :
    (liftTy k σ m).Shown (n + k) ↔ k ≤ m ∧ (σ (m - k)).Shown n := by
  simp only [liftTy]
  split
  · simp [Ty.Shown]; omega
  · simp only [Ty.shown_ren]
    constructor
    · rintro ⟨x, hx, he⟩
      have : x = n := by omega
      subst this; exact ⟨by omega, hx⟩
    · rintro ⟨_, h⟩; exact ⟨n, h, rfl⟩

theorem Ty.shown_subst_iff (σ : Nat → Ty) (t : Ty) (n : Nat) :
    (t.subst σ).Shown n ↔ ∃ m, t.Shown m ∧ (σ m).Shown n := by
  induction t using Ty.ind generalizing σ n with
  | var m => simp [Ty.subst, Ty.Shown]
  | con | cnil => simp [Ty.subst, Ty.Shown]
  | app t u iht ihu | cun t u iht ihu | run t u iht ihu =>
    simp only [Ty.subst, Ty.Shown, iht, ihu]; grind
  | rnm c t iht => simp only [Ty.subst, Ty.Shown, iht]
  | arr φ a b q iha ihb ihq _ =>
    cases φ <;> (simp only [Ty.subst, Ty.Shown, iha, ihb, ihq]; grind)
  | all k t ih =>
    simp only [Ty.subst, Ty.Shown, ih, liftTy_shown]
    constructor
    · rintro ⟨m, hm, hk, h⟩
      exact ⟨m - k, by rwa [Nat.sub_add_cancel hk], h⟩
    · rintro ⟨m, hm, h⟩
      exact ⟨m + k, hm, by omega, by simpa using h⟩

/-- A row keeps live at an instance what the instances of the variables it keeps show. -/
theorem Ty.Keeps.subst {σ : Nat → Ty} {r : Ty} {n m : Nat} (h : r.Keeps n) (hm : (σ n).Shown m) :
    (r.subst σ).Keeps m := by
  induction r using Ty.ind with
  | var v => exact h.elim
  | rnm c t _ => exact (Ty.shown_subst_iff σ t m).2 ⟨n, h, hm⟩
  | run r r' ihr ihr' =>
    rcases h with h | h
    · exact .inl (ihr h)
    · exact .inr (ihr' h)
  | con c => exact h.elim
  | cnil => exact h.elim
  | app t u _ _ => exact (Ty.shown_subst_iff σ (.app t u) m).2 ⟨n, h, hm⟩
  | arr φ a b q _ _ _ _ => exact (Ty.shown_subst_iff σ (.arr φ a b q) m).2 ⟨n, h, hm⟩
  | cun q q' _ _ => exact (Ty.shown_subst_iff σ (.cun q q') m).2 ⟨n, h, hm⟩
  | all k t _ => exact (Ty.shown_subst_iff σ (.all k t) m).2 ⟨n, h, hm⟩

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
