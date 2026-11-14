import Formal.MemoryInsert.Ownership

/-! # Executable annotation and pending-reuse preparation -/

namespace Flick.Memory.Insert

open Flick.Memory.Pass

variable {width : Nat}

-- Prefer the direct equality proof to the generic Ord-derived instance;
-- the latter brings Classical.choice into otherwise constructive arithmetic.
local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

def prepend (instructions : List (Instruction width)) (block : Block width) : Block width :=
  instructions.foldr Block.seq block

def copyInstructions (value : Operation width) (after : Mask width) : List (Instruction width) :=
  (List.finRange width).flatMap fun name =>
    List.replicate (copies ((consumed value).count name) (after name)) (.dup name)

def consumedOwners (owned : Mask width) (value : Operation width) (after : Mask width) : Mask width :=
  fun name => if (consumed value).contains name && !after name then false else owned name

def selectValue (pending : Option (Local width)) (value : Operation width) : Option (Operation width) :=
  match pending, value with
  | none, _ => some value
  | some receiver, .construct fields evidence => some (.reuse receiver fields evidence)
  | some _, _ => none

def borrowPending (value : Operation width) (after : Mask width) (nextConstructor : Bool) :
    Option (Local width) :=
  match borrowed value with
  | none => none
  | some receiver => if !after receiver && nextConstructor then some receiver else none

def borrowDrops (value : Operation width) (after : Mask width) (nextConstructor : Bool) :
    List (Instruction width) :=
  match borrowed value with
  | none => []
  | some receiver => if !after receiver && !nextConstructor then [.drop receiver] else []

def borrowOwners (owned : Mask width) (value : Operation width)
    (after : Mask width) (nextConstructor : Bool) : Mask width :=
  match borrowed value with
  | none => owned
  | some receiver =>
      if !after receiver && !nextConstructor then update owned receiver false else owned

structure Plan (width : Nat) where
  instructions : List (Instruction width)
  owned : Mask width
  pending : Option (Local width)

/-- One Rust loop iteration after selecting a possible reuse candidate.
The dead-result Drop remains after any post-borrow receiver Drop. -/
def finishInstruction (binding : Local width) (value : Operation width)
    (after : Mask width) (nextConstructor : Bool) (owned : Mask width) : Plan width :=
  let owned := consumedOwners owned value after
  let owned := borrowOwners owned value after nextConstructor
  {
    instructions := copyInstructions value after ++ [.let_ binding value] ++
      borrowDrops value after nextConstructor ++ (if after binding then [] else [.drop binding])
    owned := update owned binding (after binding)
    pending := borrowPending value after nextConstructor
  }

def prepare (binding : Local width) (value : Operation width) (after : Mask width)
    (nextConstructor : Bool) (owned : Mask width) (pending : Option (Local width)) :
    Option (Plan width) :=
  if sourceOperation value then
    (selectValue pending value).map (fun selected =>
      finishInstruction binding selected after nextConstructor owned)
  else none

def nextConstructor : Block width → Bool
  | .seq (.let_ _ (.construct ..)) _ => true
  | _ => false

theorem nextConstructor_shape (block : Block width) (construct : nextConstructor block = true) :
    ∃ binding fields evidence next, block = .seq (.let_ binding (.construct fields evidence)) next := by
  cases block with
  | ret result => simp [nextConstructor] at construct
  | branch condition yes no => simp [nextConstructor] at construct
  | seq instruction next =>
      cases instruction with
      | dup name => simp [nextConstructor] at construct
      | drop name => simp [nextConstructor] at construct
      | let_ binding value =>
          cases value with
          | consume values => simp [nextConstructor] at construct
          | borrow receiver => simp [nextConstructor] at construct
          | reuse receiver fields evidence => simp [nextConstructor] at construct
          | construct fields evidence => exact ⟨binding, fields, evidence, next, rfl⟩

