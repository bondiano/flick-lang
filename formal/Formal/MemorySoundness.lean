import Formal.MemorySoundness.Unwind

/-!
# Operational Memory Soundness, Aggregate Balance, and Unwind (#463)

Umbrella module re-exporting:
- `Formal.MemorySoundness.Sem`: pure semantic values `SemVal`, acyclic
  denotation `Denotes`, operational memory IR `MemOp`/`MemBlock`, and erased
  source evaluation `evalSource`;
- `Formal.MemorySoundness.HeapInv`: global reference-count conservation
  `HeapBalanced`, `zero_roots_empty_heap`, unified `Denotes` preservation, and
  counted primitive step lemmas (including recursive drop, unboxed aggregates,
  `BoxAggregate`/`Unbox*`, `DropReuse`, and Stage 1 / Stage 2 `CellLayout`-checked
  typed-cell reuse);
- `Formal.MemorySoundness.Exec`: `evalMemPrefix_sound`,
  `evalMem_preserves_balance_and_denote`, and `pipeline_source_equiv_and_balanced`;
- `Formal.MemorySoundness.Unwind`: `unwind_frame_and_scope_balanced`,
  `close_owner_with_late_arenas_balanced`, and `teardown_zero_live_objects`.

**Implementation-proof boundary:** These theorems prove properties of the
abstract memory IR, counted heap, aggregate, reuse, scope, and arena model in
Lean. They do NOT certify the Rust compiler (`flick-core`), the Rust tier-0 VM
(`flick-vm`), or the JS/Effect-TS/Go/future native runtimes.
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate

-- Executable regression: nested inline aggregate construction, nested inline
-- `projectPath`, parent drop, `boxAggregate`, `unboxConsume`, and return.
example :
    let lay : CellLayout := ⟨1, 8, 1⟩
    let blk : MemBlock 7 :=
      .seq (.let_ 1 (.construct .aggregate 10 lay [0] []))
        (.seq (.let_ 2 (.construct .aggregate 20 lay [1] []))
          (.seq (.let_ 3 (.projectPath 2 [0, 0]))
            (.seq (.drop 2)
              (.seq (.let_ 4 (.boxAggregate 3 lay))
                (.seq (.let_ 5 (.unboxConsume 4))
                  (.ret 5))))))
    let st : RepState 7 := unitRepState (fun n => n == 0) (fun n => n == 0) (fun _ => false)
    checkRepTokens (toRepBlock blk) st = true := by decide

-- Executable regression: Stage 2 `memPairReuse` skips `.construct .aggregate`
-- and pairs the subsequent `.construct .cell` while preserving `checkRepTokens`.
example :
    let lay : CellLayout := ⟨1, 8, 1⟩
    let next : MemBlock 5 :=
      .seq (.let_ 2 (.construct .aggregate 10 lay [1] []))
        (.seq (.let_ 3 (.construct .cell 20 lay [2] []))
          (.ret 3))
    let rewritten : MemBlock 5 := .seq (.let_ 4 (.dropReuse 0)) (memPairReuse 4 next)
    let st : RepState 5 := unitRepState (fun n => [0, 1].contains n)
      (fun n => [0, 1].contains n) (fun _ => false)
    checkRepTokens (toRepBlock (.seq (.drop 0) next)) st = true ∧
    checkRepTokens (toRepBlock rewritten) st = true := by decide

end Flick.Memory.Soundness
