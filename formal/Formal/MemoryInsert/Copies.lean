import Formal.MemoryInsert.Annotation

/-! # Copy and consuming-operation token composition -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-! ## Instruction-state composition -/

def instructionTrace (instructions : List (Instruction width)) : List (Event width) :=
  instructions.flatMap instructionEvents

theorem instructionTrace_append (left right : List (Instruction width)) :
    instructionTrace (left ++ right) = instructionTrace left ++ instructionTrace right :=
  List.flatMap_append

def added (names : List (Local width)) (state : State width) : State width :=
  { state with tokens := fun name => state.tokens name + names.count name }

/-- Grouping copies by name does not create the first token for that name. -/
theorem dupList_runs (names : List (Local width)) (state : State width)
    (present : ∀ name, name ∈ names → 0 < state.tokens name) :
    runEvents (names.map Event.dup) state = some (added names state) := by
  induction names generalizing state with
  | nil => rfl
  | cons head tail ih =>
      have positive := present head (List.mem_cons_self ..)
      let next : State width :=
        { state with tokens := update state.tokens head (state.tokens head + 1) }
      have transition : Pass.step (.dup head) state = some next := by
        simp [Pass.step, next, Nat.ne_of_gt positive]
      have tailPresent : ∀ name, name ∈ tail → 0 < next.tokens name := by
        intro name member
        have old := present name (List.mem_cons_of_mem head member)
        by_cases same : name = head
        · subst name
          simp [next, update]
        · simpa [next, update, same] using old
      simp only [List.map_cons, runEvents, transition, Option.bind_some]
      rw [ih next tailPresent]
      apply congrArg some
      apply state_ext
      · intro name
        by_cases same : name = head
        · subst name
          simp [added, next, update, List.count_cons_self]
          omega
        · simp [added, next, update, same, List.count_cons_of_ne (Ne.symm same)]
      · intro name
        rfl

def groupedNames (amount : Local width → Nat) (names : List (Local width)) : List (Local width) :=
  names.flatMap fun name => List.replicate (amount name) name

theorem groupedNames_count (amount : Local width → Nat) (names : List (Local width))
    (distinct : names.Nodup) (name : Local width) :
    (groupedNames amount names).count name = if name ∈ names then amount name else 0 := by
  induction names with
  | nil => simp [groupedNames]
  | cons head tail ih =>
      have parts := List.nodup_cons.mp distinct
      simp only [groupedNames, List.flatMap_cons, List.count_append]
      change (List.replicate (amount head) head).count name +
        (groupedNames amount tail).count name = if name ∈ head :: tail then amount name else 0
      rw [ih parts.2]
      by_cases same : head = name
      · subst name
        simp [parts.1]
      · simp [List.count_replicate, same, Ne.symm same]

def copyNames (value : Operation width) (after : Mask width) : List (Local width) :=
  groupedNames (fun name => copies ((consumed value).count name) (after name)) (List.finRange width)

theorem copyNames_count (value : Operation width) (after : Mask width) (name : Local width) :
    (copyNames value after).count name = copies ((consumed value).count name) (after name) := by
  rw [copyNames, groupedNames_count _ _ finRange_nodup]
  simp [List.mem_finRange]

theorem copyInstructions_eq (value : Operation width) (after : Mask width) :
    copyInstructions value after = (copyNames value after).map Instruction.dup := by
  simp only [copyInstructions, copyNames, groupedNames, List.map_flatMap, List.map_replicate]

theorem instructionTrace_dups (names : List (Local width)) :
    instructionTrace (names.map Instruction.dup) = names.map Event.dup := by
  induction names with
  | nil => rfl
  | cons head tail ih => simpa [instructionTrace, instructionEvents] using ih

theorem copyInstructions_trace (value : Operation width) (after : Mask width) :
    instructionTrace (copyInstructions value after) = (copyNames value after).map Event.dup := by
  rw [copyInstructions_eq, instructionTrace_dups]

theorem copies_consumed (count : Nat) (keep : Bool) (positive : 0 < count) :
    1 + copies count keep - count = kept keep := by
  have balance := balanced count keep
  have noDiscard : discarded count keep = 0 := by simp [discarded, Nat.ne_of_gt positive]
  rw [noDiscard] at balance
  omega

theorem consumedOwners_present (owned after : Mask width) (value : Operation width)
    (name : Local width) (member : (consumed value).contains name = true)
    (present : owned name = true) : consumedOwners owned value after name = after name := by
  unfold consumedOwners
  rw [member, present]
  cases after name <;> rfl

