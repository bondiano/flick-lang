import Formal.Elab.TyRen

/-!
# Elaboration: subtyping (`Ty.Le`), class variance inference, and `Cn.Serves` (B1)

This module formalizes:
1. Structural properties of `Ty.Shown`, `Ty.Keeps`, `RowLe`, `Stage.UseLe`, `Ty.Le`, and
   `Cn.Serves` (reflexivity, transitivity, substitution closure, shown-variable preservation)
   without importing the core (`Formal/Syntax.lean`) and without `Classical.choice`.
2. Declarative polarity (`Ty.Pol`, `Cn.Pol`, `Prog.ClsVarOk`) and its consequence
   `Prog.ClsVarOk.super_serves`: class-parameter variances respect superclass projection.
3. Computable class-parameter variance inference (`inferTyVar`, `inferCnVar`, `inferMethodVar`,
   `inferClassVar`) together with soundness proofs (`inferTyVar_sound`, `inferCnVar_sound`,
   `inferClassVar_sound`).
-/

namespace Flick.Elab

/-! ### `Ty.Shown` and `Ty.Keeps` under renaming and substitution -/

theorem Ty.Shown.occurs {t : Ty} {n : Nat} (h : t.Shown n) : t.Occurs n := by
  induction t using Ty.ind generalizing n with
  | var m => exact h
  | con _ => exact h.elim
  | app t u iht ihu => exact Or.imp iht ihu h
  | arr φ a b q iha ihb ihq _ =>
    rcases h with h | h | h
    · exact .inl (iha h)
    · exact .inr (.inl (ihb h))
    · exact .inr (.inr (.inl (ihq h)))
  | cnil => exact h.elim
  | cun q q' ihq ihq' => exact Or.imp ihq ihq' h
  | rnm _ t iht => exact iht h
  | run r r' ihr ihr' => exact Or.imp ihr ihr' h
  | all _ t iht => exact iht h

theorem Ty.shown_ren (t : Ty) (ξ : Nat → Nat) (n : Nat) :
    (t.ren ξ).Shown n ↔ ∃ m, t.Shown m ∧ ξ m = n := by
  induction t using Ty.ind generalizing ξ n with
  | var m => simp [Ty.ren, Ty.Shown]
  | con | cnil => simp [Ty.ren, Ty.Shown]
  | app t u iht ihu | cun t u iht ihu | run t u iht ihu =>
    simp only [Ty.ren, Ty.Shown, iht, ihu, ← exists_or, ← or_and_right]
  | rnm c t iht => simp only [Ty.ren, Ty.Shown, iht]
  | arr φ a b q iha ihb ihq _ =>
    cases φ <;> simp only [Ty.ren, Ty.Shown, iha, ihb, ihq, ← exists_or, ← or_and_right]
  | all k t ih =>
    simp only [Ty.ren, Ty.Shown, ih]
    constructor
    · rintro ⟨m, hm, he⟩
      simp only [liftN] at he
      split at he
      · exact False.elim (by omega)
      · exact ⟨m - k, by rwa [Nat.sub_add_cancel (by omega)], by omega⟩
    · rintro ⟨m, hm, rfl⟩
      exact ⟨m + k, hm, by simp [liftN, show ¬ (m + k < k) by omega]⟩

