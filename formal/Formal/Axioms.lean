import Formal.Soundness
import Formal.Elab.T6a
import Formal.Elab.Coherence
import Formal.Elab.CoreT6a
import Formal.Elab.CoreRun
import Formal.Elab.ClassVariance
import Formal.LetMut
import Formal.LetMut.LazyWitness
import Formal.LetMut.LazyOnce
import Formal.Live.T4Live
import Formal.Live.T1Regions
import Formal.Live.Install
import Formal.Generalization
import Formal.Lemma4
import Formal.IndexedRows
import Formal.InstanceHandlers
import Formal.TypeEquivShape
import Formal.Lazy
import Formal.Refine
import Formal.Refine.Numbers
import Formal.FixedWidth
import Formal.Proofs.CheckSound
import Formal.RT.LazyCell
import Formal.Equiv.Context
import Formal.Equiv.All
import Formal.RecordOrder
import Formal.MemoryArena
import Formal.MemoryScopes
import Formal.MemoryReuse
import Formal.MemoryAggregate

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

/-- info: 'Flick.Elab.inferClassVar_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.inferClassVar_sound

/-- info: 'Flick.Elab.Prog.ClsVarOk.super_serves' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.Prog.ClsVarOk.super_serves

/-- info: 'Flick.Elab.dict_transport' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.dict_transport

/-- info: 'Flick.Elab.closed_unique_common' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.closed_unique_common

/-- info: 'Flick.Elab.Cn.Serves.tr' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.Cn.Serves.tr

/-- info: 'Flick.Elab.has_dictLe' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.has_dictLe

/-- info: 'Flick.Elab.T6a_safeUse' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.T6a_safeUse

/-- info: 'Flick.T7' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T7

/-- info: 'Flick.MutOk.lzDecl' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzDecl

/-- info: 'Flick.LzSig.typ_run' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.LzSig.typ_run

/-- info: 'Flick.MutOk.lzAwait' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzAwait

/-- info: 'Flick.MutOk.lzWrite' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MutOk.lzWrite

/-- info: 'Flick.Elab.Witness.lzSig' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Elab.Witness.lzSig

/-- info: 'Flick.lzOnce' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.lzOnce

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

/-- info: 'Flick.Row.hBody_store_bool' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Row.hBody_store_bool

/-- info: 'Flick.Row.hBody_var_without' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Row.hBody_var_without

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

/-- info: 'Flick.RT.R9_again' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.RT.R9_again

/-- info: 'Flick.RT.R9_owner' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.RT.R9_owner

/-- info: 'Flick.Refine.sound' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Refine.sound

/-- info: 'Flick.Refine.numbers_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Refine.numbers_sound

/-- info: 'Flick.PT.fundamental' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.PT.fundamental

/-- info: 'Flick.PT.terminates' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.PT.terminates

/-- info: 'Flick.PT.check_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.PT.check_sound

/-- info: 'Flick.PT.checkAll_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.PT.checkAll_sound

/-- info: 'Flick.Equiv.o1a' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o1a

/-- info: 'Flick.Equiv.o1b' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o1b

/-- info: 'Flick.Equiv.o1c' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o1c

/-- info: 'Flick.Equiv.o1a_all' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o1a_all

/-- info: 'Flick.Equiv.o1c_all' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o1c_all

/-- info: 'Flick.Equiv.o2a_all' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o2a_all

/-- info: 'Flick.Equiv.EverySim.has' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.EverySim.has

/-- info: 'Flick.Equiv.o4' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o4

/-- info: 'Flick.Equiv.o4_open' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.o4_open

/-- info: 'Flick.Equiv.PSteps.det' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Equiv.PSteps.det

/-- info: 'Flick.Typ.pure_eval' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Typ.pure_eval

/-- info: 'Flick.RecEquiv.setField' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.RecEquiv.setField

/-- info: 'Flick.RecEquiv.matchP' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.RecEquiv.matchP

/-- info: 'Flick.elabSwap_canonical' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.elabSwap_canonical

/-- info: 'Flick.MemoryArena.close_balance' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryArena.close_balance

/-- info: 'Flick.MemoryScopes.close_selected' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.close_selected

/-- info: 'Flick.MemoryScopes.closeLifecycle_lifo_order' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.closeLifecycle_lifo_order

/-- info: 'Flick.MemoryScopes.closeLifecycle_selected' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.closeLifecycle_selected

/-- info: 'Flick.MemoryScopes.closeLifecycle_idempotent' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.closeLifecycle_idempotent

/-- info: 'Flick.Memory.Reuse.pairReuse_preserves_checkTokens' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.pairReuse_preserves_checkTokens

