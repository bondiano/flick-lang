import Formal.MemoryAggregate.RepCheck

/-!
# Representation-aware ownership insertion step lemmas (#744)

Defines `annotateRep`, `prepareRep`, `RepPendingValid`, and proves the single-step
invariant `prepareRep_live_step` showing that deferring only non-aggregate cell
borrows into cell constructors preserves both token balance and `checkRepOk`.
-/

namespace Flick.Memory.Aggregate

open Flick.Memory.Pass
open Flick.Memory.Insert

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

def nextCellConstruct : RepBlock width → Bool
  | .seq (.let_ _ (.construct .cell ..)) _ => true
  | _ => false

/-- Only a non-aggregate `borrow receiver .cell` immediately followed by a
`construct .cell` may defer its drop into `reuse`. Aggregates and `unboxBorrow`
never defer. -/
def canDeferRep (op : RepOp width) (isAgg : Mask width) (next : RepBlock width) : Bool :=
  match op with
  | .borrow receiver .cell => !isAgg receiver && nextCellConstruct next
  | _ => false

def selectRepValue (pending : Option (Local width)) (op : RepOp width) : Option (RepOp width) :=
  match pending, op with
  | none, _ => some op
  | some receiver, .construct .cell fields evidence => some (.reuse receiver .cell fields evidence)
  | some _, _ => none

structure RepPlan (width : Nat) where
  instructions : List (RepInst width)
  owned : Mask width
  pending : Option (Local width)
  isAgg : Mask width

def finishRepInstruction (binding : Local width) (op : RepOp width)
    (after : Mask width) (defer : Bool) (owned isAgg : Mask width) : Option (RepPlan width) :=
  (stepRepOp op binding isAgg).map fun isAgg' =>
    let passOp := eraseOp op
    let owned := consumedOwners owned passOp after
    let owned := borrowOwners owned passOp after defer
    {
      instructions := copyRepInsts op after ++ [.let_ binding op] ++
        borrowRepDrops op after defer ++ (if after binding then [] else [.drop binding])
      owned := update owned binding (after binding)
      pending := borrowPending passOp after defer
      isAgg := isAgg'
    }

def prepareRep (binding : Local width) (op : RepOp width) (after : Mask width)
    (next : RepBlock width) (owned isAgg : Mask width) (pending : Option (Local width)) :
    Option (RepPlan width) :=
  if sourceRepOp op then
    (selectRepValue pending op).bind fun selected =>
      finishRepInstruction binding selected after (canDeferRep selected isAgg next) owned isAgg
  else none

def annotateRepCore (block : RepBlock width) (owned isAgg : Mask width)
    (pending : Option (Local width)) (entry : Bool) : Option (RepBlock width) :=
  let needed := liveRep block
  let entryDrops := if entry then trimRepInsts owned needed else []
  let owned := if entry then retained owned needed else owned
  if entry && pending.isSome then none else
  match block with
  | .ret result =>
      if pending.isSome then none else
        some (prependRep (entryDrops ++ trimRepInsts owned (fun name => name == result)) (.ret result))
  | .seq (.let_ binding op) next => do
      let plan ← prepareRep binding op (liveRep next) next owned isAgg pending
      let rest ← annotateRepCore next plan.owned plan.isAgg plan.pending false
      pure (prependRep (entryDrops ++ plan.instructions) rest)
  | .seq (.dup _) _ | .seq (.drop _) _ => none
  | .branch condition yes no => do
      if pending.isSome then none else
        let successors := fun name => liveRep yes name || liveRep no name
        let duplicates : List (RepInst width) := if successors condition then [.dup condition] else []
        let book := branchBook owned successors condition
        let remaining := retained book successors
        let yesOut ← annotateRepCore yes remaining isAgg none true
        let noOut ← annotateRepCore no remaining isAgg none true
        pure (prependRep (entryDrops ++ duplicates ++ trimRepInsts book successors)
          (.branch condition yesOut noOut))

/-- Executable representation-aware ownership insertion pass (before validation). -/
def annotateRep (block : RepBlock width) (owned isAgg : Mask width) : Option (RepBlock width) :=
  annotateRepCore block owned isAgg none true

