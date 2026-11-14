import Formal.MemoryInsert.Instruction

/-! # Pending receiver masks and successor liveness -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

def pendingMask : Option (Local width) → Mask width
  | none => fun _ => false
  | some receiver => fun name => name == receiver

def withPending (needed : Mask width) (pending : Option (Local width)) : Mask width :=
  fun name => needed name || pendingMask pending name

def PendingValid (block : Block width) (pending : Option (Local width)) : Prop :=
  ∀ receiver, pending = some receiver → live block receiver = false ∧ nextConstructor block = true

theorem withPending_none (needed : Mask width) : withPending needed none = needed := by
  funext name
  simp [withPending, pendingMask]

theorem withPending_live (needed : Mask width) (pending : Option (Local width))
    (name : Local width) (isLive : needed name = true) : withPending needed pending name = true := by
  simp [withPending, isLive]

theorem withPending_receiver (needed : Mask width) (receiver : Local width) :
    withPending needed (some receiver) receiver = true := by
  simp [withPending, pendingMask]

theorem before_operand (binding : Local width) (value : Operation width) (after : Mask width)
    (name : Local width) (member : name ∈ operands value) :
    before (.let_ binding value) after name = true := by
  have present := List.contains_iff_mem.mpr member
  simpa only [before, Bool.or_eq_true] using (Or.inr present :
    (name != binding && after name) = true ∨ (operands value).contains name = true)

theorem consume_owner_arithmetic (keep used : Bool) :
    (if used && !keep then false else keep || used) = keep := by
  cases keep <;> cases used <;> rfl

/-- For a consuming source step its computed before mask is sufficient to
derive the after mask. The after-mask relation is a conclusion, not a premise. -/
theorem finish_nonborrow_shape (binding : Local width) (value : Operation width)
    (after : Mask width) (next : Bool) (noBorrow : borrowed value = none) :
    (finishInstruction binding value after next (before (.let_ binding value) after)).owned = after := by
  funext name
  by_cases same : name = binding
  · subst name
    simp [finishInstruction, update]
  · have different : (name != binding) = true := bne_iff_ne.mpr same
    simpa only [finishInstruction, borrowOwners, noBorrow, consumedOwners, update,
      beq_iff_eq, same, ite_false, before, operands, noBorrow, Option.toList_none,
      List.append_nil, different, Bool.true_and] using
      consume_owner_arithmetic (after name) ((consumed value).contains name)

theorem before_reuse (binding receiver : Local width) (fields evidence : List (Local width))
    (after : Mask width) :
    withPending (before (.let_ binding (.construct fields evidence)) after) (some receiver) =
      before (.let_ binding (.reuse receiver fields evidence)) after := by
  funext name
  simp only [withPending, pendingMask, before, operands, consumed, borrowed,
    Option.toList_none, List.append_nil, List.contains_cons]
  rw [Bool.or_assoc, Bool.or_comm ((fields ++ evidence).contains name) (name == receiver)]

/-- A borrowed receiver is kept exactly when the suffix needs it or the
actual immediate-constructor guard defers it. Fresh local names, not heap
object distinctness, discharge the result-binding case. -/
theorem finish_borrow_shape (binding receiver : Local width) (after : Mask width) (next : Bool)
    (different : receiver ≠ binding) :
    let plan := finishInstruction binding (.borrow receiver) after next
      (before (.let_ binding (.borrow receiver)) after)
    plan.owned = withPending after plan.pending := by
  have cases : after receiver = true ∨ after receiver = false := by cases after receiver <;> simp
  rcases cases with kept | dead
  · cases next <;> funext name <;> apply Bool.eq_iff_iff.mpr <;>
      by_cases bindingCase : name = binding <;> by_cases receiverCase : name = receiver <;>
      simp_all [finishInstruction, consumedOwners, borrowOwners, borrowPending, before,
        operands, consumed, borrowed, withPending, pendingMask, update]
  · cases next <;> funext name <;> apply Bool.eq_iff_iff.mpr <;>
      by_cases bindingCase : name = binding <;> by_cases receiverCase : name = receiver <;>
      simp_all [finishInstruction, consumedOwners, borrowOwners, borrowPending, before,
        operands, consumed, borrowed, withPending, pendingMask, update]

theorem borrowed_value (value : Operation width) (receiver : Local width)
    (hb : borrowed value = some receiver) : value = .borrow receiver := by
  cases value <;> simp_all [borrowed]

theorem source_let_parts (binding : Local width) (value : Operation width) (next : Block width)
    (defined : Mask width) (source : checkSource (.seq (.let_ binding value) next) defined = true) :
    sourceOperation value = true ∧ defined binding = false ∧
      checkSource next (update defined binding true) = true := by
  have parts : sourceOperation value = true ∧ (operands value).all defined = true ∧
      (!defined binding) = true ∧ checkSource next (update defined binding true) = true := by
    simpa only [checkSource, Bool.and_eq_true] using source
  have fresh : defined binding = false := by
    cases h : defined binding with
    | false => rfl
    | true => simp [h] at parts
  exact ⟨parts.1, fresh, parts.2.2.2⟩

