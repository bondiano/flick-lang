import Formal.Equiv.ActSim

/-!
# Contextual equivalence: the answer to a step of a thread

`TRes` is how the right side answers a step of the thread `τ` of the left one: steps that keep
the regions and at most one more (`TAns`), with related results, stores and regions; the cells
other threads are about to force stay pending (`KeepO`). An answer that keeps the regions
(`KAns`) is one (`KAns.res`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- The answer of the right side to a step of the thread `τ` of the left one. -/
def TRes (root : Roots) (W : World) (τ : Option (Nat × Nat)) (Λ₂ : List Rgn) (μ₂ : List Tm)
    (t₂ : Tm) (μ : List Tm) (Λ' : List Rgn) (μ' : List Tm) (t' : Tm) : Prop :=
  ∃ W' Λ₂' μ₂' t₂', W.le W' ∧ TAns root Λ₂ μ₂ t₂ Λ₂' μ₂' t₂' ∧ ThR W' μ' τ t' t₂' ∧
    StR W' μ' μ₂' ∧ KeepO W τ μ μ' ∧ RgsR W' μ' τ Λ' Λ₂'

theorem ThR.keep {W' : World} {τ : Option (Nat × Nat)} {μ μ' : List Tm} {t t' : Tm}
    (h : ThR W μ τ t t') (hW : W.le W')
    (hk : ∀ n e, W.O n τ → μ[n]? = some (.lazy e) → μ'[n]? = some (.lazy e)) :
    ThR W' μ' τ t t' := by
  rcases h with h | ⟨E, E', n, e, e', h₁, h₂, h₃, h₄, h₅, h₆, h₇, h₈⟩
  · exact .inl (h.mono hW)
  · exact .inr ⟨E, E', n, e, e', h₁, h₂, h₃, h₄, h₅.mono hW, hW.O _ _ h₆, hk _ _ h₆ h₇, h₈.mono hW⟩

theorem RgsR.keep {W' : World} {τ : Option (Nat × Nat)} {μ μ' : List Tm} {Λ Λ₂ : List Rgn}
    (h : RgsR W μ τ Λ Λ₂) (hW : W.le W') (hk : KeepO W τ μ μ') : RgsR W' μ' τ Λ Λ₂ := by
  refine ⟨h.1, fun p g g' hg hg' => ?_⟩
  obtain ⟨h₁, h₂, h₃, h₄, h₅⟩ := h.2 p g g' hg hg'
  exact ⟨h₁, h₂, fun j k k' hj hk' hk'' => (h₃ j k k' hj hk' hk'').keep hW
      (fun n e ho he => hk n _ e hj ho he),
    h₄, Flick.Elab.All2.mono h₅ fun _ _ r => ⟨r.1.mono hW, r.2⟩⟩

/-- An answer that keeps the regions. -/
theorem KAns.res {root : Roots} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn} {μ μ' μ₂ : List Tm}
    {t₂ t' : Tm} (h : KAns root W τ μ₂ t₂ μ μ' t') (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ' t' := by
  obtain ⟨W', μ₂', t₂', hW, hk, ht, hst, hko⟩ := h
  exact ⟨W', Λ₂, μ₂', t₂', hW, ⟨_, _, hk, .inl ⟨rfl, rfl, rfl⟩⟩, ht, hst, hko, hrg.keep hW hko⟩

end Flick.Equiv
