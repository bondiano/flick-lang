import Formal.MemoryArena
import Formal.MemoryScopes
import Formal.MemoryPass
import Formal.MemoryInsert
import Formal.MemoryReuse
import Formal.MemoryAggregate
import Formal.MemorySoundness

/-!
# The axioms of the memory theorems

Split from `Formal/Axioms.lean` to keep each file focused; imported by
`Formal.Axioms` so `lake build --wfail` verifies every `#guard_msgs` pin.
-/


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

/-- info: 'Flick.Memory.Pass.checkFunction_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkFunction_sound

/-- info: 'Flick.Memory.Pass.checkFunction_prefix_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkFunction_prefix_safe

/-- info: 'Flick.Memory.Insert.annotate_function_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotate_function_balanced

/--
info: 'Flick.Memory.Aggregate.projectPath_retains_and_survives_parent_drop' depends on axioms: [propext,
 Classical.choice,
 Quot.sound]
-/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.projectPath_retains_and_survives_parent_drop

/-- info: 'Flick.Memory.Aggregate.boxAggregate_then_drop_releases_descendants' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.boxAggregate_then_drop_releases_descendants

/-- info: 'Flick.Memory.Aggregate.boxAggregate_then_unboxConsume_unique_identity' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.boxAggregate_then_unboxConsume_unique_identity

/-- info: 'Flick.Memory.Aggregate.dropReuseVal_rejects_aggregate' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.dropReuseVal_rejects_aggregate

/-- info: 'Flick.Memory.Aggregate.reuseTypedCellVal_rejects_aggregate' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.reuseTypedCellVal_rejects_aggregate

/-- info: 'Flick.Memory.Aggregate.reuseTypedCellVal_live_tagged_compatible_in_place' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.reuseTypedCellVal_live_tagged_compatible_in_place

/-- info: 'Flick.Memory.Aggregate.annotateRep_function_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.annotateRep_function_balanced

/--
info: 'Flick.Memory.Aggregate.repPairReuse_preserves_checkRepTokens' depends on axioms: [propext,
 Classical.choice,
 Quot.sound]
-/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.repPairReuse_preserves_checkRepTokens

/--
info: 'Flick.Memory.Aggregate.repDropToReuse_preserves_checkRepTokens' depends on axioms: [propext,
 Classical.choice,
 Quot.sound]
-/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.repDropToReuse_preserves_checkRepTokens

/-- info: 'Flick.Memory.Aggregate.repDropToReuse_prefix_safe' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Aggregate.repDropToReuse_prefix_safe

/-- info: 'Flick.Memory.Soundness.zero_roots_empty_heap' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Soundness.zero_roots_empty_heap

/-- info: 'Flick.Memory.Soundness.evalMemPrefix_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Soundness.evalMemPrefix_sound

/-- info: 'Flick.Memory.Soundness.evalMem_preserves_balance_and_denote' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Soundness.evalMem_preserves_balance_and_denote

/-- info: 'Flick.Memory.Soundness.pipeline_source_equiv_and_balanced' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Soundness.pipeline_source_equiv_and_balanced

/-- info: 'Flick.Memory.Soundness.unwind_frame_and_scope_balanced' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Soundness.unwind_frame_and_scope_balanced

/--
info: 'Flick.Memory.Soundness.close_owner_with_late_arenas_balanced' depends on axioms: [propext,
 Classical.choice,
 Quot.sound]
-/
#guard_msgs in
#print axioms Flick.Memory.Soundness.close_owner_with_late_arenas_balanced

/-- info: 'Flick.Memory.Soundness.teardown_zero_live_objects' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Soundness.teardown_zero_live_objects
