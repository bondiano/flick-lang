import Formal.NestScope

/-!
# Closing a region keeps `⊢ Λ`; L5
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

attribute [local instance] Classical.propDecidable

theorem inside_not_value {p : Nat} {w : Tm} : ¬ Value (.inside p w) := fun h => by cases h

theorem Cfg.at_setRgn {Λ : List Rgn} {μ : List Tm} {m : Tm} {p : Nat} {g' : Rgn}
    {i : Option (Nat × Nat)} (hi : ∀ j, i ≠ some (p, j)) :
    (⟨setRgn Λ p g', μ, m⟩ : Cfg).at i = (⟨Λ, μ, m⟩ : Cfg).at i := by
  rcases i with _ | ⟨r, j⟩
  · rfl
  · have hr : r ≠ p := fun h => hi j (by rw [h])
    apply Option.ext; intro k
    rw [Cfg.at_some, Cfg.at_some]
    simp only [setRgn, List.getElem?_set_ne (Ne.symm hr)]

theorem Cfg.at_setRgn_dead {Λ : List Rgn} {μ : List Tm} {m : Tm} {p j : Nat} {g g' : Rgn}
    (hg : Λ[p]? = some g) (hl : g'.live = false) : (⟨setRgn Λ p g', μ, m⟩ : Cfg).at (some (p, j)) = none := by
  apply Option.eq_none_iff_forall_ne_some.mpr
  intro k h
  obtain ⟨g'', h₁, h₂, -⟩ := Cfg.at_some.mp h
  simp only [setRgn, List.getElem?_set_self (getElem?_lt hg), Option.some.injEq] at h₁
  subst h₁; rw [hl] at h₂; cases h₂

end Flick
