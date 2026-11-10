import Formal.Soundness
import Formal.Elab.T6a
import Formal.Elab.Coherence
import Formal.Elab.CoreT6a
import Formal.Elab.CoreRun
import Formal.Live.T4Live
import Formal.Live.T1Regions
import Formal.Live.Install
import Formal.Generalization
import Formal.Lemma4

/-!
# The axioms of the main theorems

The build fails when a main theorem rests on more than the standard axioms
of Lean: a `sorry` (`sorryAx`) or a new `axiom` changes what `#print axioms`
prints, and `#guard_msgs` turns the difference into an error.
-/

/-- info: 'Flick.T1' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T1

/-- info: 'Flick.T4'' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T4'

/-- info: 'Flick.T1_regions' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T1_regions

/-- info: 'Flick.T2a' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T2a

/-- info: 'Flick.T2b' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T2b

/-- info: 'Flick.T2b_await' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T2b_await

/-- info: 'Flick.T3' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T3

/-- info: 'Flick.lemma_6' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.lemma_6

/-- info: 'Flick.hdl_err_live' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.hdl_err_live

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
