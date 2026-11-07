import Formal.Elab.Syntax

/-!
# Elaboration: renaming of types, and substitution under a binder
-/

namespace Flick.Elab

theorem liftN_comp (k : Nat) (ξ ζ : Nat → Nat) (n : Nat) :
    liftN k ζ (liftN k ξ n) = liftN k (fun m => ζ (ξ m)) n := by
  simp only [liftN]
  split
  · simp [*]
  · rw [if_neg (by omega)]; congr 2; omega

theorem Ty.ren_ren (t : Ty) (ξ ζ : Nat → Nat) : (t.ren ξ).ren ζ = t.ren fun n => ζ (ξ n) := by
  induction t using Ty.ind generalizing ξ ζ with
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ <;> simp_all [Ty.ren, Stage.ren, Stage.All]
  | all k t ih => simp only [Ty.ren, ih]; congr 2; funext n; exact liftN_comp k ξ ζ n
  | _ => simp_all [Ty.ren]

theorem liftTy_liftN (k : Nat) (ξ : Nat → Nat) (θ : Nat → Ty) (n : Nat) :
    liftTy k θ (liftN k ξ n) = liftTy k (fun m => θ (ξ m)) n := by
  simp only [liftN, liftTy]
  split
  · simp [*]
  · rw [if_neg (by omega)]; congr 2; omega

theorem Ty.subst_ren (t : Ty) (ξ : Nat → Nat) (θ : Nat → Ty) :
    (t.ren ξ).subst θ = t.subst fun n => θ (ξ n) := by
  induction t using Ty.ind generalizing ξ θ with
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ <;> simp_all [Ty.ren, Ty.subst, Stage.ren, Stage.subst, Stage.All]
  | all k t ih => simp only [Ty.ren, Ty.subst, ih]; congr 2; funext n; exact liftTy_liftN k ξ θ n
  | _ => simp_all [Ty.ren, Ty.subst]

theorem Ty.ren_subst (t : Ty) (σ : Nat → Ty) (ξ : Nat → Nat) :
    (t.subst σ).ren ξ = t.subst fun n => (σ n).ren ξ := by
  induction t using Ty.ind generalizing σ ξ with
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ <;> simp_all [Ty.ren, Ty.subst, Stage.ren, Stage.subst, Stage.All]
  | all k t ih =>
    simp only [Ty.subst, Ty.ren, ih]; congr 2; funext n
    simp only [liftTy]
    split
    · simp [Ty.ren, liftN, *]
    · rw [Ty.ren_ren, Ty.ren_ren]; congr 1; funext m
      simp [liftN, show ¬ (m + k < k) by omega]
  | _ => simp_all [Ty.ren, Ty.subst]

theorem liftTy_var (k : Nat) (ξ : Nat → Nat) (n : Nat) :
    liftTy k (fun m => .var (ξ m)) n = .var (liftN k ξ n) := by
  simp only [liftTy, liftN]; split <;> simp [Ty.ren]

/-- A renaming is a substitution by variables. -/
theorem Ty.ren_eq (t : Ty) (ξ : Nat → Nat) : t.ren ξ = t.subst fun n => .var (ξ n) := by
  induction t using Ty.ind generalizing ξ with
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ <;> simp_all [Ty.ren, Ty.subst, Stage.ren, Stage.subst, Stage.All]
  | all k t ih =>
    simp only [Ty.ren, Ty.subst, ih]; congr 2; funext n; exact (liftTy_var k ξ n).symm
  | _ => simp_all [Ty.ren, Ty.subst]

theorem Ty.subst_id (t : Ty) : t.subst .var = t := by
  have h : ∀ k, liftTy k .var = .var := by
    intro k; funext n; simp only [liftTy, Ty.ren]; split
    · rfl
    · congr 1; omega
  induction t using Ty.ind with
  | arr φ a b q iha ihb ihq ihφ => cases φ <;> simp_all [Ty.subst, Stage.subst, Stage.All]
  | all k t ih => simp only [Ty.subst, h, ih]
  | _ => simp_all [Ty.subst]

theorem Ty.ren_inj {t t' : Ty} {k : Nat} (h : t.ren (· + k) = t'.ren (· + k)) : t = t' := by
  have := congrArg (Ty.subst fun n => .var (n - k)) h
  simpa [Ty.subst_ren, Ty.subst_id] using this