/-- info: 'Flick.Memory.Reuse.dropToReuse_preserves_checkTokens' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.dropToReuse_preserves_checkTokens

/-- info: 'Flick.Memory.Reuse.dropToReuse_prefix_safe' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.dropToReuse_prefix_safe

/-- info: 'Flick.Memory.Reuse.dropReuseHeap_unique_tagged' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.dropReuseHeap_unique_tagged

/-- info: 'Flick.Memory.Reuse.dropReuseHeap_shared_preserves_aliases' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.dropReuseHeap_shared_preserves_aliases

/-- info: 'Flick.Memory.Reuse.reuseTaggedHeap_unique_in_place' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.reuseTaggedHeap_unique_in_place

/-- info: 'Flick.Memory.Reuse.reuseTaggedHeap_null_preserves_aliases' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.reuseTaggedHeap_null_preserves_aliases

/-- info: 'Flick.Memory.Reuse.updateRecordHeap_shared_preserves_aliases' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.updateRecordHeap_shared_preserves_aliases

/-- info: 'Flick.Memory.Reuse.drop_unused_reuseToken_clean' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.drop_unused_reuseToken_clean

/-- info: 'Flick.Memory.Reuse.cycle_break_vs_teardown' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Reuse.cycle_break_vs_teardown

/-- info: 'Flick.Memory.Aggregate.dupValue_adds_boxedTokens' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.dupValue_adds_boxedTokens

/-- info: 'Flick.Memory.Aggregate.dupValue_immediate_noop' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.dupValue_immediate_noop

/-- info: 'Flick.Memory.Aggregate.dropValue_subtracts_boxedTokens' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.dropValue_subtracts_boxedTokens

/-- info: 'Flick.Memory.Aggregate.dropValue_immediate_noop' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.dropValue_immediate_noop

/-- info: 'Flick.Memory.Aggregate.projectAgg_retains_field' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.projectAgg_retains_field

/-- info: 'Flick.Memory.Aggregate.projectAgg_survives_parent_drop' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.projectAgg_survives_parent_drop

/-- info: 'Flick.Memory.Aggregate.boxAggregate_conserves_tokens' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.boxAggregate_conserves_tokens

/-- info: 'Flick.Memory.Aggregate.unboxBorrow_retains_without_releasing' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.unboxBorrow_retains_without_releasing

/-- info: 'Flick.Memory.Aggregate.unboxConsume_unique_transfers' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.unboxConsume_unique_transfers

/-- info: 'Flick.Memory.Aggregate.unboxConsume_shared_retains_preserves_aliases' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.unboxConsume_shared_retains_preserves_aliases

/-- info: 'Flick.Memory.Aggregate.reuseTypedCell_incompatible_layout_fallback' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.reuseTypedCell_incompatible_layout_fallback

/-- info: 'Flick.Memory.Aggregate.reuseTypedCell_compatible_in_place' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.reuseTypedCell_compatible_in_place

/-- info: 'Flick.Memory.Aggregate.checkRepTokens_rejects_explicit_agg_reuse' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.checkRepTokens_rejects_explicit_agg_reuse

/-- info: 'Flick.Memory.Aggregate.checkRepTokens_rejects_tracked_agg_reuse' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.checkRepTokens_rejects_tracked_agg_reuse

/-- info: 'Flick.Memory.Aggregate.checkRepTokens_rejects_agg_dropReuse' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.checkRepTokens_rejects_agg_dropReuse

/-- info: 'Flick.Memory.Aggregate.checkRepTokens_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.checkRepTokens_sound

/-- info: 'Flick.Memory.Aggregate.checkRepTokens_prefix_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.checkRepTokens_prefix_safe

/-- info: 'Flick.Memory.Aggregate.annotateRep_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.annotateRep_balanced

/-- info: 'Flick.Memory.Aggregate.annotateRep_prefix_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.annotateRep_prefix_safe

/-- info: 'Flick.FixedWidth.wrap_inRange' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.FixedWidth.wrap_inRange

/-- info: 'Flick.FixedWidth.wrap_neg_add_cancel' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.FixedWidth.wrap_neg_add_cancel

/-- info: 'Flick.FixedWidth.safeDiv_min_neg_one' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.FixedWidth.safeDiv_min_neg_one

/-- info: 'Flick.FixedWidth.safeRem_min_neg_one' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.FixedWidth.safeRem_min_neg_one

/-- info: 'Flick.FixedWidth.checkedDiv_sound' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.FixedWidth.checkedDiv_sound

/-- info: 'Flick.FixedWidth.lossless_wrap_eq' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.FixedWidth.lossless_wrap_eq

/-- info: 'Flick.FixedWidth.ordNaNLast_greatest' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.FixedWidth.ordNaNLast_greatest
