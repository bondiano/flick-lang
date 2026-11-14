import Formal.MemoryInsert.Copies

/-! # Binding, dropping and instruction-state composition -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem bind_unitState (binding : Local width) (owned defined : Mask width)
    (fresh : defined binding = false) :
    runEvents [.bind binding] (unitState owned defined) =
      some (unitState (update owned binding true) (update defined binding true)) := by
  simp only [runEvents, Pass.step, unitState, fresh, Bool.false_eq_true, ite_false, Option.bind_some]
  apply congrArg some
  apply state_ext
  · intro name
    by_cases same : name = binding
    · subst name
      simp [update]
    · simp [update, same]
  · intro name
    rfl

theorem drop_unitState (name : Local width) (owned defined : Mask width)
    (present : owned name = true) :
    runEvents [.take name] (unitState owned defined) =
      some (unitState (update owned name false) defined) := by
  simp only [runEvents, Pass.step, unitState, present, ite_true]
  simp only [show ((1 : Nat) == 0) = false from rfl, Bool.false_eq_true, ite_false, Option.bind_some]
  apply congrArg some
  apply state_ext
  · intro other
    by_cases same : other = name
    · subst other
      simp [update]
    · simp [update, same]
  · intro other
    rfl

theorem update_override (mask : Mask width) (name : Local width) (first second : Bool) :
    update (update mask name first) name second = update mask name second := by
  funext other
  by_cases same : other = name
  · subst other
    simp [update]
  · simp [update, same]

theorem update_commute (mask : Mask width) (left right : Local width) (a b : Bool)
    (different : left ≠ right) :
    update (update mask left a) right b = update (update mask right b) left a := by
  funext name
  by_cases first : name = left
  · subst name
    simp [update, different]
  · by_cases second : name = right
    · subst name
      simp [update, Ne.symm different]
    · simp [update, first, second]

theorem resultDrop_runs (binding : Local width) (owned after defined : Mask width) :
    runEvents (if after binding then [] else [.take binding])
      (unitState (update owned binding true) defined) =
        some (unitState (update owned binding (after binding)) defined) := by
  by_cases keep : after binding = true
  · simp [keep, runEvents]
  · have dead : after binding = false := by cases h : after binding <;> simp_all
    simp only [dead, Bool.false_eq_true, ite_false]
    rw [drop_unitState binding (update owned binding true) defined (by simp [update])]
    rw [update_override]

theorem instructionTrace_singleton (instruction : Instruction width) :
    instructionTrace [instruction] = instructionEvents instruction := by
  simp [instructionTrace]

theorem resultDrop_trace (binding : Local width) (keep : Bool) :
    instructionTrace (if keep then [] else [.drop binding]) =
      if keep then [] else [.take binding] := by
  cases keep <;> rfl

/-- The fresh result is bound before a dead borrowed receiver is released.
Distinctness here is derived from source-name freshness, not heap identity. -/
theorem bind_borrow_runs (binding : Local width) (value : Operation width)
    (after owned defined : Mask width) (next : Bool)
    (fresh : defined binding = false)
    (ownersDefined : ∀ name, owned name = true → defined name = true)
    (available : ∀ receiver, borrowed value = some receiver → owned receiver = true) :
    runEvents ([.bind binding] ++ instructionTrace (borrowDrops value after next))
      (unitState owned defined) =
        some (unitState (update (borrowOwners owned value after next) binding true)
          (update defined binding true)) := by
  rw [runEvents_append, bind_unitState binding owned defined fresh]
  simp only [Option.bind_some]
  cases hb : borrowed value with
  | none => simp [borrowDrops, borrowOwners, hb, instructionTrace, runEvents]
  | some receiver =>
      have receiverOwned := available receiver hb
      have receiverDefined := ownersDefined receiver receiverOwned
      have different : receiver ≠ binding := by
        intro same
        subst receiver
        simp [fresh] at receiverDefined
      by_cases release : (!after receiver && !next) = true
      · simp only [borrowDrops, borrowOwners, hb, release, ite_true]
        rw [instructionTrace_singleton]
        change runEvents [.take receiver]
          (unitState (update owned binding true) (update defined binding true)) = _
        have retained : update owned binding true receiver = true := by
          simp [update, different, receiverOwned]
        rw [drop_unitState receiver (update owned binding true) (update defined binding true) retained]
        rw [update_commute owned binding receiver true false (Ne.symm different)]
      · simp [borrowDrops, borrowOwners, hb, release, instructionTrace, runEvents]

