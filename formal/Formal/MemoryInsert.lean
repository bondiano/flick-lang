import Formal.MemoryInsert.Balance

/-!
# Ownership insertion helpers (phase M)

This module projects `flick-core/src/opt/perceus.rs` before its final
validator call. Locals range over an arbitrary finite universe; enumeration
is numeric, matching the ordered Rust maps/sets. The token model and source
analysis are those of `MemoryPass`, not assumptions about accepted output.

The executable annotation below preserves the one-instruction dead-borrow
deferral: only an immediately following source constructor becomes reuse.
Entry trimming happens once per original block, not once per sequential
node. Branch successors independently trim the same logical owned set;
the selected runtime branch does not clone its owned tokens.

`annotate_balanced` proves whole-block token balance of this projection
before any final validation call. Computed source liveness, source binding
validity and the owner/definition inventory establish the pending-aware
induction; successful output validation is not a premise. The connection
to Rust, machine-counter bounds and heap/value/reuse semantics remain
separate obligations.
-/


namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

-- The theorem accepts sibling branches reusing the same fresh result
-- name; it does not quietly strengthen source validity to global freshness.
example :
    let block : Block 4 := .branch 0 (.seq (.let_ 1 (.consume [])) (.ret 1))
      (.seq (.let_ 1 (.consume [])) (.ret 1))
    let defined : Mask 4 := fun name => name == 0
    ∃ output, annotate block defined = some output ∧
      checkTokens output (unitState defined defined) = true := by
  dsimp only
  apply annotate_defined_balanced
  decide

-- The general theorem, not just a finite output check, covers the pending
-- receiver whose dead borrow result drops before the next constructor.
example :
    let block : Block 4 := .seq (.let_ 1 (.borrow 0))
      (.seq (.let_ 2 (.construct [] [])) (.ret 2))
    let defined : Mask 4 := fun name => name == 0
    ∃ output, annotate block defined = some output ∧
      checkTokens output (unitState defined defined) = true := by
  dsimp only
  apply annotate_defined_balanced
  decide

example (choices : List Bool) :
    let block : Block 4 := .branch 0 (.ret 0) (.seq (.let_ 1 (.consume [])) (.ret 1))
    let defined : Mask 4 := fun name => name == 0
    ∃ output, annotate block defined = some output ∧
      ∀ before after, path output choices = before ++ after →
        ∃ middle, runEvents before (unitState defined defined) = some middle := by
  dsimp only
  apply annotate_prefix_safe
  · decide
  · exact checkSource_live_defined _ _ (by decide)
  · intro name present
    exact present

example :
    let parameters : List (Local 4) := [0]
    let block : Block 4 := .branch 0 (.seq (.let_ 1 (.consume [])) (.ret 1))
      (.seq (.let_ 1 (.consume [])) (.ret 1))
    ∃ output, annotate block (parameterMask parameters) = some output ∧
      checkFunction parameters output = true := by
  dsimp only
  apply annotate_function_balanced
  · decide
  · decide

-- The mask alone cannot stand in for the uniqueness premise: duplicate
-- parameters are still rejected by the independent function checker.
example : annotate (width := 4) (.ret 0) (parameterMask [0, 0]) = some (.ret 0) ∧
    checkFunction (width := 4) [0, 0] (.ret 0) = false := by decide

private def inventory (state : State width) : List (Nat × Bool) :=
  (List.finRange width).map (fun name => (state.tokens name, state.defined name))

