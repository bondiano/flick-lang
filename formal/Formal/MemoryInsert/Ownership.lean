import Formal.Memory
import Formal.MemoryPass

/-! # Token state, entry trimming and branch ownership -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

def unitState (owned defined : Mask width) : State width :=
  ⟨fun name => if owned name then 1 else 0, defined⟩

theorem state_ext (left right : State width)
    (tokens : ∀ name, left.tokens name = right.tokens name)
    (defined : ∀ name, left.defined name = right.defined name) : left = right := by
  cases left with
  | mk leftTokens leftDefined =>
      cases right with
      | mk rightTokens rightDefined =>
          have ht : leftTokens = rightTokens := funext tokens
          have hd : leftDefined = rightDefined := funext defined
          subst rightTokens
          subst rightDefined
          rfl

def subtract (names : List (Local width)) (state : State width) : State width :=
  { state with tokens := fun name => state.tokens name - names.count name }

/-- The exact multiplicity of every consumed local must be available.
The operand order is retained, rather than silently grouping ownership uses. -/
theorem takeList_runs (names : List (Local width)) (state : State width)
    (enough : ∀ name, names.count name ≤ state.tokens name) :
    runEvents (names.map Event.take) state = some (subtract names state) := by
  induction names generalizing state with
  | nil => rfl
  | cons head tail ih =>
      have positive : 0 < state.tokens head := by
        have h := enough head
        simp only [List.count_cons_self] at h
        omega
      let next : State width :=
        { state with tokens := update state.tokens head (state.tokens head - 1) }
      have transition : Pass.step (.take head) state = some next := by
        simp [Pass.step, next, Nat.ne_of_gt positive]
      have tailEnough : ∀ name, tail.count name ≤ next.tokens name := by
        intro name
        have h := enough name
        by_cases same : name = head
        · subst name
          simp only [List.count_cons_self] at h
          simp [next, update]
          omega
        · have other : head ≠ name := Ne.symm same
          simpa [next, update, same, List.count_cons_of_ne other] using h
      simp only [List.map_cons, runEvents, transition, Option.bind_some]
      rw [ih next tailEnough]
      apply congrArg some
      apply state_ext
      · intro name
        by_cases same : name = head
        · subst name
          simp [subtract, next, update, List.count_cons_self]
          omega
        · have other : head ≠ name := Ne.symm same
          simp [subtract, next, update, same, List.count_cons_of_ne other]
      · intro name
        rfl

def retained (owned keep : Mask width) : Mask width :=
  fun name => owned name && keep name

theorem retained_eq (owned keep : Mask width)
    (available : ∀ name, keep name = true → owned name = true) : retained owned keep = keep := by
  funext name
  apply Bool.eq_iff_iff.mpr
  simp only [retained, Bool.and_eq_true]
  exact ⟨And.right, fun live => ⟨available name live, live⟩⟩

def trimNames (owned keep : Mask width) : List (Local width) :=
  (List.finRange width).filter (fun name => owned name && !keep name)

def trimInstructions (owned keep : Mask width) : List (Instruction width) :=
  (trimNames owned keep).map Instruction.drop

theorem finRange_nodup : (List.finRange width).Nodup := by
  apply List.pairwise_iff_getElem.mpr
  intro i j hi hj smaller
  intro same
  have equalValues := congrArg Fin.val same
  simp at equalValues
  omega

theorem trimNames_nodup (owned keep : Mask width) : (trimNames owned keep).Nodup := by
  exact List.filter_sublist.nodup finRange_nodup

theorem trimNames_mem (owned keep : Mask width) (name : Local width) :
    name ∈ trimNames owned keep ↔ owned name = true ∧ keep name = false := by
  simp [trimNames, List.mem_filter]

theorem trimNames_count (owned keep : Mask width) (name : Local width) :
    (trimNames owned keep).count name = if owned name && !keep name then 1 else 0 := by
  rw [(trimNames_nodup owned keep).count]
  simp only [trimNames_mem]
  cases owned name <;> cases keep name <;> simp

