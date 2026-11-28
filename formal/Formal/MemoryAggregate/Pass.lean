import Formal.MemoryAggregate.RepInsert

/-!
# Whole-block representation-aware balance and prefix safety (#744)

Proves `annotateRep_balanced` and `annotateRep_prefix_safe` over arbitrary
finite local width and arbitrary branch nesting, combining token balance with
representation discipline (no aggregate reuse candidates).
-/

namespace Flick.Memory.Aggregate

open Flick.Memory.Pass
open Flick.Memory.Insert

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

private theorem annotateRepCore_entry (block : RepBlock width) (owned isAgg : Mask width) :
    annotateRepCore block owned isAgg none true =
      (annotateRepCore block (retained owned (liveRep block)) isAgg none false).map
        (prependRep (trimRepInsts owned (liveRep block))) := by
  cases block with
  | ret result =>
      simp [annotateRepCore, prependRep_append]
  | seq inst next =>
      cases inst with
      | dup _ | drop _ => rfl
      | let_ binding op =>
          simp only [annotateRepCore, Option.isSome_none, Bool.and_false, ite_true]
          cases hp : prepareRep binding op (liveRep next) next
              (retained owned (liveRep (.seq (.let_ binding op) next))) isAgg none with
          | none => simp [hp]
          | some plan =>
              cases hr : annotateRepCore next plan.owned plan.isAgg plan.pending false with
              | none => simp [hp, hr]
              | some out => simp [hp, hr, prependRep_append]
  | branch cond yes no =>
      simp only [annotateRepCore, Option.isSome_none, Bool.and_false, ite_true]
      let incoming := retained owned (liveRep (.branch cond yes no))
      let successors := fun name => liveRep yes name || liveRep no name
      let remaining := retained (branchBook incoming successors cond) successors
      cases hy : annotateRepCore yes remaining isAgg none true with
      | none => simp [incoming, successors, remaining] at hy; simp [hy]
      | some left =>
          cases hn : annotateRepCore no remaining isAgg none true with
          | none => simp [incoming, successors, remaining] at hy hn; simp [hy, hn]
          | some right =>
              simp [incoming, successors, remaining] at hy hn
              simp [hy, hn, prependRep_append]

private theorem entryRep_lift (block : RepBlock width) (owned defined isAgg : Mask width)
    (output : RepBlock width)
    (available : ∀ name, liveRep block name = true → owned name = true)
    (annotated : annotateRepCore block (liveRep block) isAgg none false = some output)
    (continuation : checkRepTokens output (unitRepState (liveRep block) defined isAgg) = true) :
    annotateRepCore block owned isAgg none true =
      some (prependRep (trimRepInsts owned (liveRep block)) output) ∧
    checkRepTokens (prependRep (trimRepInsts owned (liveRep block)) output)
      (unitRepState owned defined isAgg) = true := by
  simp only [checkRepTokens, Bool.and_eq_true, unitRepState_toPass] at continuation ⊢
  constructor
  · rw [annotateRepCore_entry, retained_eq owned (liveRep block) available, annotated]
    rfl
  · constructor
    · rw [eraseBlock_prependRep, eraseInst_trimRepInsts]
      apply checkTokens_prepend_of_run _ _ _ _ _ continuation.1
      rw [instructionTrace_trim]
      exact entry_trim_runs owned (liveRep block) defined available
    · rw [checkRepOk_pure_insts _ _ _ (trimRepInsts_pure owned (liveRep block))]
      exact continuation.2

