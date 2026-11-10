import Formal.NestRedex

/-!
# Creating and closing a region keeps `⊢ Λ`; L5

A new region `π` gets as ancestors the regions of the place of its `scope` and
itself; its home is the thread of the `scope`. Closing `π` drops it from what live
work may name: no live region but `π` has `π` among its ancestors (`Nest.nested`,
L5), since such a region has its `inside` form in the body of `π` or in a child
of a region inside `π`, and these are finished.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Cfg.at_append {Λ : List Rgn} {μ : List Tm} {m : Tm} (i : Option (Nat × Nat)) :
    (⟨Λ ++ [Rgn.new], μ, m⟩ : Cfg).at i = (⟨Λ, μ, m⟩ : Cfg).at i := by
  rcases i with _ | ⟨r, j⟩
  · rfl
  · apply Option.ext; intro k
    rw [Cfg.at_some, Cfg.at_some]
    simp only
    by_cases hr : r < Λ.length
    · rw [List.getElem?_append_left hr]
    · rw [List.getElem?_eq_none (show Λ.length ≤ r by omega)]
      constructor
      · rintro ⟨g, hg, -, hk⟩
        rw [List.getElem?_append_right (by omega)] at hg
        cases hj : r - Λ.length with
        | zero => rw [hj] at hg; simp at hg; subst hg; simp [Rgn.new] at hk
        | succ n => rw [hj] at hg; simp at hg
      · rintro ⟨g, hg, -⟩; cases hg

end Flick