/-- `entry` distinguishes a block entrance from the next instruction of
that same block. Cached after/branch masks come from one analysis, as in
Rust. Invalid annotations, cache shapes and pending shapes fail explicitly;
the private Rust function reaches them only after source validation. -/
def annotateCached (block : Block width) (analysis : Analysis width) (owned : Mask width)
    (pending : Option (Local width)) (entry : Bool) : Option (Block width) :=
  let entryDrops := if entry then trimInstructions owned analysis.entry else []
  let owned := if entry then retained owned analysis.entry else owned
  if entry && pending.isSome then none else
  match block, analysis with
  | .ret result, .ret _ =>
      if pending.isSome then none else
        some (prepend (entryDrops ++ trimInstructions owned (fun name => name == result)) (.ret result))
  | .seq (.let_ binding value) next, .seq _ after tail => do
      let plan ← prepare binding value after (nextConstructor next) owned pending
      let rest ← annotateCached next tail plan.owned plan.pending false
      pure (prepend (entryDrops ++ plan.instructions) rest)
  | .branch condition yes no, .branch _ yesLive noLive => do
      if pending.isSome then none else
        let successors := fun name => yesLive.entry name || noLive.entry name
        let duplicates := if successors condition then [.dup condition] else []
        let book := branchBook owned successors condition
        let remaining := retained book successors
        let yes ← annotateCached yes yesLive remaining none true
        let no ← annotateCached no noLive remaining none true
        pure (prepend (entryDrops ++ duplicates ++ trimInstructions book successors)
          (.branch condition yes no))
  | _, _ => none
termination_by structural block

def annotateCore (block : Block width) (owned : Mask width)
    (pending : Option (Local width)) (entry : Bool) : Option (Block width) :=
  annotateCached block (analyze block) owned pending entry

/-- The executable insertion projection, with no post-validation call. -/
def annotate (block : Block width) (owned : Mask width) : Option (Block width) :=
  annotateCore block owned none true

/-- The guard that establishes the pending receiver's deadness is part of
the executable iteration, not an assumed disjointness conclusion. -/
theorem borrowPending_dead (value : Operation width) (after : Mask width)
    (nextConstructor : Bool) (receiver : Local width)
    (pending : borrowPending value after nextConstructor = some receiver) :
    after receiver = false ∧ nextConstructor = true := by
  cases hb : borrowed value with
  | none => simp [borrowPending, hb] at pending
  | some original =>
      by_cases guard : (!after original && nextConstructor) = true
      · have same : original = receiver := by simpa [borrowPending, hb, guard] using pending
        subst receiver
        have parts : (!after original) = true ∧ nextConstructor = true := by
          simpa only [Bool.and_eq_true] using guard
        have dead : after original = false := by cases h : after original <;> simp_all
        exact ⟨dead, parts.2⟩
      · simp [borrowPending, hb, guard] at pending

theorem prepare_pending_dead (binding : Local width) (value : Operation width)
    (after : Mask width) (next : Bool) (owned : Mask width)
    (previous : Option (Local width)) (plan : Plan width) (receiver : Local width)
    (prepared : prepare binding value after next owned previous = some plan)
    (pending : plan.pending = some receiver) : after receiver = false ∧ next = true := by
  cases source : sourceOperation value with
  | false => simp [prepare, source] at prepared
  | true =>
      cases selected : selectValue previous value with
      | none => simp [prepare, source, selected] at prepared
      | some emitted =>
          have same : finishInstruction binding emitted after next owned = plan := by
            simpa [prepare, source, selected] using prepared
          subst plan
          exact borrowPending_dead emitted after next receiver pending

/-- A receiver absent from a source constructor's computed live-in set
cannot also be one of that constructor's ordinary or evidence operands. -/
theorem dead_constructor_operand (binding receiver : Local width)
    (fields evidence : List (Local width)) (next : Block width)
    (dead : live (.seq (.let_ binding (.construct fields evidence)) next) receiver = false) :
    receiver ∉ fields ++ evidence := by
  intro member
  have present : (operands (.construct fields evidence)).contains receiver = true := by
    apply List.contains_iff_mem.mpr
    simpa [operands, consumed, borrowed] using member
  have needed : live (.seq (.let_ binding (.construct fields evidence)) next) receiver = true := by
    change ((receiver != binding && live next receiver) ||
      (operands (.construct fields evidence)).contains receiver) = true
    simpa only [Bool.or_eq_true] using (Or.inr present :
      (receiver != binding && live next receiver) = true ∨
        (operands (.construct fields evidence)).contains receiver = true)
  simp [dead] at needed

