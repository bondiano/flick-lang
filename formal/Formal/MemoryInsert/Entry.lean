import Formal.MemoryInsert.Pending

/-! # Block entry and annotation unfolding -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem prepend_append (left right : List (Instruction width)) (block : Block width) :
    prepend (left ++ right) block = prepend left (prepend right block) := by
  induction left with
  | nil => rfl
  | cons instruction rest ih =>
      change Block.seq instruction (prepend (rest ++ right) block) =
        Block.seq instruction (prepend rest (prepend right block))
      rw [ih]

theorem checkTokens_prepend (instructions : List (Instruction width)) (block : Block width)
    (state : State width) :
    checkTokens (prepend instructions block) state =
      match runEvents (instructionTrace instructions) state with
      | none => false
      | some after => checkTokens block after := by
  induction instructions generalizing state with
  | nil => rfl
  | cons instruction rest ih =>
      change checkTokens (.seq instruction (prepend rest block)) state = _
      change checkTokens (.seq instruction (prepend rest block)) state =
        match runEvents (instructionEvents instruction ++ instructionTrace rest) state with
        | none => false
        | some after => checkTokens block after
      simp only [checkTokens, runEvents_append]
      cases first : runEvents (instructionEvents instruction) state with
      | none => rfl
      | some after => simpa only [Option.bind_some] using ih after

theorem checkTokens_prepend_of_run (instructions : List (Instruction width)) (block : Block width)
    (state after : State width)
    (executed : runEvents (instructionTrace instructions) state = some after)
    (continuation : checkTokens block after = true) :
    checkTokens (prepend instructions block) state = true := by
  rw [checkTokens_prepend, executed]
  exact continuation

theorem annotateCore_ret (result : Local width) (owned : Mask width)
    (pending : Option (Local width)) (entry : Bool) :
    annotateCore (.ret result) owned pending entry =
      let drops := if entry then trimInstructions owned (live (.ret result)) else []
      let owned := if entry then retained owned (live (.ret result)) else owned
      if entry && pending.isSome then none else
        if pending.isSome then none else
          some (prepend (drops ++ trimInstructions owned (fun name => name == result)) (.ret result)) := rfl

theorem annotateCore_seq (binding : Local width) (value : Operation width) (next : Block width)
    (owned : Mask width) (pending : Option (Local width)) (entry : Bool) :
    annotateCore (.seq (.let_ binding value) next) owned pending entry =
      (let drops := if entry then trimInstructions owned (live (.seq (.let_ binding value) next)) else []
      let owned := if entry then retained owned (live (.seq (.let_ binding value) next)) else owned
      if entry && pending.isSome then none else do
        let plan ← prepare binding value (live next) (nextConstructor next) owned pending
        let rest ← annotateCore next plan.owned plan.pending false
        pure (prepend (drops ++ plan.instructions) rest)) := rfl

theorem annotateCore_branch (condition : Local width) (yes no : Block width)
    (owned : Mask width) (pending : Option (Local width)) (entry : Bool) :
    annotateCore (.branch condition yes no) owned pending entry =
      (let drops := if entry then trimInstructions owned (live (.branch condition yes no)) else []
      let owned := if entry then retained owned (live (.branch condition yes no)) else owned
      if entry && pending.isSome then none else
        if pending.isSome then none else do
          let successors := fun name => live yes name || live no name
          let duplicates := if successors condition then [.dup condition] else []
          let book := branchBook owned successors condition
          let remaining := retained book successors
          let yes ← annotateCore yes remaining none true
          let no ← annotateCore no remaining none true
          pure (prepend (drops ++ duplicates ++ trimInstructions book successors) (.branch condition yes no))) := rfl

/-- Exact entry-only trimming equation for the executable cached algorithm. -/
theorem annotateCore_entry (block : Block width) (owned : Mask width) :
    annotateCore block owned none true =
      (annotateCore block (retained owned (live block)) none false).map
        (prepend (trimInstructions owned (live block))) := by
  cases block with
  | ret result =>
      rw [annotateCore_ret, annotateCore_ret]
      simp [prepend_append]
  | seq instruction next =>
      cases instruction with
      | dup name => rfl
      | drop name => rfl
      | let_ binding value =>
          rw [annotateCore_seq, annotateCore_seq]
          simp only [Option.isSome_none, Bool.and_false, ite_true]
          cases prepared : prepare binding value (live next) (nextConstructor next)
              (retained owned (live (.seq (.let_ binding value) next))) none with
          | none => simp [prepared]
          | some plan =>
              cases rest : annotateCore next plan.owned plan.pending false with
              | none => simp [prepared, rest]
              | some output => simp [prepared, rest, prepend_append]
  | branch condition yes no =>
      rw [annotateCore_branch, annotateCore_branch]
      simp only [Option.isSome_none, Bool.and_false, ite_true]
      let incoming := retained owned (live (.branch condition yes no))
      let successors := fun name => live yes name || live no name
      let remaining := retained (branchBook incoming successors condition) successors
      cases first : annotateCore yes remaining none true with
      | none => simp [incoming, successors, remaining] at first; simp [first]
      | some left =>
          cases second : annotateCore no remaining none true with
          | none => simp [incoming, successors, remaining] at first second; simp [first, second]
          | some right =>
              simp [incoming, successors, remaining] at first second
              simp [first, second, prepend_append]

