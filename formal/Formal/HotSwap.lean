import Formal.HotSwap.Compat
import Formal.HotSwap.Transfer
import Formal.HotSwap.Migrate
import Formal.HotSwap.Session

/-!
# Hot swap and typed migrations (`docs/formal/hot-swap.md`)

Models a live session over λFlick:
- global cells in `Sig` / `Roots` whose code can be replaced via a compatible
  `swap` (`SchemeSubsumes`: the new scheme subsumes the old one, with effect
  rows no wider and region/capture qualifiers respecting live subtyping);
- declared state cells in the store `μ` that can be transformed atomically via
  pure `Old -> New` `migration` functions (`MigrateStep`): committing extends
  the store typing `Ψ` with the new versions while keeping old cells intact for
  suspended frames, whereas a failing or diverging migration rolls back
  atomically leaving the entire configuration unchanged;
- arbitrary interleaved execution, compatible swaps, and migrations
  (`SessionOk`), proving that:
  1. compatible swaps preserve configuration typing (`swap_preserves_cfgTy`);
  2. well-typed migrations preserve configuration typing and are atomic on
     failure/divergence (`migration_preserves_cfgTy`,
     `migration_rollback_atomic`);
  3. progress is unaffected and old frames remain well typed against the store
     typing extended with both versions
     (`old_frame_well_typed_under_extended_store`,
     `hotSwap_progress_unaffected`).
-/