theorem liftTy_shown (k : Nat) (σ : Nat → Ty) (m n : Nat) :
    (liftTy k σ m).Shown (n + k) ↔ k ≤ m ∧ (σ (m - k)).Shown n := by
  simp only [liftTy]
  split
  · simp only [Ty.Shown]
    exact ⟨fun h => False.elim (by omega), fun ⟨h, _⟩ => False.elim (by omega)⟩
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
    simp only [Ty.subst, Ty.Shown, iht, ihu, ← exists_or, ← or_and_right]
  | rnm c t iht => simp only [Ty.subst, Ty.Shown, iht]
  | arr φ a b q iha ihb ihq _ =>
    cases φ <;> simp only [Ty.subst, Ty.Shown, iha, ihb, ihq, ← exists_or, ← or_and_right]
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
  | var _ => exact h.elim
  | rnm _ t _ => exact (Ty.shown_subst_iff σ t m).2 ⟨n, h, hm⟩
  | run r r' ihr ihr' =>
    rcases h with h | h
    · exact .inl (ihr h)
    · exact .inr (ihr' h)
  | con _ => exact h.elim
  | cnil => exact h.elim
  | app t u _ _ => exact (Ty.shown_subst_iff σ (.app t u) m).2 ⟨n, h, hm⟩
  | arr φ a b q _ _ _ _ => exact (Ty.shown_subst_iff σ (.arr φ a b q) m).2 ⟨n, h, hm⟩
  | cun q q' _ _ => exact (Ty.shown_subst_iff σ (.cun q q') m).2 ⟨n, h, hm⟩
  | all k t _ => exact (Ty.shown_subst_iff σ (.all k t) m).2 ⟨n, h, hm⟩

/-! ### Nominal type spines `Ty.tcon` -/

/-- Decompose a nominal type application `T u₁ … uₙ` into `(T, [u₁, …, uₙ])`. -/
def Ty.unspine : Ty → Option (Nat × List Ty)
  | .con T => some (T, [])
  | .app f a => (Ty.unspine f).map fun p => (p.1, p.2 ++ [a])
  | _ => none

theorem Ty.unspine_foldl (us : List Ty) : ∀ (acc : Ty) (T : Nat) (vs : List Ty),
    acc.unspine = some (T, vs) → (us.foldl .app acc).unspine = some (T, vs ++ us) := by
  induction us with
  | nil => intro acc T vs h; simpa using h
  | cons u us ih =>
    intro acc T vs h
    have := ih (.app acc u) T (vs ++ [u]) (by simp [Ty.unspine, h])
    simpa using this

theorem Ty.unspine_tcon (T : Nat) (us : List Ty) : (Ty.tcon T us).unspine = some (T, us) := by
  simpa using Ty.unspine_foldl us (.con T) T [] rfl

