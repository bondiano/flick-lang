import Formal.NestHdl

/-!
# The pure steps of handlers keep `⊢ Λ`

The value of a body leaves its handler; the value of an arm goes back into the parked
body, or the parked body unwinds to its handler; the value of a resumed body goes
into the arm, past the `inside` forms the arm opened, which are younger than what the
handler gives back.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem plug_frame_iff {E : List Frame} {x y : Tm} (h : ∀ q, x.Has (.frame q) ↔ y.Has (.frame q)) (q : Nat) :
    (plug E x).Has (.frame q) ↔ (plug E y).Has (.frame q) := by
  rw [plug_has_iff, plug_has_iff (x := y), h]

theorem hole_frames {p q : Nat} : ¬ (Tm.hole p).Has (.frame q) := by simp [Tm.Has]

end Flick