theorem instructionTail_runs (binding : Local width) (value : Operation width)
    (after owned defined : Mask width) (next : Bool)
    (fresh : defined binding = false)
    (ownersDefined : ∀ name, owned name = true → defined name = true)
    (available : ∀ receiver, borrowed value = some receiver → owned receiver = true) :
    runEvents (([.bind binding] ++ instructionTrace (borrowDrops value after next)) ++
      (if after binding then [] else [.take binding])) (unitState owned defined) =
        some (unitState (update (borrowOwners owned value after next) binding (after binding))
          (update defined binding true)) := by
  rw [runEvents_append, bind_borrow_runs binding value after owned defined next fresh ownersDefined available]
  simp only [Option.bind_some]
  exact resultDrop_runs binding (borrowOwners owned value after next) after (update defined binding true)

theorem finishInstruction_trace (binding : Local width) (value : Operation width)
    (after owned : Mask width) (next : Bool) :
    instructionTrace (finishInstruction binding value after next owned).instructions =
      (instructionTrace (copyInstructions value after) ++ operationEvents value) ++
        (([.bind binding] ++ instructionTrace (borrowDrops value after next)) ++
          (if after binding then [] else [.take binding])) := by
  simp only [finishInstruction, instructionTrace_append, instructionTrace_singleton,
    instructionEvents, resultDrop_trace, List.append_assoc]

theorem consumedOwners_defined (owned after defined : Mask width) (value : Operation width)
    (ownersDefined : ∀ name, owned name = true → defined name = true) :
    ∀ name, consumedOwners owned value after name = true → defined name = true := by
  intro name kept
  unfold consumedOwners at kept
  split at kept
  · contradiction
  · exact ownersDefined name kept

/-- One exact emitted instruction iteration preserves the unit-token
ownership inventory and extends source definitions by its fresh result.
It covers consuming operations, repeated/evidence operands, reads, and
either immediate or deferred release of a dead borrowed receiver. -/
theorem finishInstruction_runs (binding : Local width) (value : Operation width)
    (after owned defined : Mask width) (next : Bool)
    (fresh : defined binding = false)
    (ownersDefined : ∀ name, owned name = true → defined name = true)
    (available : ∀ name, name ∈ operands value → owned name = true) :
    runEvents (instructionTrace (finishInstruction binding value after next owned).instructions)
      (unitState owned defined) =
        some (unitState (finishInstruction binding value after next owned).owned
          (update defined binding true)) := by
  have postDefined := consumedOwners_defined owned after defined value ownersDefined
  have borrowedAvailable : ∀ receiver, borrowed value = some receiver →
      consumedOwners owned value after receiver = true := by
    intro receiver hb
    have empty := borrowed_consumed_nil value receiver hb
    rw [consumedOwners_nil owned after value empty]
    apply available receiver
    simp [operands, hb]
  rw [finishInstruction_trace, runEvents_append,
    copy_operation_runs value after owned defined available]
  simp only [Option.bind_some]
  exact instructionTail_runs binding value after (consumedOwners owned value after) defined next
    fresh postDefined borrowedAvailable

theorem removedOwners_defined (owned defined : Mask width) (receiver : Local width)
    (ownersDefined : ∀ name, owned name = true → defined name = true) :
    ∀ name, update owned receiver false name = true → defined name = true := by
  intro name present
  by_cases same : name = receiver
  · subst name
    simp [update] at present
  · apply ownersDefined name
    simpa [update, same] using present

theorem borrowOwners_defined (owned after defined : Mask width) (value : Operation width) (next : Bool)
    (ownersDefined : ∀ name, owned name = true → defined name = true) :
    ∀ name, borrowOwners owned value after next name = true → defined name = true := by
  cases hb : borrowed value with
  | none => simpa [borrowOwners, hb] using ownersDefined
  | some receiver =>
      by_cases release : (!after receiver && !next) = true
      · simpa [borrowOwners, hb, release] using removedOwners_defined owned defined receiver ownersDefined
      · simpa [borrowOwners, hb, release] using ownersDefined

theorem finishInstruction_defined (binding : Local width) (value : Operation width)
    (after owned defined : Mask width) (next : Bool)
    (ownersDefined : ∀ name, owned name = true → defined name = true) :
    ∀ name, (finishInstruction binding value after next owned).owned name = true →
      update defined binding true name = true := by
  have oldDefined := borrowOwners_defined (consumedOwners owned value after) after defined value next
    (consumedOwners_defined owned after defined value ownersDefined)
  intro name present
  by_cases same : name = binding
  · subst name
    simp [update]
  · have old : borrowOwners (consumedOwners owned value after) value after next name = true := by
      simpa [finishInstruction, update, same] using present
    simpa [update, same] using oldDefined name old

end Flick.Memory.Insert