theorem Ty.unspine_eq {t : Ty} {T : Nat} {us : List Ty} (h : t.unspine = some (T, us)) :
    t = Ty.tcon T us := by
  induction t using Ty.ind generalizing T us with
  | con c =>
    simp only [Ty.unspine, Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl⟩ := h; rfl
  | app f a ih _ =>
    simp only [Ty.unspine, Option.map_eq_some_iff] at h
    obtain ⟨⟨c, vs⟩, hf, he⟩ := h
    cases he
    rw [ih hf]
    simp [Ty.tcon, List.foldl_append]
  | _ => simp [Ty.unspine] at h

theorem Ty.tcon_eq {T T' : Nat} {us vs : List Ty} (h : Ty.tcon T us = Ty.tcon T' vs) :
    T = T' ∧ us = vs := by
  have := congrArg Ty.unspine h
  rw [Ty.unspine_tcon, Ty.unspine_tcon] at this
  simpa using this

theorem Ty.arr_ne_tcon {φ a b q T us} : Ty.arr φ a b q ≠ Ty.tcon T us := by
  intro h
  have := congrArg Ty.unspine h
  rw [Ty.unspine_tcon] at this
  simp [Ty.unspine] at this

theorem Ty.foldl_app_subst (σ : Nat → Ty) (us : List Ty) : ∀ acc : Ty,
    (us.foldl .app acc).subst σ = (us.map (·.subst σ)).foldl .app (acc.subst σ) := by
  induction us with
  | nil => intro acc; rfl
  | cons u us ih => intro acc; exact ih (.app acc u)

theorem Ty.tcon_subst (σ : Nat → Ty) (T : Nat) (us : List Ty) :
    (Ty.tcon T us).subst σ = Ty.tcon T (us.map (·.subst σ)) :=
  Ty.foldl_app_subst σ us (.con T)

theorem Ty.sizeOf_lt_foldl : ∀ {us : List Ty} {f : Ty},
    sizeOf f < sizeOf (us.foldl .app f) + 1 ∧
    ∀ {j : Nat} {u : Ty}, us[j]? = some u → sizeOf u < sizeOf (us.foldl .app f)
  | [], f => ⟨by simp, by simp⟩
  | x :: xs, f => by
    obtain ⟨h1, h2⟩ := Ty.sizeOf_lt_foldl (us := xs) (f := Ty.app f x)
    simp only [List.foldl] at h1 h2 ⊢
    have e : sizeOf (Ty.app f x) = 1 + sizeOf f + sizeOf x := by simp
    have hx : sizeOf x < sizeOf (xs.foldl Ty.app (Ty.app f x)) := by omega
    refine ⟨by omega, fun {j u} h => ?_⟩
    cases j with
    | zero => simp at h; subst h; exact hx
    | succ j => exact h2 (by simpa using h)

theorem Ty.sizeOf_lt_tcon {T : Nat} {us : List Ty} {j : Nat} {u : Ty} (h : us[j]? = some u) :
    sizeOf u < sizeOf (Ty.tcon T us) :=
  (Ty.sizeOf_lt_foldl).2 h

theorem Ty.shown_foldl_iff : ∀ {us : List Ty} {f : Ty} {n : Nat},
    (us.foldl .app f).Shown n ↔ f.Shown n ∨ ∃ (j : Nat) (u : Ty), us[j]? = some u ∧ u.Shown n
  | [], f, n => by simp
  | x :: xs, f, n => by
    rw [List.foldl_cons, Ty.shown_foldl_iff]
    simp only [Ty.Shown]
    constructor
    · rintro ((hf | hx) | ⟨j, u, hj, hu⟩)
      · exact .inl hf
      · exact .inr ⟨0, x, rfl, hx⟩
      · exact .inr ⟨j + 1, u, by simpa using hj, hu⟩
    · rintro (hf | ⟨j, u, hj, hu⟩)
      · exact .inl (.inl hf)
      · cases j with
        | zero => simp at hj; subst hj; exact .inl (.inr hu)
        | succ j => exact .inr ⟨j, u, by simpa using hj, hu⟩

theorem Ty.shown_tcon_iff {T : Nat} {us : List Ty} {n : Nat} :
    (Ty.tcon T us).Shown n ↔ ∃ (j : Nat) (u : Ty), us[j]? = some u ∧ u.Shown n := by
  rw [Ty.tcon, Ty.shown_foldl_iff]; simp [Ty.Shown]

theorem Ty.occurs_foldl_iff : ∀ {us : List Ty} {f : Ty} {n : Nat},
    (us.foldl .app f).Occurs n ↔ f.Occurs n ∨ ∃ (j : Nat) (u : Ty), us[j]? = some u ∧ u.Occurs n
  | [], f, n => by simp
  | x :: xs, f, n => by
    rw [List.foldl_cons, Ty.occurs_foldl_iff]
    simp only [Ty.Occurs]
    constructor
    · rintro ((hf | hx) | ⟨j, u, hj, hu⟩)
      · exact .inl hf
      · exact .inr ⟨0, x, rfl, hx⟩
      · exact .inr ⟨j + 1, u, by simpa using hj, hu⟩
    · rintro (hf | ⟨j, u, hj, hu⟩)
      · exact .inl (.inl hf)
      · cases j with
        | zero => simp at hj; subst hj; exact .inl (.inr hu)
        | succ j => exact .inr ⟨j, u, by simpa using hj, hu⟩

theorem Ty.occurs_tcon_iff {T : Nat} {us : List Ty} {n : Nat} :
    (Ty.tcon T us).Occurs n ↔ ∃ (j : Nat) (u : Ty), us[j]? = some u ∧ u.Occurs n := by
  rw [Ty.tcon, Ty.occurs_foldl_iff]; simp [Ty.Occurs]

end Flick.Elab
