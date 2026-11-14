import Formal.MemoryInsert.Entry

/-! # Whole-block and function-entry balance -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-- Structural correctness of an already entered block. The invariant is
one token per computed live local plus at most one disjoint pending local;
`prepare_live_step` derives, rather than assumes, its successor instance. -/
theorem annotateBody_balanced (block : Block width) :
    ∀ defined pending,
      checkSource block defined = true →
      (∀ name, withPending (live block) pending name = true → defined name = true) →
      PendingValid block pending →
      ∃ output,
        annotateCore block (withPending (live block) pending) pending false = some output ∧
        checkTokens output (unitState (withPending (live block) pending) defined) = true := by
  induction block with
  | ret result =>
      intro defined pending source ownersDefined valid
      have noPending := pending_none_of_not_constructor (.ret result) pending valid rfl
      subst pending
      simp only [withPending_none]
      refine ⟨.ret result, ?_, return_check result defined⟩
      rw [annotateCore_ret]
      change some (prepend (trimInstructions (live (.ret result)) (live (.ret result))) (.ret result)) = _
      rw [trimInstructions_same]
      rfl
  | seq instruction next ih =>
      intro defined pending source ownersDefined valid
      cases instruction with
      | dup name => simp [checkSource] at source
      | drop name => simp [checkSource] at source
      | let_ binding value =>
          obtain ⟨plan, prepared, shape, executed, postDefined, postValid⟩ :=
            prepare_live_step binding value next defined pending source ownersDefined valid
          have sourceNext := (source_let_parts binding value next defined source).2.2
          have ownersNext : ∀ name, withPending (live next) plan.pending name = true →
              update defined binding true name = true := by
            simpa only [shape] using postDefined
          obtain ⟨output, restAnnotated, restAccepted⟩ :=
            ih (update defined binding true) plan.pending sourceNext ownersNext postValid
          have restAnnotated' : annotateCore next plan.owned plan.pending false = some output := by
            simpa only [shape] using restAnnotated
          have restAccepted' : checkTokens output (unitState plan.owned (update defined binding true)) = true := by
            simpa only [shape] using restAccepted
          refine ⟨prepend plan.instructions output, ?_, ?_⟩
          · rw [annotateCore_seq]
            simp [prepared, restAnnotated']
          · exact checkTokens_prepend_of_run plan.instructions output _ _ executed restAccepted'
  | branch condition yes no ihYes ihNo =>
      intro defined pending source ownersDefined valid
      have noPending := pending_none_of_not_constructor (.branch condition yes no) pending valid rfl
      subst pending
      simp only [withPending_none] at ownersDefined ⊢
      have sources : defined condition = true ∧ checkSource yes defined = true ∧ checkSource no defined = true := by
        simpa only [checkSource, Bool.and_eq_true] using source
      let owned := live (.branch condition yes no)
      let successors := fun name => live yes name || live no name
      let book := branchBook owned successors condition
      let remaining := retained book successors
      have conditionOwned : owned condition = true := by
        simp [owned, live, analyze, Analysis.entry]
      have available : ∀ name, successors name = true → owned name = true := by
        intro name needed
        change ((live yes name || live no name) || name == condition) = true
        change (live yes name || live no name) = true at needed
        rw [needed]
        rfl
      have successorDefined : ∀ name, successors name = true → defined name = true := by
        intro name needed
        exact ownersDefined name (available name needed)
      have remainingEq : remaining = successors :=
        retained_eq book successors (branchBook_available owned successors condition available)
      have yesNeeded : ∀ name, live yes name = true → successors name = true := by
        intro name needed
        simp [successors, needed]
      have noNeeded : ∀ name, live no name = true → successors name = true := by
        intro name needed
        simp [successors, needed]
      have yesDefined : ∀ name, withPending (live yes) none name = true → defined name = true := by
        simp only [withPending_none]
        intro name needed
        exact successorDefined name (yesNeeded name needed)
      have noDefined : ∀ name, withPending (live no) none name = true → defined name = true := by
        simp only [withPending_none]
        intro name needed
        exact successorDefined name (noNeeded name needed)
      obtain ⟨yesBody, yesNormalized, yesChecked⟩ := ihYes defined none sources.2.1 yesDefined (pendingValid_none yes)
      obtain ⟨noBody, noNormalized, noChecked⟩ := ihNo defined none sources.2.2 noDefined (pendingValid_none no)
      simp only [withPending_none] at yesNormalized yesChecked noNormalized noChecked
      have yesLift := entry_lift yes successors defined yesBody yesNeeded yesNormalized yesChecked
      have noLift := entry_lift no successors defined noBody noNeeded noNormalized noChecked
      let yesOutput := prepend (trimInstructions successors (live yes)) yesBody
      let noOutput := prepend (trimInstructions successors (live no)) noBody
      have yesAnnotated : annotateCore yes remaining none true = some yesOutput := by
        simpa only [remainingEq] using yesLift.1
      have noAnnotated : annotateCore no remaining none true = some noOutput := by
        simpa only [remainingEq] using noLift.1
      let output := prepend (branchInstructions owned successors condition) (.branch condition yesOutput noOutput)
      refine ⟨output, ?_, ?_⟩
      · rw [annotateCore_branch]
        simp only [Option.isSome_none, Bool.and_false]
        change (do
          let left ← annotateCore yes remaining none true
          let right ← annotateCore no remaining none true
          pure (prepend (branchInstructions owned successors condition) (.branch condition left right))) = some output
        rw [yesAnnotated, noAnnotated]
        rfl
      · apply checkTokens_prepend_branch (branchInstructions owned successors condition) condition yesOutput noOutput
          (unitState owned defined) (unitState successors defined) _ yesLift.2 noLift.2
        rw [branchInstructions_trace]
        exact branch_live_runs owned successors defined condition conditionOwned available

/-- Ownership insertion is balanced *before* its final validator call.
The only premises are source binding validity, availability of computed
live-in locals, and the current owner-to-definition invariant. -/
theorem annotate_balanced (block : Block width) (defined owned : Mask width)
    (source : checkSource block defined = true)
    (available : ∀ name, live block name = true → owned name = true)
    (ownersDefined : ∀ name, owned name = true → defined name = true) :
    ∃ output, annotate block owned = some output ∧
      checkTokens output (unitState owned defined) = true := by
  have normalizedDefined : ∀ name, withPending (live block) none name = true → defined name = true := by
    simp only [withPending_none]
    intro name needed
    exact ownersDefined name (available name needed)
  obtain ⟨output, normalized, accepted⟩ :=
    annotateBody_balanced block defined none source normalizedDefined (pendingValid_none block)
  simp only [withPending_none] at normalized accepted
  have lifted := entry_lift block owned defined output available normalized accepted
  exact ⟨prepend (trimInstructions owned (live block)) output, lifted.1, lifted.2⟩

/-- The canonical defined-name inventory owns one token per source name.
This is not a list-parameter constructor: the function corollary below
separately requires parameter uniqueness, as the Rust pass does. -/
theorem annotate_defined_balanced (block : Block width) (defined : Mask width)
    (source : checkSource block defined = true) :
    ∃ output, annotate block defined = some output ∧
      checkTokens output (unitState defined defined) = true := by
  exact annotate_balanced block defined defined source (checkSource_live_defined block defined source)
    (fun _ present => present)

theorem annotate_path_safe (block : Block width) (defined owned : Mask width)
    (source : checkSource block defined = true)
    (available : ∀ name, live block name = true → owned name = true)
    (ownersDefined : ∀ name, owned name = true → defined name = true) (choices : List Bool) :
    ∃ output final,
      annotate block owned = some output ∧
      runEvents (path output choices) (unitState owned defined) = some final ∧
      ∀ name, final.tokens name = 0 := by
  obtain ⟨output, annotated, accepted⟩ := annotate_balanced block defined owned source available ownersDefined
  obtain ⟨final, executed, empty⟩ := checkTokens_sound output (unitState owned defined) accepted choices
  exact ⟨output, final, annotated, executed, empty⟩

theorem annotate_prefix_safe (block : Block width) (defined owned : Mask width)
    (source : checkSource block defined = true)
    (available : ∀ name, live block name = true → owned name = true)
    (ownersDefined : ∀ name, owned name = true → defined name = true) (choices : List Bool) :
    ∃ output, annotate block owned = some output ∧
      ∀ before after, path output choices = before ++ after →
        ∃ middle, runEvents before (unitState owned defined) = some middle := by
  obtain ⟨output, annotated, accepted⟩ := annotate_balanced block defined owned source available ownersDefined
  refine ⟨output, annotated, ?_⟩
  intro before after split
  exact checkTokens_prefix_safe output (unitState owned defined) accepted choices before after split

def includeNames (mask : Mask width) (names : List (Local width)) : Mask width :=
  fun name => mask name || names.contains name

def parameterMask (parameters : List (Local width)) : Mask width :=
  fun name => parameters.contains name

theorem includeNames_nil (mask : Mask width) : includeNames mask [] = mask := by
  funext name
  simp [includeNames]

theorem includeNames_cons (mask : Mask width) (head : Local width) (tail : List (Local width)) :
    includeNames mask (head :: tail) = includeNames (update mask head true) tail := by
  funext name
  by_cases same : name = head
  · subst name
    simp [includeNames, update]
  · simp [includeNames, update, same]

/-- Parameter initialization checks freshness in list order; it does not
erase duplicate parameters by converting them to a set first. -/
theorem bindList_runs (names : List (Local width)) (owned defined : Mask width)
    (unique : names.Nodup) (fresh : ∀ name, name ∈ names → defined name = false) :
    runEvents (names.map Event.bind) (unitState owned defined) =
      some (unitState (includeNames owned names) (includeNames defined names)) := by
  induction names generalizing owned defined with
  | nil => simp only [List.map_nil, runEvents, includeNames_nil]
  | cons head tail ih =>
      have parts := List.nodup_cons.mp unique
      have headFresh := fresh head (List.mem_cons_self ..)
      have tailFresh : ∀ name, name ∈ tail → update defined head true name = false := by
        intro name member
        have different : name ≠ head := by
          intro same
          subst name
          exact parts.1 member
        have old := fresh name (List.mem_cons_of_mem head member)
        simpa [update, different] using old
      change runEvents ([.bind head] ++ tail.map Event.bind) (unitState owned defined) = _
      rw [runEvents_append, bind_unitState head owned defined headFresh]
      simp only [Option.bind_some]
      rw [ih (update owned head true) (update defined head true) parts.2 tailFresh]
      rw [← includeNames_cons owned head tail, ← includeNames_cons defined head tail]

theorem parameter_init_runs (parameters : List (Local width)) (unique : parameters.Nodup) :
    runEvents (parameters.map Event.bind) initial =
      some (unitState (parameterMask parameters) (parameterMask parameters)) := by
  have executed := bindList_runs parameters (fun _ => false) (fun _ => false) unique (by intros; rfl)
  simpa only [includeNames, parameterMask, Bool.false_or] using executed

/-- Function entry mirrors Rust's explicit rejection of duplicate parameter
names. Fresh names may still repeat in different branch siblings. -/
theorem annotate_function_balanced (parameters : List (Local width)) (block : Block width)
    (unique : parameters.Nodup)
    (source : checkSource block (parameterMask parameters) = true) :
    ∃ output, annotate block (parameterMask parameters) = some output ∧
      checkFunction parameters output = true := by
  obtain ⟨output, annotated, accepted⟩ := annotate_defined_balanced block (parameterMask parameters) source
  refine ⟨output, annotated, ?_⟩
  unfold checkFunction
  rw [parameter_init_runs parameters unique]
  exact accepted

end Flick.Memory.Insert