private theorem annotateRepBody_balanced (block : RepBlock width) :
    ∀ defined isAgg pending,
      checkRepSource block defined = true →
      (∀ name, withPending (liveRep block) pending name = true → defined name = true) →
      RepPendingValid block isAgg pending →
      ∃ output,
        annotateRepCore block (withPending (liveRep block) pending) isAgg pending false = some output ∧
        checkRepTokens output (unitRepState (withPending (liveRep block) pending) defined isAgg) = true := by
  induction block with
  | ret result =>
      intro defined isAgg pending _ _ valid
      have noPending := repPending_none_of_not_cell (.ret result) isAgg pending valid rfl
      subst pending
      simp only [withPending_none]
      refine ⟨.ret result, ?_, ?_⟩
      · have htrim : trimRepInsts (liveRep (.ret result)) (fun name => name == result) = [] := by
          have h := trimInstructions_same (live (.ret result))
          unfold trimInstructions at h
          unfold trimRepInsts
          change (trimNames (live (.ret result)) (live (.ret result))).map RepInst.drop = []
          have hnil : trimNames (live (.ret result)) (live (.ret result)) = [] := by
            cases hm : trimNames (live (.ret result)) (live (.ret result)) with
            | nil => rfl
            | cons _ _ => simp [hm] at h
          simp [hnil]
        simp [annotateRepCore, htrim, prependRep]
      · simp only [checkRepTokens, Bool.and_eq_true, unitRepState_toPass, checkRepOk, and_true]
        exact return_check result defined
  | seq inst next ih =>
      intro defined isAgg pending source ownersDefined valid
      cases inst with
      | dup _ | drop _ => simp [checkRepSource, eraseBlock, eraseInst, checkSource] at source
      | let_ binding op =>
          obtain ⟨plan, hprep, hshape, hexec, hpostDef, hpostValid, hrepOk⟩ :=
            prepareRep_live_step binding op next defined isAgg pending source ownersDefined valid
          have sourceNext := (source_let_parts binding (eraseOp op) (eraseBlock next) defined source).2.2
          have ownersNext : ∀ name, withPending (liveRep next) plan.pending name = true →
              update defined binding true name = true := by
            simpa only [hshape] using hpostDef
          obtain ⟨rest, hrestAnn, hrestOk⟩ :=
            ih (update defined binding true) plan.isAgg plan.pending sourceNext ownersNext hpostValid
          have hrestAnn' : annotateRepCore next plan.owned plan.isAgg plan.pending false = some rest := by
            simpa only [hshape] using hrestAnn
          have hrestOk' : checkRepTokens rest (unitRepState plan.owned (update defined binding true) plan.isAgg) = true := by
            simpa only [hshape] using hrestOk
          simp only [checkRepTokens, Bool.and_eq_true, unitRepState_toPass] at hrestOk'
          refine ⟨prependRep plan.instructions rest, ?_, ?_⟩
          · simp [annotateRepCore, hprep, hrestAnn']
          · simp only [checkRepTokens, Bool.and_eq_true, unitRepState_toPass]
            constructor
            · rw [eraseBlock_prependRep]
              exact checkTokens_prepend_of_run _ _ _ _ hexec hrestOk'.1
            · exact hrepOk rest hrestOk'.2
  | branch cond yes no ihYes ihNo =>
      intro defined isAgg pending source ownersDefined valid
      have noPending := repPending_none_of_not_cell (.branch cond yes no) isAgg pending valid rfl
      subst pending
      simp only [withPending_none] at ownersDefined ⊢
      have sources : defined cond = true ∧ checkRepSource yes defined = true ∧ checkRepSource no defined = true := by
        simpa only [checkRepSource, eraseBlock, checkSource, Bool.and_eq_true] using source
      let owned := liveRep (.branch cond yes no)
      let successors := fun name => liveRep yes name || liveRep no name
      let book := branchBook owned successors cond
      let remaining := retained book successors
      have condOwned : owned cond = true := by simp [owned, liveRep, eraseBlock, live, analyze, Analysis.entry]
      have available : ∀ name, successors name = true → owned name = true := by
        intro name needed
        change ((liveRep yes name || liveRep no name) || name == cond) = true
        change (liveRep yes name || liveRep no name) = true at needed
        rw [needed]; rfl
      have succDef : ∀ name, successors name = true → defined name = true :=
        fun name h => ownersDefined name (available name h)
      have remEq : remaining = successors :=
        retained_eq book successors (branchBook_available owned successors cond available)
      have yesNeeded : ∀ name, liveRep yes name = true → successors name = true :=
        fun name h => by simp [successors, h]
      have noNeeded : ∀ name, liveRep no name = true → successors name = true :=
        fun name h => by simp [successors, h]
      obtain ⟨yesBody, yesNorm, yesChk⟩ := ihYes defined isAgg none sources.2.1
        (by simp only [withPending_none]; intro n h; exact succDef n (yesNeeded n h))
        (repPendingValid_none yes isAgg)
      obtain ⟨noBody, noNorm, noChk⟩ := ihNo defined isAgg none sources.2.2
        (by simp only [withPending_none]; intro n h; exact succDef n (noNeeded n h))
        (repPendingValid_none no isAgg)
      simp only [withPending_none] at yesNorm yesChk noNorm noChk
      have yesLift := entryRep_lift yes successors defined isAgg yesBody yesNeeded yesNorm yesChk
      have noLift := entryRep_lift no successors defined isAgg noBody noNeeded noNorm noChk
      let yesOut := prependRep (trimRepInsts successors (liveRep yes)) yesBody
      let noOut := prependRep (trimRepInsts successors (liveRep no)) noBody
      have yesAnn : annotateRepCore yes remaining isAgg none true = some yesOut := by
        simpa only [remEq] using yesLift.1
      have noAnn : annotateRepCore no remaining isAgg none true = some noOut := by
        simpa only [remEq] using noLift.1
      let dupInsts : List (RepInst width) := if successors cond then [.dup cond] else []
      let brInsts := dupInsts ++ trimRepInsts book successors
      let out := prependRep brInsts (.branch cond yesOut noOut)
      refine ⟨out, ?_, ?_⟩
      · simp only [annotateRepCore, Option.isSome_none, Bool.and_false]
        change (do
          let left ← annotateRepCore yes remaining isAgg none true
          let right ← annotateRepCore no remaining isAgg none true
          pure (prependRep ([] ++ dupInsts ++ trimRepInsts book successors) (.branch cond left right))) = some out
        rw [yesAnn, noAnn]
        rfl
      · simp only [checkRepTokens, Bool.and_eq_true, unitRepState_toPass] at yesLift noLift ⊢
        constructor
        · rw [eraseBlock_prependRep]
          have heraseBr : brInsts.map eraseInst = branchInstructions owned successors cond := by
            simp only [brInsts, dupInsts, branchInstructions, List.map_append, eraseInst_trimRepInsts]
            cases successors cond <;> rfl
          rw [heraseBr]
          apply checkTokens_prepend_branch (branchInstructions owned successors cond) cond
            (eraseBlock yesOut) (eraseBlock noOut) (unitState owned defined)
            (unitState successors defined) _ yesLift.2.1 noLift.2.1
          rw [branchInstructions_trace]
          exact branch_live_runs owned successors defined cond condOwned available
        · change checkRepOk (prependRep brInsts (.branch cond yesOut noOut)) isAgg = true
          rw [checkRepOk_pure_insts brInsts (.branch cond yesOut noOut) isAgg]
          · simp only [checkRepOk, Bool.and_eq_true]
            exact ⟨yesLift.2.2, noLift.2.2⟩
          · intro i hi
            simp only [brInsts, List.mem_append] at hi
            rcases hi with hdup | htrim
            · cases hsc : successors cond
              · simp [dupInsts, hsc] at hdup
              · simp [dupInsts, hsc] at hdup; exact Or.inl ⟨cond, hdup⟩
            · exact trimRepInsts_pure book successors i htrim

/-- Whole-block representation-aware balance theorem: for any source-valid
`RepBlock` with `live ⊆ owned ⊆ defined` and any incoming aggregate mask
`isAgg`, `annotateRep` produces a block that passes BOTH token balance AND
representation discipline (no aggregate reuse candidates). -/
theorem annotateRep_balanced (block : RepBlock width) (defined owned isAgg : Mask width)
    (source : checkRepSource block defined = true)
    (available : ∀ name, liveRep block name = true → owned name = true)
    (ownersDefined : ∀ name, owned name = true → defined name = true) :
    ∃ output, annotateRep block owned isAgg = some output ∧
      checkRepTokens output (unitRepState owned defined isAgg) = true := by
  have normDef : ∀ name, withPending (liveRep block) none name = true → defined name = true := by
    simp only [withPending_none]
    intro name h
    exact ownersDefined name (available name h)
  obtain ⟨output, hnorm, hok⟩ :=
    annotateRepBody_balanced block defined isAgg none source normDef (repPendingValid_none block isAgg)
  simp only [withPending_none] at hnorm hok
  have lifted := entryRep_lift block owned defined isAgg output available hnorm hok
  exact ⟨prependRep (trimRepInsts owned (liveRep block)) output, lifted.1, lifted.2⟩

/-- Prefix safety corollary: every selected branch path and every event prefix
of `annotateRep`'s output succeeds without token underflow and never violates
aggregate reuse discipline. -/
theorem annotateRep_prefix_safe (block : RepBlock width) (defined owned isAgg : Mask width)
    (source : checkRepSource block defined = true)
    (available : ∀ name, liveRep block name = true → owned name = true)
    (ownersDefined : ∀ name, owned name = true → defined name = true)
    (choices : List Bool) :
    ∃ output, annotateRep block owned isAgg = some output ∧
      checkRepOk output isAgg = true ∧
      ∀ before after, pathRep output choices = before ++ after →
        ∃ mid, runEvents before (unitState owned defined) = some mid := by
  obtain ⟨output, hann, hok⟩ := annotateRep_balanced block defined owned isAgg source available ownersDefined
  refine ⟨output, hann, (checkRepTokens_sound output _ hok choices).1, ?_⟩
  intro before after hsplit
  exact checkRepTokens_prefix_safe output (unitRepState owned defined isAgg) hok choices before after hsplit

/-- Whole-function representation-aware insertion corollary: distinct parameters
(`parameters.Nodup`) plus source validity entail balanced, aggregate-safe output
accepted by `checkRepFunction`. -/
theorem annotateRep_function_balanced (parameters : List (Local width)) (block : RepBlock width)
    (paramIsAgg : Mask width)
    (unique : parameters.Nodup)
    (source : checkRepSource block (parameterMask parameters) = true) :
    ∃ output, annotateRep block (parameterMask parameters) paramIsAgg = some output ∧
      checkRepFunction parameters output paramIsAgg = true := by
  have hlive : ∀ name, liveRep block name = true → parameterMask parameters name = true :=
    checkSource_live_defined (eraseBlock block) (parameterMask parameters) source
  obtain ⟨output, hann, hok⟩ :=
    annotateRep_balanced block (parameterMask parameters) (parameterMask parameters) paramIsAgg
      source hlive (fun _ h => h)
  refine ⟨output, hann, ?_⟩
  unfold checkRepFunction
  rw [parameter_init_runs parameters unique]
  exact hok

end Flick.Memory.Aggregate