theorem checkRepOk_finishRep (binding : Local width) (op : RepOp width)
    (after : Mask width) (defer : Bool) (owned isAgg isAgg' : Mask width)
    (plan : RepPlan width) (rest : RepBlock width)
    (hstep : stepRepOp op binding isAgg = some isAgg')
    (hplan : finishRepInstruction binding op after defer owned isAgg = some plan)
    (hrest : checkRepOk rest isAgg' = true) :
    checkRepOk (prependRep plan.instructions rest) isAgg = true := by
  simp only [finishRepInstruction, hstep, Option.map_some, Option.some.injEq] at hplan
  subst hplan
  simp only [prependRep_append]
  rw [checkRepOk_pure_insts _ _ _ (copyRepInsts_pure op after)]
  change checkRepOk (.seq (.let_ binding op) _) isAgg = true
  simp only [checkRepOk, hstep]
  change checkRepOk (prependRep (borrowRepDrops op after defer) _) isAgg' = true
  rw [checkRepOk_pure_insts _ _ _ (borrowRepDrops_pure op after defer)]
  cases after binding with
  | true => exact hrest
  | false => exact hrest

/-- Pending invariant for representation-aware insertion: a pending receiver is
dead in `block`, `block` starts with a `.construct .cell`, AND `isAgg receiver = false`. -/
def RepPendingValid (block : RepBlock width) (isAgg : Mask width)
    (pending : Option (Local width)) : Prop :=
  ∀ receiver, pending = some receiver →
    liveRep block receiver = false ∧ nextCellConstruct block = true ∧ isAgg receiver = false

theorem repPendingValid_none (block : RepBlock width) (isAgg : Mask width) :
    RepPendingValid block isAgg none := by
  intro _ h; cases h

theorem repPending_none_of_not_cell (block : RepBlock width) (isAgg : Mask width)
    (pending : Option (Local width)) (valid : RepPendingValid block isAgg pending)
    (hnot : nextCellConstruct block = false) : pending = none := by
  cases pending with
  | none => rfl
  | some r =>
      have h := (valid r rfl).2.1
      simp [hnot] at h

theorem stepRepOp_source (op : RepOp width) (binding : Local width) (isAgg : Mask width)
    (hsrc : sourceRepOp op = true) :
    ∃ isAgg', stepRepOp op binding isAgg = some isAgg' := by
  cases op with
  | dropReuse _ => simp [sourceRepOp] at hsrc
  | reuse _ _ _ _ => simp [sourceRepOp] at hsrc
  | consume _ => exact ⟨_, rfl⟩
  | boxAggregate _ => exact ⟨_, rfl⟩
  | unboxBorrow _ => exact ⟨_, rfl⟩
  | unboxConsume _ => exact ⟨_, rfl⟩
  | construct _ _ _ => exact ⟨_, rfl⟩
  | borrow _ rep =>
      cases rep <;> exact ⟨_, rfl⟩

theorem prepareRep_live_step (binding : Local width) (op : RepOp width)
    (next : RepBlock width) (defined isAgg : Mask width) (pending : Option (Local width))
    (source : checkRepSource (.seq (.let_ binding op) next) defined = true)
    (ownersDefined : ∀ name,
      withPending (liveRep (.seq (.let_ binding op) next)) pending name = true → defined name = true)
    (valid : RepPendingValid (.seq (.let_ binding op) next) isAgg pending) :
    ∃ plan,
      prepareRep binding op (liveRep next) next
        (withPending (liveRep (.seq (.let_ binding op) next)) pending) isAgg pending = some plan ∧
      plan.owned = withPending (liveRep next) plan.pending ∧
      runEvents (instructionTrace (plan.instructions.map eraseInst))
        (unitState (withPending (liveRep (.seq (.let_ binding op) next)) pending) defined) =
          some (unitState plan.owned (update defined binding true)) ∧
      (∀ name, plan.owned name = true → update defined binding true name = true) ∧
      RepPendingValid next plan.isAgg plan.pending ∧
      (∀ rest, checkRepOk rest plan.isAgg = true →
        checkRepOk (prependRep plan.instructions rest) isAgg = true) := by
  have parts := source_let_parts binding (eraseOp op) (eraseBlock next) defined source
  have hsrcRep : sourceRepOp op = true := by rw [sourceRepOp_eq]; exact parts.1
  cases pending with
  | none =>
      simp only [withPending_none] at ownersDefined ⊢
      let owned := liveRep (.seq (.let_ binding op) next)
      let defer := canDeferRep op isAgg next
      obtain ⟨isAgg', hstep⟩ := stepRepOp_source op binding isAgg hsrcRep
      let passPlan := finishInstruction binding (eraseOp op) (liveRep next) defer owned
      let plan : RepPlan width := {
        instructions := copyRepInsts op (liveRep next) ++ [.let_ binding op] ++
          borrowRepDrops op (liveRep next) defer ++ (if liveRep next binding then [] else [.drop binding])
        owned := passPlan.owned
        pending := passPlan.pending
        isAgg := isAgg'
      }
      have hfin : finishRepInstruction binding op (liveRep next) defer owned isAgg = some plan := by
        simp [finishRepInstruction, hstep, plan, passPlan, finishInstruction]
      have hprep : prepareRep binding op (liveRep next) next owned isAgg none = some plan := by
        simp [prepareRep, hsrcRep, selectRepValue, defer, hfin]
      have herase : plan.instructions.map eraseInst = passPlan.instructions := by
        simp only [plan, passPlan, finishInstruction, List.map_append,
          eraseInst_copyRepInsts, eraseInst_borrowRepDrops]
        cases liveRep next binding <;> rfl
      have available : ∀ name, name ∈ operands (eraseOp op) → owned name = true :=
        fun name mem => before_operand binding (eraseOp op) (liveRep next) name mem
      have shape : plan.owned = withPending (liveRep next) plan.pending := by
        change passPlan.owned = withPending (liveRep next) passPlan.pending
        cases hb : borrowed (eraseOp op) with
        | none =>
            have noPending : passPlan.pending = none := by
              simp [passPlan, finishInstruction, borrowPending, hb]
            rw [noPending, withPending_none]
            exact finish_nonborrow_shape binding (eraseOp op) (liveRep next) defer hb
        | some receiver =>
            have hsame := borrowed_value (eraseOp op) receiver hb
            have receiverOwned : owned receiver = true :=
              available receiver (by simp [operands, hb])
            have receiverDefined := ownersDefined receiver receiverOwned
            have different := source_binding_fresh binding receiver (eraseOp op) (eraseBlock next)
              defined source receiverDefined
            have howned : owned = before (.let_ binding (.borrow receiver)) (liveRep next) := by
              change before (.let_ binding (eraseOp op)) (liveRep next) = _
              rw [hsame]
            dsimp only [passPlan]
            rw [hsame, howned]
            exact finish_borrow_shape binding receiver (liveRep next) defer different
      refine ⟨plan, hprep, shape, ?_, ?_, ?_, ?_⟩
      · rw [herase]
        exact finishInstruction_runs binding (eraseOp op) (liveRep next) owned defined defer
          parts.2.1 ownersDefined available
      · exact finishInstruction_defined binding (eraseOp op) (liveRep next) owned defined defer
          ownersDefined
      · intro r hr
        have hpend : borrowPending (eraseOp op) (liveRep next) defer = some r := hr
        obtain ⟨hdead, hdefer⟩ := borrowPending_dead (eraseOp op) (liveRep next) defer r hpend
        cases op with
        | borrow recv rep =>
            cases rep with
            | aggregate => simp [defer, canDeferRep] at hdefer
            | cell =>
                simp only [eraseOp, borrowed, borrowPending, hdefer, Bool.and_true] at hpend
                by_cases hdeadRecv : (!liveRep next recv) = true
                · simp only [hdeadRecv, ite_true, Option.some.injEq] at hpend
                  subst hpend
                  simp only [defer, canDeferRep, Bool.and_eq_true, Bool.not_eq_true'] at hdefer
                  have recvOwned : owned recv = true :=
                    available recv (by simp [operands, eraseOp, borrowed])
                  have recvDef := ownersDefined recv recvOwned
                  have hne : recv ≠ binding :=
                    source_binding_fresh binding recv (eraseOp (.borrow recv .cell))
                      (eraseBlock next) defined source recvDef
                  have hisAgg' : isAgg' = update isAgg binding false := by
                    simp [stepRepOp, repResultIsAgg] at hstep
                    exact hstep.symm
                  refine ⟨hdead, hdefer.2, ?_⟩
                  simp [plan, hisAgg', update, hne, hdefer.1]
                · simp [hdeadRecv] at hpend
        | consume _ => simp [defer, canDeferRep] at hdefer
        | construct _ _ _ => simp [defer, canDeferRep] at hdefer
        | boxAggregate _ => simp [defer, canDeferRep] at hdefer
        | unboxBorrow _ => simp [defer, canDeferRep] at hdefer
        | unboxConsume _ => simp [defer, canDeferRep] at hdefer
        | dropReuse _ => simp [defer, canDeferRep] at hdefer
        | reuse _ _ _ _ => simp [defer, canDeferRep] at hdefer
      · intro rest hrest
        exact checkRepOk_finishRep binding op (liveRep next) defer owned isAgg isAgg' plan rest
          hstep hfin hrest
  | some receiver =>
      have ⟨_, hcell, hnotAgg⟩ := valid receiver rfl
      cases op with
      | consume _ => simp [nextCellConstruct] at hcell
      | borrow _ _ => simp [nextCellConstruct] at hcell
      | boxAggregate _ => simp [nextCellConstruct] at hcell
      | unboxBorrow _ => simp [nextCellConstruct] at hcell
      | unboxConsume _ => simp [nextCellConstruct] at hcell
      | dropReuse _ => simp [nextCellConstruct] at hcell
      | reuse _ _ _ _ => simp [nextCellConstruct] at hcell
      | construct rep fields evidence =>
          cases rep with
          | aggregate => simp [nextCellConstruct] at hcell
          | cell =>
              let owned := withPending (liveRep (.seq (.let_ binding (.construct .cell fields evidence)) next)) (some receiver)
              let emitted : RepOp width := .reuse receiver .cell fields evidence
              have hstep : stepRepOp emitted binding isAgg = some (update isAgg binding false) := by
                simp [emitted, stepRepOp, hnotAgg]
              let passPlan := finishInstruction binding (eraseOp emitted) (liveRep next) false owned
              let plan : RepPlan width := {
                instructions := copyRepInsts emitted (liveRep next) ++ [.let_ binding emitted] ++
                  borrowRepDrops emitted (liveRep next) false ++
                  (if liveRep next binding then [] else [.drop binding])
                owned := passPlan.owned
                pending := passPlan.pending
                isAgg := update isAgg binding false
              }
              have hfin : finishRepInstruction binding emitted (liveRep next)
                  (canDeferRep emitted isAgg next) owned isAgg = some plan := by
                simp [canDeferRep, emitted, finishRepInstruction, hstep, plan, passPlan, finishInstruction]
              have hprep : prepareRep binding (.construct .cell fields evidence) (liveRep next) next
                  owned isAgg (some receiver) = some plan := by
                simp only [prepareRep, sourceRepOp, selectRepValue, ite_true, Option.bind_some]
                exact hfin
              have herase : plan.instructions.map eraseInst = passPlan.instructions := by
                simp only [plan, passPlan, finishInstruction, List.map_append,
                  eraseInst_copyRepInsts, eraseInst_borrowRepDrops]
                cases liveRep next binding <;> rfl
              have available : ∀ name, name ∈ operands (eraseOp emitted) → owned name = true := by
                intro name mem
                have hcases : name = receiver ∨ name ∈ fields ++ evidence := by
                  simpa only [emitted, eraseOp, operands, consumed, borrowed, Option.toList_none,
                    List.append_nil, List.mem_cons] using mem
                rcases hcases with hsame | mem'
                · subst hsame
                  exact withPending_receiver _ name
                · apply withPending_live
                  exact before_operand binding (.construct fields evidence) (liveRep next) name
                    (by simpa [operands, consumed, borrowed] using mem')
              have shape : plan.owned = withPending (liveRep next) plan.pending := by
                have noPending : plan.pending = none := rfl
                rw [noPending, withPending_none]
                change (finishInstruction binding (.reuse receiver fields evidence) (liveRep next) false
                  (withPending (before (.let_ binding (.construct fields evidence)) (liveRep next)) (some receiver))).owned = _
                rw [before_reuse]
                exact finish_nonborrow_shape binding (.reuse receiver fields evidence) (liveRep next) false rfl
              refine ⟨plan, hprep, shape, ?_, ?_, ?_, ?_⟩
              · rw [herase]
                exact finishInstruction_runs binding (eraseOp emitted) (liveRep next) owned defined false
                  parts.2.1 ownersDefined available
              · exact finishInstruction_defined binding (eraseOp emitted) (liveRep next) owned defined false
                  ownersDefined
              · exact repPendingValid_none next (update isAgg binding false)
              · intro rest hrest
                exact checkRepOk_finishRep binding emitted (liveRep next) false owned isAgg
                  (update isAgg binding false) plan rest hstep
                  (by simpa [canDeferRep, emitted] using hfin) hrest

end Flick.Memory.Aggregate
