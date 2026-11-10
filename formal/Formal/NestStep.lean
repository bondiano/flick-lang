import Formal.NestClose

/-!
# A step keeps `⊢ Λ`
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

/-- The run-time forms of a handler, whose pure steps leave the region of the handler. -/
def Tm.IsHdl : Tm → Prop
  | .hwith .. | .hctl .. | .hres .. | .hdone .. | .hkeep .. | .hkarm .. => True
  | _ => False

theorem Cfg.at_grow {Λ : List Rgn} {μ : List Tm} {m : Tm} {p : Nat} {g g' : Rgn} {i e}
    (hg : Λ[p]? = some g) (hl : g'.live = g.live) (hk : ∃ ks, g'.kids = g.kids ++ ks)
    (h : (⟨Λ, μ, m⟩ : Cfg).at i = some e) : (⟨setRgn Λ p g', μ, m⟩ : Cfg).at i = some e := by
  rcases i with _ | ⟨r, j⟩
  · exact h
  · obtain ⟨g'', h₁, h₂, h₃⟩ := Cfg.at_some.mp h
    apply Cfg.at_some.mpr
    by_cases hr : r = p
    · subst hr; rw [hg] at h₁; cases h₁
      obtain ⟨ks, hks⟩ := hk
      exact ⟨g', by simp [setRgn, List.getElem?_set_self (getElem?_lt hg)], hl.trans h₂, by
        rw [hks, List.getElem?_append_left (getElem?_lt h₃)]; exact h₃⟩
    · exact ⟨g'', by rw [setRgn, List.getElem?_set_ne (Ne.symm hr)]; exact h₁, h₂, h₃⟩

theorem Frame.hole_eq {F : Frame} {A : Nat → Prop} (h : ¬ F.isInside) : F.hole A = A := by
  cases F <;> simp_all [Frame.hole, Frame.isInside]

end Flick
