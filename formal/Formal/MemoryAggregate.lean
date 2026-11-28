import Formal.MemoryAggregate.Heap
import Formal.MemoryAggregate.Pass
import Formal.MemoryAggregate.RepReuse

/-!
# Abstract unboxed aggregate values: memory IR, counted heap, and insertion (#744)

Umbrella module re-exporting:
- `Formal.MemoryAggregate.Heap`: abstract aggregate value trees, `boxedTokens`,
  immediate-aggregate no-op `dup`/`drop`, borrowed `project` + nested inline
  `projectPath` + parent-drop survival, `boxAggregate`, `unboxBorrow`,
  `unboxConsume` (unique vs shared), fallback-cell round-trips, aggregate-rejecting
  `dropReuseVal`/`reuseTypedCellVal`, and `CellLayout`-checked typed-cell reuse.
- `Formal.MemoryAggregate.Pass`: representation-aware `checkRepTokens`,
  `checkRepFunction`, `annotateRep`, and `annotateRep_function_balanced`.
- `Formal.MemoryAggregate.RepReuse`: Stage 2 representation-aware `repDropToReuse`
  and `repPairReuse` skipping `.construct .aggregate` sites and preserving
  `checkRepTokens` and prefix safety.
-/

namespace Flick.Memory.Aggregate

open Flick.Memory.Pass

-- Executable regressions mirroring the Rust validation and Perceus tests.

-- Immediate aggregates have zero boxed tokens.
example :
    let immAgg : Value := .agg 1 [.imm, .agg 2 [.imm, .imm]]
    isImmediate immAgg = true ∧ boxedTokens immAgg 0 = 0 := by decide

-- Nested aggregate containing a boxed leaf counts that leaf exactly once.
example :
    let inner : Value := .agg 1 [.boxed 10, .imm]
    let outer : Value := .agg 2 [inner, .imm]
    isImmediate outer = false ∧ boxedTokens outer 10 = 1 ∧ boxedTokens inner 10 = 1 := by decide

-- Representation validator rejects explicit aggregate Reuse, tracked aggregate Reuse, and aggregate DropReuse.
example :
    let s : RepState 4 := unitRepState (fun n => n == 0) (fun n => n == 0) (fun _ => false)
    let explicitBad : RepBlock 4 := .seq (.let_ 1 (.reuse 0 .aggregate [] [])) (.ret 1)
    let trackedBad : RepBlock 4 :=
      .seq (.let_ 1 (.construct .aggregate [0] []))
        (.seq (.let_ 2 (.dropReuse 1)) (.ret 2))
    checkRepTokens explicitBad s = false ∧ checkRepTokens trackedBad s = false := by decide

-- `annotateRep` NEVER defers a dead borrowed aggregate receiver into Reuse.
example :
    let src : RepBlock 4 :=
      .seq (.let_ 1 (.borrow 0 .aggregate))
        (.seq (.let_ 2 (.construct .cell [1] [])) (.ret 2))
    let mask : Mask 4 := fun n => n == 0
    annotateRep src mask (fun _ => false) =
      some (.seq (.let_ 1 (.borrow 0 .aggregate))
        (.seq (.drop 0) (.seq (.let_ 2 (.construct .cell [1] [])) (.ret 2)))) := by decide

-- `annotateRep` NEVER rewrites `construct .aggregate` into Reuse after a cell borrow.
example :
    let src : RepBlock 4 :=
      .seq (.let_ 1 (.borrow 0 .cell))
        (.seq (.let_ 2 (.construct .aggregate [1] [])) (.ret 2))
    let mask : Mask 4 := fun n => n == 0
    annotateRep src mask (fun _ => false) =
      some (.seq (.let_ 1 (.borrow 0 .cell))
        (.seq (.drop 0) (.seq (.let_ 2 (.construct .aggregate [1] [])) (.ret 2)))) := by decide

-- `annotateRep` balances boxAggregate + unboxBorrow + unboxConsume without aggregate reuse.
example :
    let src : RepBlock 5 :=
      .seq (.let_ 1 (.construct .aggregate [0] []))
        (.seq (.let_ 2 (.boxAggregate 1))
          (.seq (.let_ 3 (.unboxBorrow 2))
            (.seq (.let_ 4 (.unboxConsume 2))
              (.ret 4))))
    let mask : Mask 5 := fun n => n == 0
    (match annotateRep src mask (fun _ => false) with
    | none => false
    | some out => checkRepTokens out (unitRepState mask mask (fun _ => false))) = true := by decide

-- Stage 2 `repPairReuse` skips `.construct .aggregate` and pairs the later `.construct .cell`.
example :
    let body : RepBlock 5 :=
      .seq (.drop 0)
        (.seq (.let_ 2 (.construct .aggregate [1] []))
          (.seq (.let_ 3 (.construct .cell [2] [])) (.ret 3)))
    let rewritten : RepBlock 5 :=
      .seq (.let_ 4 (.dropReuse 0))
        (repPairReuse 4
          (.seq (.let_ 2 (.construct .aggregate [1] []))
            (.seq (.let_ 3 (.construct .cell [2] [])) (.ret 3))))
    checkRepFunction [0, 1] body (fun _ => false) = true ∧
    checkRepFunction [0, 1] rewritten (fun _ => false) = true ∧
    checkRepFunction (width := 5) [0, 0] (.ret 0) (fun _ => false) = false := by decide

end Flick.Memory.Aggregate
