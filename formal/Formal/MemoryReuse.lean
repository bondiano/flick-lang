import Formal.MemoryReuse.Pass
import Formal.MemoryReuse.Heap

/-!
# Frame-limited FBIP reuse and dynamic alias/cycle invariants (phase M3)

Umbrella module re-exporting:
- `Formal.MemoryReuse.Pass`: whole-block token-balance preservation and prefix
  safety for `DropReuse` + `pairReuse` across arbitrary finite local width and
  branch nesting (`pairReuse_preserves_checkTokens`,
  `dropToReuse_preserves_checkTokens`, `dropToReuse_prefix_safe`).
- `Formal.MemoryReuse.Heap`: abstract counted-heap uniqueness, alias
  preservation, unused-token cleanup, and `Ref` cycle-break vs. teardown
  theorems (`dropReuseHeap_unique_tagged`,
  `dropReuseHeap_shared_preserves_aliases`, `reuseTaggedHeap_unique_in_place`,
  `reuseTaggedHeap_null_preserves_aliases`,
  `updateRecordHeap_shared_preserves_aliases`, `drop_unused_reuseToken_clean`,
  `cycle_break_vs_teardown`).
-/