/-- The emitted grouped copies and the operation's ordered consuming uses
compose to one token per remaining owner. Repeated/evidence operands retain
their full multiplicity; no validator-success premise is used. -/
theorem copy_consume_runs (value : Operation width) (after owned defined : Mask width)
    (available : ∀ name, name ∈ consumed value → owned name = true) :
    runEvents (instructionTrace (copyInstructions value after) ++ (consumed value).map Event.take)
      (unitState owned defined) = some (unitState (consumedOwners owned value after) defined) := by
  have copiedPresent : ∀ name, name ∈ copyNames value after →
      0 < (unitState owned defined).tokens name := by
    intro name member
    have copied := List.count_pos_iff.mpr member
    rw [copyNames_count] at copied
    have used : 0 < (consumed value).count name := by
      unfold copies at copied
      omega
    have owner := available name (List.count_pos_iff.mp used)
    simp [unitState, owner]
  have enough : ∀ name, (consumed value).count name ≤
      (added (copyNames value after) (unitState owned defined)).tokens name := by
    intro name
    change (consumed value).count name ≤
      (if owned name then 1 else 0) + (copyNames value after).count name
    rw [copyNames_count]
    by_cases zero : (consumed value).count name = 0
    · simp [zero]
    · have positive := Nat.pos_of_ne_zero zero
      have owner := available name (List.count_pos_iff.mp positive)
      rw [owner]
      have bound := no_underflow ((consumed value).count name) (after name)
      simp only [ite_true]
      omega
  rw [runEvents_append, copyInstructions_trace, dupList_runs _ _ copiedPresent]
  simp only [Option.bind_some]
  rw [takeList_runs _ _ enough]
  apply congrArg some
  apply state_ext
  · intro name
    change ((if owned name then 1 else 0) + (copyNames value after).count name) -
      (consumed value).count name = if consumedOwners owned value after name then 1 else 0
    rw [copyNames_count]
    by_cases zero : (consumed value).count name = 0
    · have notMember := List.count_eq_zero.mp zero
      simp [zero, copies, consumedOwners, notMember]
    · have positive := Nat.pos_of_ne_zero zero
      have member := List.count_pos_iff.mp positive
      have owner := available name member
      have contains := List.contains_iff_mem.mpr member
      rw [consumedOwners_present owned after value name contains owner, owner]
      simpa [kept] using copies_consumed ((consumed value).count name) (after name) positive
  · intro name
    rfl

theorem copyInstructions_nil (value : Operation width) (after : Mask width)
    (empty : consumed value = []) : copyInstructions value after = [] := by
  simp [copyInstructions, empty, copies]

theorem borrowed_consumed_nil (value : Operation width) (receiver : Local width)
    (hasBorrow : borrowed value = some receiver) : consumed value = [] := by
  cases value <;> simp_all [borrowed, consumed]

theorem consumedOwners_nil (owned after : Mask width) (value : Operation width)
    (empty : consumed value = []) : consumedOwners owned value after = owned := by
  funext name
  simp [consumedOwners, empty]

theorem borrow_unitState (receiver : Local width) (owned defined : Mask width)
    (present : owned receiver = true) :
    runEvents [.borrow receiver] (unitState owned defined) = some (unitState owned defined) := by
  simp [runEvents, Pass.step, unitState, present]

/-- The current operation family never combines a borrowed receiver with
consuming operands. Its read therefore composes with the exact copy/use stream. -/
theorem copy_operation_runs (value : Operation width) (after owned defined : Mask width)
    (available : ∀ name, name ∈ operands value → owned name = true) :
    runEvents (instructionTrace (copyInstructions value after) ++ operationEvents value)
      (unitState owned defined) = some (unitState (consumedOwners owned value after) defined) := by
  have usedAvailable : ∀ name, name ∈ consumed value → owned name = true := by
    intro name member
    apply available name
    exact List.mem_append_left _ member
  cases hb : borrowed value with
  | none =>
      rw [operationEvents_projection, hb]
      simp only [Option.toList_none, List.map_nil, List.nil_append]
      exact copy_consume_runs value after owned defined usedAvailable
  | some receiver =>
      have empty := borrowed_consumed_nil value receiver hb
      have present : owned receiver = true := by
        apply available receiver
        simp [operands, hb]
      rw [copyInstructions_nil value after empty, operationEvents_projection, hb, empty]
      simp only [instructionTrace, List.flatMap_nil, Option.toList_some, List.map_cons,
        List.map_nil, List.nil_append, List.append_nil]
      rw [consumedOwners_nil owned after value empty]
      exact borrow_unitState receiver owned defined present

end Flick.Memory.Insert
