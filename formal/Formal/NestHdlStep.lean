import Formal.NestHdlRt

/-!
# The steps of handlers in a context keep `⊢ Λ`

An operation runs its arm at the handler, the argument of a type without regions; the
arm resumes the body under the handler, and waits beside it; a new handler is a new
region around its body, with a new cell.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Cfg.at_mem {c : Cfg} {μ' : List Tm} (i : Option (Nat × Nat)) :
    (⟨c.rgs, μ', c.main⟩ : Cfg).at i = c.at i := by
  rcases i with _ | ⟨r, j⟩ <;> rfl

theorem park_frames {p ℓ κ ℓ' : Nat} {tl : Bool} {Ef : Name} {al rf a v f : Tm} {Ei : List Frame}
    (han : a.NoIns) (hf : ∀ q, ¬ (Tm.app f v).Has (.frame q)) (q : Nat) :
    (Tm.hctl p ℓ κ tl Ef al rf a (plug Ei (.hole p))).Has (.frame q) ↔
      (Tm.hwith p ℓ' Ef al rf (plug Ei (.app f v))).Has (.frame q) := by
  have h₅ := plug_frame_iff (E := Ei) (x := .hole p) (y := .app f v)
    (fun q => ⟨fun h => (hole_frames h).elim, fun h => (hf q h).elim⟩) q
  have h₄ := han.frame q
  simp only [Tm.Has] at ⊢; grind

end Flick