theorem liftTy_comp (k : Nat) (σ θ : Nat → Ty) (n : Nat) :
    (liftTy k σ n).subst (liftTy k θ) = liftTy k (fun m => (σ m).subst θ) n := by
  simp only [liftTy]
  split
  · simp [Ty.subst, liftTy, *]
  · rw [Ty.subst_ren, Ty.ren_subst]; congr 1; funext m
    simp [liftTy, show ¬ (m + k < k) by omega]

theorem Ty.subst_comp (t : Ty) (σ θ : Nat → Ty) :
    (t.subst σ).subst θ = t.subst fun n => (σ n).subst θ := by
  induction t using Ty.ind generalizing σ θ with
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ <;> simp_all [Ty.subst, Stage.subst, Stage.All]
  | all k t ih => simp only [Ty.subst, ih]; congr 2; funext n; exact liftTy_comp k σ θ n
  | _ => simp_all [Ty.subst]

theorem Ty.occurs_ren (t : Ty) (ξ : Nat → Nat) (n : Nat) :
    (t.ren ξ).Occurs n ↔ ∃ m, t.Occurs m ∧ ξ m = n := by
  induction t using Ty.ind generalizing ξ n with
  | var m => simp [Ty.ren, Ty.Occurs]
  | con | cnil => simp [Ty.ren, Ty.Occurs]
  | app t u iht ihu | cun t u iht ihu | run t u iht ihu =>
    simp only [Ty.ren, Ty.Occurs, iht, ihu]; grind
  | rnm c t iht => simp only [Ty.ren, Ty.Occurs, iht]
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ with
    | P => simp only [Ty.ren, Stage.ren, Ty.Occurs, Stage.Occurs, iha, ihb, ihq]; grind
    | R r e =>
      simp only [Ty.ren, Stage.ren, Ty.Occurs, Stage.Occurs, iha, ihb, ihq, ihφ.1, ihφ.2]; grind
  | all k t ih =>
    simp only [Ty.ren, Ty.Occurs, ih]
    constructor
    · rintro ⟨m, hm, he⟩
      simp only [liftN] at he
      split at he
      · omega
      · exact ⟨m - k, by rwa [Nat.sub_add_cancel (by omega)], by omega⟩
    · rintro ⟨m, hm, rfl⟩
      exact ⟨m + k, hm, by simp [liftN, show ¬ (m + k < k) by omega]⟩

theorem liftTy_occurs (k : Nat) (σ : Nat → Ty) (m n : Nat) :
    (liftTy k σ m).Occurs (n + k) ↔ k ≤ m ∧ (σ (m - k)).Occurs n := by
  simp only [liftTy]
  split
  · simp [Ty.Occurs]; omega
  · simp only [Ty.occurs_ren]
    constructor
    · rintro ⟨x, hx, he⟩
      have : x = n := by omega
      subst this; exact ⟨by omega, hx⟩
    · rintro ⟨_, h⟩; exact ⟨n, h, rfl⟩

theorem Ty.occurs_subst_iff (σ : Nat → Ty) (t : Ty) (n : Nat) :
    (t.subst σ).Occurs n ↔ ∃ m, t.Occurs m ∧ (σ m).Occurs n := by
  induction t using Ty.ind generalizing σ n with
  | var m => simp [Ty.subst, Ty.Occurs]
  | con | cnil => simp [Ty.subst, Ty.Occurs]
  | app t u iht ihu | cun t u iht ihu | run t u iht ihu =>
    simp only [Ty.subst, Ty.Occurs, iht, ihu]; grind
  | rnm c t iht => simp only [Ty.subst, Ty.Occurs, iht]
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ with
    | P => simp only [Ty.subst, Stage.subst, Ty.Occurs, Stage.Occurs, iha, ihb, ihq]; grind
    | R r e =>
      simp only [Ty.subst, Stage.subst, Ty.Occurs, Stage.Occurs, iha, ihb, ihq, ihφ.1, ihφ.2]; grind
  | all k t ih =>
    simp only [Ty.subst, Ty.Occurs, ih, liftTy_occurs]
    constructor
    · rintro ⟨m, hm, hk, h⟩
      exact ⟨m - k, by rwa [Nat.sub_add_cancel hk], h⟩
    · rintro ⟨m, hm, h⟩
      exact ⟨m + k, hm, by omega, by simpa using h⟩

end Flick.Elab