-- A consumed local retained for the continuation needs all its consuming
-- copies; an unrelated incoming owner remains untouched by this iteration.
example :
    let owned : Mask 5 := fun name => [0, 1, 2].contains name
    let after : Mask 5 := fun name => name == 0
    let plan := finishInstruction 3 (.consume [0, 0, 1]) after false owned
    (runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory =
      some [(1, true), (0, true), (1, true), (0, true), (0, false)] := by decide

-- Ordinary fields and evidence both participate, including shared locals.
example :
    let owned : Mask 5 := fun name => [0, 1].contains name
    let after : Mask 5 := fun name => [1, 3].contains name
    let plan := finishInstruction 3 (.construct [0] [0, 1]) after false owned
    (runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory =
      some [(0, true), (1, true), (0, false), (1, true), (0, false)] := by decide

-- Deferred receiver ownership survives a dead newly bound borrow result.
example :
    let owned : Mask 5 := fun name => name == 0
    let plan := finishInstruction 1 (.borrow 0) (fun _ => false) true owned
    ((runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory, plan.pending) =
      (some [(1, true), (0, true), (0, false), (0, false), (0, false)], some 0) := by decide

-- Without an immediate constructor, both dead receiver and dead result drop.
example :
    let owned : Mask 5 := fun name => name == 0
    let plan := finishInstruction 1 (.borrow 0) (fun _ => false) false owned
    ((runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory, plan.pending) =
      (some [(0, true), (0, true), (0, false), (0, false), (0, false)], none) := by decide

-- A receiver live after the iteration is not a deferred candidate.
example :
    let owned : Mask 5 := fun name => name == 0
    let plan := finishInstruction 1 (.borrow 0) owned true owned
    ((runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory, plan.pending) =
      (some [(1, true), (0, true), (0, false), (0, false), (0, false)], none) := by decide

-- The generic instruction lemma does not assume distinct heap values or
-- distinct candidate/field local operands; runtime reuse may fall back.
example :
    let owned : Mask 5 := fun name => [0, 1].contains name
    let after : Mask 5 := fun name => name == 3
    let plan := finishInstruction 3 (.reuse 0 [0] [1]) after false owned
    (runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory =
      some [(0, true), (0, true), (0, false), (1, true), (0, false)] := by decide

-- These failures pin the real freshness/ownership premises: copies cannot
-- invent a first token, and an already defined name cannot be rebound.
example :
    let owned : Mask 5 := fun name => name == 0
    let plan := finishInstruction 0 (.consume []) (fun _ => false) false owned
    (runEvents (instructionTrace plan.instructions) (unitState owned owned)).map inventory = none := by decide
example :
    let plan := finishInstruction (width := 5) 1 (.consume [0, 0]) (fun _ => false) false (fun _ => false)
    (runEvents (instructionTrace plan.instructions)
      (unitState (fun _ => false) (fun name => name == 0))).map inventory = none := by decide
example :
    let plan := finishInstruction (width := 5) 1 (.borrow 0) (fun _ => false) false (fun _ => false)
    (runEvents (instructionTrace plan.instructions)
      (unitState (fun _ => false) (fun name => name == 0))).map inventory = none := by decide

/-! ## Executable projections of insertion regressions

These small examples pin the exact instruction order as well as token
validation. They complement, rather than replace, the general annotation
theorems above.
-/

-- Repeated fields and unused incoming parameters: Drop before Dup/Let.
example :
    let source : Block 4 := .seq (.let_ 2 (.construct [0, 0] [])) (.ret 2)
    annotate source (fun name => [0, 1].contains name) =
      some (.seq (.drop 1) (.seq (.dup 0) (.seq (.let_ 2 (.construct [0, 0] [])) (.ret 2)))) := by decide

-- Hidden evidence participates in the same consuming-use count.
example :
    let source : Block 4 := .seq (.let_ 1 (.construct [0] [0])) (.ret 1)
    annotate source (fun name => name == 0) =
      some (.seq (.dup 0) (.seq (.let_ 1 (.construct [0] [0])) (.ret 1))) := by decide

-- A last borrow is released only after its new result is bound.
example :
    let source : Block 4 := .seq (.let_ 1 (.borrow 0)) (.ret 1)
    annotate source (fun name => name == 0) =
      some (.seq (.let_ 1 (.borrow 0)) (.seq (.drop 0) (.ret 1))) := by decide

-- The immediate constructor consumes the deferred candidate as Reuse.
example :
    let source : Block 4 := .seq (.let_ 1 (.borrow 0))
      (.seq (.let_ 2 (.construct [1] [])) (.ret 2))
    annotate source (fun name => name == 0) =
      some (.seq (.let_ 1 (.borrow 0)) (.seq (.let_ 2 (.reuse 0 [1] [])) (.ret 2))) := by decide

-- A dead borrow result drops before the deferred receiver is reused.
example :
    let source : Block 4 := .seq (.let_ 1 (.borrow 0))
      (.seq (.let_ 2 (.construct [] [])) (.ret 2))
    annotate source (fun name => name == 0) =
      some (.seq (.let_ 1 (.borrow 0)) (.seq (.drop 1)
        (.seq (.let_ 2 (.reuse 0 [] [])) (.ret 2)))) := by decide

-- A still-live receiver is not a deferred candidate, even for a constructor.
example :
    let source : Block 4 := .seq (.let_ 1 (.borrow 0))
      (.seq (.let_ 2 (.construct [0, 1] [])) (.ret 2))
    annotate source (fun name => name == 0) = some source := by decide

-- No receiver release is deferred across unrelated work.
example :
    let source : Block 4 := .seq (.let_ 1 (.borrow 0))
      (.seq (.let_ 2 (.consume [])) (.seq (.let_ 3 (.construct [1] [])) (.ret 3)))
    annotate source (fun name => name == 0) =
      some (.seq (.let_ 1 (.borrow 0)) (.seq (.drop 0) (.seq (.let_ 2 (.consume []))
        (.seq (.drop 2) (.seq (.let_ 3 (.construct [1] [])) (.ret 3)))))) := by decide

-- The condition needs one extra token if a selected successor can use it.
example :
    let source : Block 4 := .branch 0 (.ret 0) (.ret 0)
    annotate source (fun name => name == 0) = some (.seq (.dup 0) source) := by decide

-- A condition used on one successor is copied once; the other drops it.
example :
    let source : Block 4 := .branch 0 (.ret 0) (.seq (.let_ 1 (.consume [])) (.ret 1))
    annotate source (fun name => name == 0) = some (.seq (.dup 0)
      (.branch 0 (.ret 0) (.seq (.drop 0) (.seq (.let_ 1 (.consume [])) (.ret 1))))) := by decide

-- Each successor trims its own inherited set, not the other successor's.
example :
    let source : Block 4 := .branch 0 (.ret 1) (.ret 2)
    annotate source (fun name => [0, 1, 2].contains name) =
      some (.branch 0 (.seq (.drop 2) (.ret 1)) (.seq (.drop 1) (.ret 2))) := by decide

-- Return transfers its one token after releasing all unused parameters.
example : annotate (width := 4) (.ret 0) (fun name => [0, 2, 3].contains name) =
    some (.seq (.drop 2) (.seq (.drop 3) (.ret 0))) := by decide

-- Preannotated source and invalid pending shapes are explicit errors.
example : annotate (width := 4) (.seq (.dup 0) (.ret 0)) (fun name => name == 0) = none := by decide
example : annotate (width := 4) (.seq (.let_ 1 (.reuse 0 [] [])) (.ret 1))
    (fun name => name == 0) = none := by decide
example : annotateCore (width := 4) (.ret 0) (fun name => name == 0) (some 0) false = none := by decide

-- The independent checker accepts each generated regression output.
example :
    let sources : List (List (Local 4) × Block 4) := [
      ([0, 1], .seq (.let_ 2 (.construct [0, 0] [])) (.ret 2)),
      ([0], .seq (.let_ 1 (.construct [0] [0])) (.ret 1)),
      ([0], .seq (.let_ 1 (.borrow 0)) (.ret 1)),
      ([0], .seq (.let_ 1 (.borrow 0)) (.seq (.let_ 2 (.construct [1] [])) (.ret 2))),
      ([0], .seq (.let_ 1 (.borrow 0)) (.seq (.let_ 2 (.construct [] [])) (.ret 2))),
      ([0], .seq (.let_ 1 (.borrow 0)) (.seq (.let_ 2 (.construct [0, 1] [])) (.ret 2))),
      ([0], .branch 0 (.ret 0) (.ret 0)),
      ([0], .branch 0 (.ret 0) (.seq (.let_ 1 (.consume [])) (.ret 1))),
      ([0, 1, 2], .branch 0 (.ret 1) (.ret 2)),
      ([0, 2, 3], .ret 0)]
    sources.all (fun (parameters, source) =>
      match annotate source (fun name => parameters.contains name) with
      | none => false
      | some output => checkFunction parameters output) = true := by decide

/-!
## Whole-block proof boundary

For arbitrary `block`, `defined` and `owned`, source acceptance together
with `live block ⊆ owned` and `owned ⊆ defined` entails an `output` such that
`annotate block owned = some output` and
`checkTokens output (unitState owned defined) = true`.

`annotate_balanced` proves this statement for the executable projection.
The function-entry corollary requires unique parameter names and derives
liveness availability from source acceptance. Path and prefix corollaries
exclude token underflow and leave
zero local tokens after transferring Return. The pending owned-mask
invariant is derived by `prepare_live_step`, not supplied for the successor.
No successful post-validation premise or global freshness across branch
siblings is used. Rust-to-Lean correspondence, Rust counter overflow,
operation/heap and failure-unwinding semantics, dynamic storage reuse and
physical reclamation remain unproved by this module.
-/

end Flick.Memory.Insert

/-- info: 'Flick.Memory.Insert.takeList_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.takeList_runs

/-- info: 'Flick.Memory.Insert.trim_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.trim_runs

/-- info: 'Flick.Memory.Insert.entry_trim_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.entry_trim_runs

/-- info: 'Flick.Memory.Insert.branch_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.branch_runs

/-- info: 'Flick.Memory.Insert.prepare_pending_dead' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.prepare_pending_dead

/-- info: 'Flick.Memory.Insert.pending_reuse_no_copy' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.pending_reuse_no_copy

/-- info: 'Flick.Memory.Insert.prepared_reuse_ready' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.prepared_reuse_ready

/-- info: 'Flick.Memory.Insert.dupList_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.dupList_runs

/-- info: 'Flick.Memory.Insert.copy_consume_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.copy_consume_runs

/-- info: 'Flick.Memory.Insert.finishInstruction_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.finishInstruction_runs

/-- info: 'Flick.Memory.Insert.finishInstruction_defined' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.finishInstruction_defined

/-- info: 'Flick.Memory.Insert.prepare_live_step' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.prepare_live_step

/-- info: 'Flick.Memory.Insert.annotateBody_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotateBody_balanced

/-- info: 'Flick.Memory.Insert.annotate_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotate_balanced

/-- info: 'Flick.Memory.Insert.annotate_defined_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotate_defined_balanced

/-- info: 'Flick.Memory.Insert.annotate_path_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotate_path_safe

/-- info: 'Flick.Memory.Insert.annotate_prefix_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotate_prefix_safe

/-- info: 'Flick.Memory.Insert.bindList_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.bindList_runs

/-- info: 'Flick.Memory.Insert.parameter_init_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.parameter_init_runs

/-- info: 'Flick.Memory.Insert.annotate_function_balanced' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Insert.annotate_function_balanced