theorem instructionTrace_drops (names : List (Local width)) :
    instructionTrace (names.map Instruction.drop) = names.map Event.take := by
  induction names with
  | nil => rfl
  | cons head tail ih => simpa [instructionTrace, instructionEvents] using ih

theorem instructionTrace_trim (owned needed : Mask width) :
    instructionTrace (trimInstructions owned needed) = (trimNames owned needed).map Event.take := by
  rw [trimInstructions, instructionTrace_drops]

theorem entry_lift (block : Block width) (owned defined : Mask width) (output : Block width)
    (available : ∀ name, live block name = true → owned name = true)
    (annotated : annotateCore block (live block) none false = some output)
    (continuation : checkTokens output (unitState (live block) defined) = true) :
    annotateCore block owned none true = some (prepend (trimInstructions owned (live block)) output) ∧
      checkTokens (prepend (trimInstructions owned (live block)) output) (unitState owned defined) = true := by
  constructor
  · rw [annotateCore_entry, retained_eq owned (live block) available, annotated]
    rfl
  · apply checkTokens_prepend_of_run _ _ _ _ _ continuation
    rw [instructionTrace_trim]
    exact entry_trim_runs owned (live block) defined available

theorem trimInstructions_same (owned : Mask width) : trimInstructions owned owned = [] := by
  have empty (names : List (Local width)) :
      names.filter (fun name => owned name && !owned name) = [] := by
    induction names with
    | nil => rfl
    | cons head tail ih =>
        simp only [List.filter_cons]
        cases h : owned head <;> simpa only [h] using ih
  unfold trimInstructions trimNames
  rw [empty]
  rfl

theorem return_check (result : Local width) (defined : Mask width) :
    checkTokens (.ret result) (unitState (live (.ret result)) defined) = true := by
  let needed : Mask width := fun name => name == result
  change checkTokens (.ret result) (unitState needed defined) = true
  have present : needed result = true := by simp [needed]
  have cleared : update needed result false = fun _ => false := by
    funext name
    by_cases same : name = result
    · subst name
      simp [update]
    · simp [update, needed, same]
  simp only [checkTokens]
  rw [drop_unitState result needed defined present, cleared]
  apply (emptyTokens_iff _).mpr
  intro name
  rfl

theorem pending_none_of_not_constructor (block : Block width) (pending : Option (Local width))
    (valid : PendingValid block pending) (notConstructor : nextConstructor block = false) :
    pending = none := by
  cases pending with
  | none => rfl
  | some receiver =>
      have impossible := (valid receiver rfl).2
      simp [notConstructor] at impossible

theorem branchBook_available (owned successors : Mask width) (condition : Local width)
    (available : ∀ name, successors name = true → owned name = true) :
    ∀ name, successors name = true → branchBook owned successors condition name = true := by
  intro name needed
  by_cases kept : successors condition = true
  · simpa [branchBook, kept] using available name needed
  · have dead : successors condition = false := by cases h : successors condition <;> simp_all
    have different : name ≠ condition := by
      intro same
      subst name
      simp [dead] at needed
    simpa [branchBook, dead, update, different] using available name needed

def branchInstructions (owned successors : Mask width) (condition : Local width) : List (Instruction width) :=
  (if successors condition then [.dup condition] else []) ++
    trimInstructions (branchBook owned successors condition) successors

theorem branchInstructions_trace (owned successors : Mask width) (condition : Local width) :
    instructionTrace (branchInstructions owned successors condition) ++ [.take condition] =
      branchEvents owned successors condition := by
  unfold branchInstructions
  rw [instructionTrace_append, instructionTrace_trim]
  unfold branchEvents branchNames
  rw [List.map_append]
  by_cases kept : successors condition = true
  · simp [branchDups, instructionTrace, instructionEvents, kept]
  · have dead : successors condition = false := by cases h : successors condition <;> simp_all
    simp [branchDups, instructionTrace, dead]

theorem checkTokens_prepend_branch (instructions : List (Instruction width)) (condition : Local width)
    (yes no : Block width) (state after : State width)
    (executed : runEvents (instructionTrace instructions ++ [.take condition]) state = some after)
    (yesAccepted : checkTokens yes after = true) (noAccepted : checkTokens no after = true) :
    checkTokens (prepend instructions (.branch condition yes no)) state = true := by
  rw [checkTokens_prepend]
  rw [runEvents_append] at executed
  cases prefixRun : runEvents (instructionTrace instructions) state with
  | none => simp [prefixRun] at executed
  | some middle =>
      have conditionRun : runEvents [.take condition] middle = some after := by
        simpa [prefixRun] using executed
      simp [checkTokens, conditionRun, yesAccepted, noAccepted]

theorem pendingValid_none (block : Block width) : PendingValid block none := by
  intro receiver impossible
  cases impossible

end Flick.Memory.Insert
