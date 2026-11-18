import Formal.Soundness
import Formal.Elab.T6a
import Formal.Elab.Coherence
import Formal.Elab.CoreT6a
import Formal.Elab.CoreRun
import Formal.LetMut
import Formal.LetMut.LazyWitness
import Formal.Live.T4Live
import Formal.Live.T1Regions
import Formal.Live.Install
import Formal.Generalization
import Formal.Lemma4
import Formal.IndexedRows
import Formal.TypeEquivShape
import Formal.Lazy
import Formal.Refine
import Formal.Refine.Numbers
import Formal.RT.LazyCell

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

/-- info: 'Flick.T7' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T7

/-- info: 'Flick.MutOk.lzDecl' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzDecl

/-- info: 'Flick.MutOk.lzAwaitOf' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzAwaitOf

/-- info: 'Flick.MutOk.lzAwait' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzAwait

/-- info: 'Flick.MutOk.lzWrite' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzWrite

/-- info: 'Flick.MutOk.lzDeclFn' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzDeclFn

/-- info: 'Flick.MutOk.lzAwaitFn' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzAwaitFn

/-- info: 'Flick.Tm.rename_redirect' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Tm.rename_redirect

/-- info: 'Flick.Elab.Witness.lzSig' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.Witness.lzSig

/-- info: 'Flick.Ty.Equiv.substT' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Ty.Equiv.substT

/-- info: 'Flick.Ty.substT_equiv' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Ty.substT_equiv

/-- info: 'Flick.Ty.rnm_int_not_bool' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Ty.rnm_int_not_bool

/-- info: 'Flick.Ty.Equiv.row_int_not_bool' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Ty.Equiv.row_int_not_bool

/-- info: 'Flick.Ty.ref_int_not_bool' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Ty.ref_int_not_bool

/-- info: 'Flick.Ty.bound_int_not_bool' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Ty.bound_int_not_bool

/-- info: 'Flick.Ty.rless_removes_all' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Ty.rless_removes_all

/-- info: 'Flick.Ty.Equiv.kind' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Ty.Equiv.kind

/-- info: 'Flick.Lazy.L1' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Lazy.L1

/-- info: 'Flick.Lazy.L1_once' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Lazy.L1_once

/-- info: 'Flick.Lazy.L2' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Lazy.L2

/-- info: 'Flick.RT.R9' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.RT.R9

/-- info: 'Flick.Refine.sound' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Refine.sound

/-- info: 'Flick.Refine.numbers_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Refine.numbers_sound