/-- This derives both the next owned mask and its pending invariant from
the computed source before mask. Neither is a premise of the step rule. -/
theorem prepare_live_step (binding : Local width) (value : Operation width) (next : Block width)
    (defined : Mask width) (pending : Option (Local width))
    (source : checkSource (.seq (.let_ binding value) next) defined = true)
    (ownersDefined : ∀ name,
      withPending (live (.seq (.let_ binding value) next)) pending name = true → defined name = true)
    (valid : PendingValid (.seq (.let_ binding value) next) pending) :
    ∃ plan,
      prepare binding value (live next) (nextConstructor next)
        (withPending (live (.seq (.let_ binding value) next)) pending) pending = some plan ∧
      plan.owned = withPending (live next) plan.pending ∧
      runEvents (instructionTrace plan.instructions)
        (unitState (withPending (live (.seq (.let_ binding value) next)) pending) defined) =
          some (unitState plan.owned (update defined binding true)) ∧
      (∀ name, plan.owned name = true → update defined binding true name = true) ∧
      PendingValid next plan.pending := by
  have parts := source_let_parts binding value next defined source
  cases pending with
  | none =>
      simp only [withPending_none] at ownersDefined ⊢
      let owned := live (.seq (.let_ binding value) next)
      let plan := finishInstruction binding value (live next) (nextConstructor next) owned
      have available : ∀ name, name ∈ operands value → owned name = true := by
        intro name member
        exact before_operand binding value (live next) name member
      have prepared : prepare binding value (live next) (nextConstructor next) owned none = some plan := by
        simp [prepare, parts.1, selectValue, plan]
      have shape : plan.owned = withPending (live next) plan.pending := by
        cases hb : borrowed value with
        | none =>
            have noPending : plan.pending = none := by simp [plan, finishInstruction, borrowPending, hb]
            rw [noPending, withPending_none]
            exact finish_nonborrow_shape binding value (live next) (nextConstructor next) hb
        | some receiver =>
            have same := borrowed_value value receiver hb
            subst value
            have receiverOwned : owned receiver = true := by
              apply available receiver
              simp [operands, consumed, borrowed]
            have receiverDefined := ownersDefined receiver receiverOwned
            have different := source_binding_fresh binding receiver (.borrow receiver) next defined source receiverDefined
            exact finish_borrow_shape binding receiver (live next) (nextConstructor next) different
      refine ⟨plan, prepared, shape, ?_, ?_, ?_⟩
      · exact finishInstruction_runs binding value (live next) owned defined (nextConstructor next)
          parts.2.1 ownersDefined available
      · exact finishInstruction_defined binding value (live next) owned defined (nextConstructor next) ownersDefined
      · intro receiver isPending
        exact prepare_pending_dead binding value (live next) (nextConstructor next) owned none plan receiver
          prepared isPending
  | some receiver =>
      have isConstructor := (valid receiver rfl).2
      cases value with
      | consume arguments => simp [nextConstructor] at isConstructor
      | borrow argument => simp [nextConstructor] at isConstructor
      | reuse candidate fields evidence => simp [nextConstructor] at isConstructor
      | construct fields evidence =>
          let owned := withPending (live (.seq (.let_ binding (.construct fields evidence)) next)) (some receiver)
          let emitted := Operation.reuse receiver fields evidence
          let plan := finishInstruction binding emitted (live next) (nextConstructor next) owned
          have available : ∀ name, name ∈ operands emitted → owned name = true := by
            intro name member
            have cases : name = receiver ∨ name ∈ fields ++ evidence := by
              simpa only [emitted, operands, consumed, borrowed, Option.toList_none,
                List.append_nil, List.mem_cons] using member
            rcases cases with same | member
            · subst name
              exact withPending_receiver _ receiver
            · apply withPending_live
              apply before_operand binding (.construct fields evidence) (live next) name
              simpa [operands, consumed, borrowed] using member
          have prepared : prepare binding (.construct fields evidence) (live next) (nextConstructor next)
              owned (some receiver) = some plan := rfl
          have shape : plan.owned = withPending (live next) plan.pending := by
            have noPending : plan.pending = none := rfl
            rw [noPending, withPending_none]
            change (finishInstruction binding (.reuse receiver fields evidence) (live next) (nextConstructor next)
              (withPending (before (.let_ binding (.construct fields evidence)) (live next)) (some receiver))).owned = _
            rw [before_reuse]
            exact finish_nonborrow_shape binding (.reuse receiver fields evidence) (live next) (nextConstructor next) rfl
          refine ⟨plan, prepared, shape, ?_, ?_, ?_⟩
          · exact finishInstruction_runs binding emitted (live next) owned defined (nextConstructor next)
              parts.2.1 ownersDefined available
          · exact finishInstruction_defined binding emitted (live next) owned defined (nextConstructor next) ownersDefined
          · intro candidate isPending
            exact prepare_pending_dead binding (.construct fields evidence) (live next) (nextConstructor next)
              owned (some receiver) plan candidate prepared isPending

end Flick.Memory.Insert