theorem trim_arithmetic (owned keep : Bool) :
    (if owned && !keep then 1 else 0) ≤ (if owned then 1 else 0) ∧
      (if owned then 1 else 0) - (if owned && !keep then 1 else 0) =
        (if owned && keep then 1 else 0) := by
  cases owned <;> cases keep <;> decide

/-- Trimming drops each discarded owner exactly once, preserving the
defined-name mask and every retained token. No heap sweep is involved. -/
theorem trim_runs (owned keep defined : Mask width) :
    runEvents ((trimNames owned keep).map Event.take) (unitState owned defined) =
      some (unitState (retained owned keep) defined) := by
  have enough : ∀ name, (trimNames owned keep).count name ≤
      (unitState owned defined).tokens name := by
    intro name
    rw [trimNames_count]
    exact (trim_arithmetic (owned name) (keep name)).1
  rw [takeList_runs _ _ enough]
  apply congrArg some
  apply state_ext
  · intro name
    simp only [subtract, unitState, retained]
    rw [trimNames_count]
    exact (trim_arithmetic (owned name) (keep name)).2
  · intro name
    rfl

/-- If the computed live set was available, entry trim leaves precisely
one token per live local, not just an unspecified subset of incoming owners. -/
theorem entry_trim_runs (owned needed defined : Mask width)
    (available : ∀ name, needed name = true → owned name = true) :
    runEvents ((trimNames owned needed).map Event.take) (unitState owned defined) =
      some (unitState needed defined) := by
  rw [trim_runs, retained_eq owned needed available]

def branchBook (owned successors : Mask width) (condition : Local width) : Mask width :=
  if successors condition then owned else update owned condition false

def branchDups (successors : Mask width) (condition : Local width) : List (Event width) :=
  if successors condition then [.dup condition] else []

def branchState (owned successors : Mask width) (condition : Local width)
    (defined : Mask width) : State width :=
  let state := unitState owned defined
  if successors condition then
    { state with tokens := update state.tokens condition (state.tokens condition + 1) }
  else state

def branchNames (owned successors : Mask width) (condition : Local width) : List (Local width) :=
  trimNames (branchBook owned successors condition) successors ++ [condition]

/-- Exit preparation in Rust order: optional condition Dup, trim Drops,
and finally the condition token consumed by the Branch instruction. -/
def branchEvents (owned successors : Mask width) (condition : Local width) : List (Event width) :=
  branchDups successors condition ++ (branchNames owned successors condition).map Event.take

theorem branchBook_at (owned successors : Mask width) (condition name : Local width) :
    branchBook owned successors condition name =
      if successors condition then owned name else owned name && !(condition == name) := by
  by_cases kept : successors condition = true
  · simp [branchBook, kept]
  · have absent : successors condition = false := by cases h : successors condition <;> simp_all
    by_cases same : condition = name
    · subst name
      simp [branchBook, absent, update]
    · simp [branchBook, absent, update, same, Ne.symm same]

theorem branchState_tokens (owned successors defined : Mask width)
    (condition name : Local width) :
    (branchState owned successors condition defined).tokens name =
      (if owned name then 1 else 0) +
        (if successors condition && (condition == name) then 1 else 0) := by
  by_cases kept : successors condition = true
  · by_cases same : condition = name
    · subst name
      simp [branchState, kept, unitState, update]
    · simp [branchState, kept, unitState, update, same, Ne.symm same]
  · have absent : successors condition = false := by cases h : successors condition <;> simp_all
    simp [branchState, absent, unitState]

theorem branchNames_count (owned successors : Mask width) (condition name : Local width) :
    (branchNames owned successors condition).count name =
      (if branchBook owned successors condition name && !successors name then 1 else 0) +
        (if condition == name then 1 else 0) := by
  simp only [branchNames, List.count_append, trimNames_count, List.count_cons,
    List.count_nil, Nat.zero_add]

