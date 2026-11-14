import Formal.FreshEquivalence

/-!
# Fresh intermediate representations

Subtraction may remove a label whose argument has an unobservable dictionary
index. The equivalent row is syntactically fresh even when that index is not.
The runtime proof must therefore allocate the killed representation.
-/

namespace Flick

private def ghost : Ty := .rless (fun _ => True) (.rnm "Ghost" (.dict 0 (.rg 1)))

example : Ty.Equiv ghost (.caps Caps.none) :=
  .rless_drop (fun _ => True) "Ghost" (.dict 0 (.rg 1)) trivial

example : ¬ ghost.YF 0 := by
  intro h
  simp [ghost, Ty.YF, Ty.substT, kill] at h

example : ghost.YFMod 0 := by
  have h : (Ty.caps Caps.none).YF 0 := by
    simp [Ty.YF, Ty.substT]
  exact h.modulo.equiv (Ty.Equiv.rless_drop (fun _ => True) "Ghost" (.dict 0 (.rg 1)) trivial).symm

example : (ghost.substT (kill 0)).YF 0 := Ty.yf_kill 0 ghost

end Flick
