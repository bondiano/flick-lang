import Formal.NestPut

/-!
# A step at the redex keeps `⊢ Λ`

A value moves from one place to another (from a cell to the redex, from the redex
to a cell, out of a closing region, to a new child) and stays at its place when
the two places agree on what its type shows (`Tm.Within.move`).
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem plug_frame {q : Nat} {x : Tm} : ∀ {E : List Frame}, .inside q ∈ E → (plug E x).Has (.frame q)
  | F :: E, h => by
    rcases List.mem_cons.mp h with rfl | h
    · exact plug_has E (by simp [Frame.plug, Tm.Has])
    · exact plug_frame (x := F.plug x) h

theorem plug_frames_iff {E : List Frame} {x : Tm} {q : Nat} (hx : ∀ q, ¬ x.Has (.frame q)) :
    (plug E x).Has (.frame q) ↔ (plug E (.var 0)).Has (.frame q) :=
  ⟨fun h => (plug_has_inv E h).resolve_right (hx q), plug_of_hole⟩

end Flick