theorem branch_arithmetic (ownedHere keepHere same conditionKeep : Bool)
    (conditionOwned : same = true → ownedHere = true)
    (conditionMask : same = true → keepHere = conditionKeep) :
    let bookkeeping := if conditionKeep then ownedHere else ownedHere && !same
    let available := (if ownedHere then 1 else 0) + (if conditionKeep && same then 1 else 0)
    let consumed := (if bookkeeping && !keepHere then 1 else 0) + (if same then 1 else 0)
    consumed ≤ available ∧ available - consumed = (if ownedHere && keepHere then 1 else 0) := by
  cases ownedHere <;> cases keepHere <;> cases same <;> cases conditionKeep <;> simp_all

theorem branchDup_runs (owned successors defined : Mask width) (condition : Local width)
    (conditionOwned : owned condition = true) :
    runEvents (branchDups successors condition) (unitState owned defined) =
      some (branchState owned successors condition defined) := by
  by_cases kept : successors condition = true
  · simp [branchDups, branchState, kept, runEvents, Pass.step, unitState, conditionOwned]
  · have absent : successors condition = false := by cases h : successors condition <;> simp_all
    simp [branchDups, branchState, absent, runEvents]

/-- Branch preparation is safe even when the condition remains live in a
successor. Its extra copy is consumed exactly once, leaving one token for
each retained successor local. Unused incoming owners are all dropped. -/
theorem branch_runs (owned successors defined : Mask width) (condition : Local width)
    (conditionOwned : owned condition = true) :
    runEvents (branchEvents owned successors condition) (unitState owned defined) =
      some (unitState (retained owned successors) defined) := by
  have arithmetic (name : Local width) :=
    branch_arithmetic (owned name) (successors name) (condition == name) (successors condition)
      (by intro same; have equal := beq_iff_eq.mp same; subst name; exact conditionOwned)
      (by intro same; have equal := beq_iff_eq.mp same; subst name; rfl)
  have enough : ∀ name, (branchNames owned successors condition).count name ≤
      (branchState owned successors condition defined).tokens name := by
    intro name
    rw [branchNames_count, branchBook_at, branchState_tokens]
    exact (arithmetic name).1
  unfold branchEvents
  rw [runEvents_append, branchDup_runs owned successors defined condition conditionOwned]
  simp only [Option.bind_some]
  rw [takeList_runs _ _ enough]
  apply congrArg some
  apply state_ext
  · intro name
    change (branchState owned successors condition defined).tokens name -
      (branchNames owned successors condition).count name =
        (if owned name && successors name then 1 else 0)
    rw [branchNames_count, branchBook_at, branchState_tokens]
    exact (arithmetic name).2
  · intro name
    by_cases kept : successors condition = true
    · simp [subtract, branchState, kept, unitState]
    · have absent : successors condition = false := by cases h : successors condition <;> simp_all
      simp [subtract, branchState, absent, unitState]

theorem branch_live_runs (owned successors defined : Mask width) (condition : Local width)
    (conditionOwned : owned condition = true)
    (available : ∀ name, successors name = true → owned name = true) :
    runEvents (branchEvents owned successors condition) (unitState owned defined) =
      some (unitState successors defined) := by
  rw [branch_runs owned successors defined condition conditionOwned, retained_eq owned successors available]

/-- Partial execution cannot fail at an earlier event and be repaired by
later events. This applies directly to all successful helper runs above. -/
theorem successful_prefix (before after : List (Event width)) (state final : State width)
    (success : runEvents (before ++ after) state = some final) :
    ∃ middle, runEvents before state = some middle := by
  rw [runEvents_append] at success
  cases h : runEvents before state with
  | none => simp [h] at success
  | some middle => exact ⟨middle, rfl⟩

end Flick.Memory.Insert
