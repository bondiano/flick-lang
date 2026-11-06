import Formal.Soundness
import Formal.Elab.T6a
import Formal.Elab.Coherence
import Formal.Elab.CoreT6a
import Formal.Elab.CoreRun

/-!
# The axioms of the main theorems

The build fails when a main theorem rests on more than the standard axioms
of Lean: a `sorry` (`sorryAx`) or a new `axiom` changes what `#print axioms`
prints, and `#guard_msgs` turns the difference into an error.
-/

/-- info: 'Flick.T1' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T1

/-- info: 'Flick.Elab.T6a' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.T6a

/-- info: 'Flick.Elab.T6b' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.T6b

/-- info: 'Flick.Elab.T6a_core' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.T6a_core

/-- info: 'Flick.Elab.T6b_core' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.T6b_core