theorem pending_reuse_count (binding receiver : Local width)
    (fields evidence : List (Local width)) (next : Block width)
    (dead : live (.seq (.let_ binding (.construct fields evidence)) next) receiver = false) :
    (consumed (.reuse receiver fields evidence)).count receiver = 1 := by
  simp only [consumed, List.count_cons_self]
  rw [List.count_eq_zero.mpr (dead_constructor_operand binding receiver fields evidence next dead)]

theorem source_binding_fresh (binding receiver : Local width) (value : Operation width)
    (next : Block width) (defined : Mask width)
    (source : checkSource (.seq (.let_ binding value) next) defined = true)
    (receiverDefined : defined receiver = true) : receiver ≠ binding := by
  intro same
  subst receiver
  simp [checkSource, receiverDefined] at source

theorem dead_before_suffix (binding receiver : Local width) (value : Operation width)
    (next : Block width) (different : receiver ≠ binding)
    (dead : live (.seq (.let_ binding value) next) receiver = false) :
    live next receiver = false := by
  have needed : live next receiver = true →
      live (.seq (.let_ binding value) next) receiver = true := by
    intro after
    change ((receiver != binding && live next receiver) || (operands value).contains receiver) = true
    simp [different, after]
  cases h : live next receiver with
  | false => rfl
  | true => have used := needed h
            simp [dead] at used

/-- Source freshness and computed deadness entail that reuse consumes the
pending token once and emits no copy of it. Local-name distinctness is
derived, not assumed; heap-object aliasing is outside this theorem. -/
theorem pending_reuse_no_copy (binding receiver : Local width)
    (fields evidence : List (Local width)) (next : Block width) (defined : Mask width)
    (source : checkSource (.seq (.let_ binding (.construct fields evidence)) next) defined = true)
    (receiverDefined : defined receiver = true)
    (dead : live (.seq (.let_ binding (.construct fields evidence)) next) receiver = false) :
    copies ((consumed (.reuse receiver fields evidence)).count receiver) (live next receiver) = 0 := by
  have different := source_binding_fresh binding receiver (.construct fields evidence) next defined
    source receiverDefined
  have afterDead := dead_before_suffix binding receiver (.construct fields evidence) next different dead
  rw [pending_reuse_count binding receiver fields evidence next dead, afterDead]
  rfl

/-- An actual pending token produced by `prepare` has a real next source
constructor; its receiver local is absent from the constructor operand
lists and needs no Dup when reused there. Receiver availability is a
source-name premise, not assumed local-name distinctness. This does not
prove heap-object nonaliasing or dynamic reuse eligibility. -/
theorem prepared_reuse_ready (binding : Local width) (value : Operation width)
    (next : Block width) (owned defined : Mask width) (previous : Option (Local width))
    (plan : Plan width) (receiver : Local width)
    (prepared : prepare binding value (live next) (nextConstructor next) owned previous = some plan)
    (pending : plan.pending = some receiver)
    (sourceNext : checkSource next defined = true)
    (receiverDefined : defined receiver = true) :
    ∃ nextBinding fields evidence rest,
      next = .seq (.let_ nextBinding (.construct fields evidence)) rest ∧
      selectValue (some receiver) (.construct fields evidence) = some (.reuse receiver fields evidence) ∧
      (consumed (.reuse receiver fields evidence)).count receiver = 1 ∧
      copies ((consumed (.reuse receiver fields evidence)).count receiver) (live rest receiver) = 0 := by
  obtain ⟨dead, isConstructor⟩ :=
    prepare_pending_dead binding value (live next) (nextConstructor next) owned previous plan receiver
      prepared pending
  obtain ⟨nextBinding, fields, evidence, rest, shape⟩ := nextConstructor_shape next isConstructor
  subst next
  refine ⟨nextBinding, fields, evidence, rest, rfl, rfl, ?_, ?_⟩
  · exact pending_reuse_count nextBinding receiver fields evidence rest dead
  · exact pending_reuse_no_copy nextBinding receiver fields evidence rest defined sourceNext receiverDefined dead

end Flick.Memory.Insert
