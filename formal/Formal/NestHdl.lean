import Formal.NestStep

/-!
# The steps of handlers keep `⊢ Λ`: the frames they move and the cells they add

A step of a handler replaces its redex by one with the same `inside` forms: a parked
body goes back under its handler, an arm waits beside the resumed body. A new cell of
a handler or of `resume` holds `lam todo`, which no term names as a location.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem lam_todo_sites : (Tm.lam .todo).NoIns ∧ (∀ p, ¬ (Tm.lam .todo).Has (.rgn p)) ∧
    ∀ l, ¬ (Tm.lam .todo).Has (.loc l) := by
  refine ⟨?_, ?_, ?_⟩ <;> simp [Tm.NoIns, Tm.Has]

end Flick
