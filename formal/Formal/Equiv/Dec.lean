import Formal.Equiv.DecFrame

/-!
# Contextual equivalence: decomposition around the focus
-/

namespace Flick.Equiv

open Flick.Elab (IsAlts)

/-- **Decomposition**: the right side, caught up, around the focus of the left one. -/
theorem Sim.dec_aux {r : Tm} (hrv : ¬ Value r) (hr : ∀ f a, r = .app f a → Value f ∧ Value a)
    (hrc : ∀ l e a, r ≠ .rcons l e a) (hal : ¬ IsAlts r)
    (hlet : ∀ a b, r = .let_ a b → Value a) (hset : ∀ l a, r = .settle l a → Final a) :
    ∀ (m : Nat) {E : List Frame} {t : Tm} (W : World), E.length ≤ m → OkE E →
      Sim W .any (plug E r) t → ∀ {μ₁ : List Tm} (μ : List Tm), StR W μ₁ μ →
      (W.B → (plug E r).Closed ∧ t.Closed) → Dec W μ t E r := by
  intro m
  induction m with
  | zero =>
    intro E t W hl _ h μ₁ μ hst hcl
    cases E with
    | nil => exact h.dec_base rfl hrv hr hrc hal hlet W μ (World.le.refl _) hst hcl
    | cons => simp at hl
  | succ m ih =>
    intro E t W hl hE h μ₁ μ hst hcl
    rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
    · exact h.dec_base rfl hrv hr hrc hal hlet W μ (World.le.refl _) hst hcl
    · rw [plug_snoc] at h hcl
      have hE₀ : OkE E₀ := fun G hG => hE G (List.mem_append_left _ hG)
      have hF : F.ok := hE F (List.mem_append_right _ (List.mem_singleton_self _))
      exact Sim.dec_frame hF rfl hrv hset
        (fun W' _ μ' y hy hst' hcl' => ih W' (by simp at hl; omega) hE₀ hy μ' hst' hcl')
        (fun l E₀₀ heq W' _ μ' y hy hst' hcl' =>
          ih W' (by subst heq; simp at hl; omega)
            (fun G hG => hE₀ G (heq ▸ List.mem_append_left _ hG)) hy μ' hst' hcl')
        h rfl rfl W μ (World.le.refl _) hst hcl

theorem Sim.dec {r : Tm} (hrv : ¬ Value r) (hr : ∀ f a, r = .app f a → Value f ∧ Value a)
    (hrc : ∀ l e a, r ≠ .rcons l e a) (hal : ¬ IsAlts r)
    (hlet : ∀ a b, r = .let_ a b → Value a) (hset : ∀ l a, r = .settle l a → Final a)
    (m : Nat) {E : List Frame} {t : Tm} (W : World) (hl : E.length = m) (hE : OkE E)
    (h : Sim W .any (plug E r) t) {μ₁ : List Tm} (μ : List Tm) (hst : StR W μ₁ μ)
    (hcl : W.B → (plug E r).Closed ∧ t.Closed) : Dec W μ t E r :=
  Sim.dec_aux hrv hr hrc hal hlet hset m W (Nat.le_of_eq hl) hE h μ hst hcl

end Flick.Equiv
