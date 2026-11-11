import Formal.Live.LNestKept

/-!
# The run-time forms of a pure redex and of a frame left by `raise` or `unw`
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {anc : Nat → Nat → Prop}

/-- A pure redex holds no run-time form, but for the forms of a handler. -/
theorem PHead.noInsF {r r' : Tm} {A : Nat → Prop} (hh : PHead r r') (h : r.Fw Ψ anc A)
    (hd : ¬ r.IsHdl) : r.NoIns := by
  have vn : ∀ {x : Tm} {A}, Value x → x.Fw Ψ anc A → ¬ x.Has .ins := fun hv hx => hv.noInsF hx
  cases hh <;> simp only [Tm.Fw] at h <;> simp only [Tm.NoIns, Tm.Has, not_or] <;>
    first
    | exact (hd trivial).elim
    | exact ⟨h.1.2, vn ‹_› h.2.1⟩
    | exact ⟨vn ‹_› h.1, h.2.2⟩
    | exact ⟨h.2.2.1, h.2.2.2⟩
    | exact vn ‹_› h
    | exact ⟨vn ‹_› h.1, vn ‹_› h.2.1⟩
    | exact ⟨h.2.2, vn ‹_› h.2.1⟩
    | (have h3 := h.2.2; simp only [Tm.NoIns, Tm.Has, not_or] at h3; exact ⟨vn ‹_› h.1, h3⟩)
    | simp

theorem Frame.Fw.noFrame_plug {F : Frame} {A : Nat → Prop} {x : Tm} (hF : F.Fw Ψ anc A)
    (hni : ¬ F.isInside) (hnc : ¬ F.isCtl) (hx : ∀ q, ¬ x.Has (.frame q)) :
    ∀ q, ¬ (F.plug x).Has (.frame q) := by
  have vn : ∀ {y : Tm} {A}, Value y → y.Fw Ψ anc A → ¬ y.Has .ins := fun hv hy => hv.noInsF hy
  intro q h
  cases F <;> simp only [Frame.Fw, Frame.isInside, Frame.isCtl] at hF hni hnc <;>
    simp only [Frame.plug, Tm.Has] at h <;>
    first
    | exact hni trivial
    | exact hnc trivial
    | grind [Tm.has_frame_ins, Tm.NoIns]
    | exact hx q h
    | (rcases h with h | h
       · exact hx q h
       · first | exact hF.2 (Tm.has_frame_ins h) | exact vn hF.2 hF.1 (Tm.has_frame_ins h))
    | (rcases h with h | h
       · first | exact hF.2 (Tm.has_frame_ins h) | exact vn hF.2 hF.1 (Tm.has_frame_ins h)
       · exact hx q h)
    | (rcases h with h | h | h | h
       · cases h
       · cases h
       · exact hF.1 (Tm.has_frame_ins h)
       · exact hx q h)

end Flick
