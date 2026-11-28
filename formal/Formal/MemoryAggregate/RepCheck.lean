import Formal.MemoryAggregate.Heap
import Formal.MemoryInsert.Balance

/-!
# Representation-aware static token checking for unboxed aggregates (#744)

Defines representation-annotated ANF operations (`RepOp`, `RepInst`, `RepBlock`),
erasure to `MemoryPass`, the representation discipline check `checkRepOk` and
combined static validator `checkRepTokens`, and proves rejection of aggregate
reuse candidates plus path soundness and prefix safety.
-/

namespace Flick.Memory.Aggregate

open Flick.Memory.Pass
open Flick.Memory.Insert

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-- Representation-annotated ANF operations including fallback-cell boxing/unboxing. -/
inductive RepOp (width : Nat) where
  | consume (operands : List (Local width))
  | borrow (receiver : Local width) (rep : ValueRep)
  | construct (rep : ValueRep) (fields evidence : List (Local width))
  | boxAggregate (value : Local width)
  | unboxBorrow (object : Local width)
  | unboxConsume (object : Local width)
  | dropReuse (object : Local width)
  | reuse (receiver : Local width) (rep : ValueRep) (fields evidence : List (Local width))
  deriving DecidableEq, Repr

inductive RepInst (width : Nat) where
  | let_ (binding : Local width) (value : RepOp width)
  | dup (name : Local width)
  | drop (name : Local width)
  deriving DecidableEq, Repr

inductive RepBlock (width : Nat) where
  | ret (result : Local width)
  | seq (instruction : RepInst width) (next : RepBlock width)
  | branch (condition : Local width) (yes no : RepBlock width)
  deriving DecidableEq, Repr

/-- Erase representation metadata to the underlying `MemoryPass` token projection. -/
def eraseOp : RepOp width → Pass.Operation width
  | .consume operands => .consume operands
  | .borrow receiver _ => .borrow receiver
  | .construct _ fields evidence => .construct fields evidence
  | .boxAggregate value => .consume [value]
  | .unboxBorrow object => .borrow object
  | .unboxConsume object => .consume [object]
  | .dropReuse object => .dropReuse object
  | .reuse receiver _ fields evidence => .reuse receiver fields evidence

def eraseInst : RepInst width → Pass.Instruction width
  | .let_ binding value => .let_ binding (eraseOp value)
  | .dup name => .dup name
  | .drop name => .drop name

def eraseBlock : RepBlock width → Pass.Block width
  | .ret result => .ret result
  | .seq inst next => .seq (eraseInst inst) (eraseBlock next)
  | .branch cond yes no => .branch cond (eraseBlock yes) (eraseBlock no)

def prependRep (instructions : List (RepInst width)) (block : RepBlock width) : RepBlock width :=
  instructions.foldr RepBlock.seq block

theorem prependRep_append (left right : List (RepInst width)) (b : RepBlock width) :
    prependRep (left ++ right) b = prependRep left (prependRep right b) := by
  induction left with
  | nil => rfl
  | cons i rest ih =>
      change RepBlock.seq i (prependRep (rest ++ right) b) =
        RepBlock.seq i (prependRep rest (prependRep right b))
      rw [ih]

theorem eraseBlock_prependRep (instructions : List (RepInst width)) (block : RepBlock width) :
    eraseBlock (prependRep instructions block) =
      prepend (instructions.map eraseInst) (eraseBlock block) := by
  induction instructions with
  | nil => rfl
  | cons inst rest ih =>
      change Pass.Block.seq (eraseInst inst) (eraseBlock (prependRep rest block)) =
        Pass.Block.seq (eraseInst inst) (prepend (rest.map eraseInst) (eraseBlock block))
      rw [ih]

def repResultIsAgg : RepOp width → Bool
  | .construct .aggregate .. | .unboxBorrow _ | .unboxConsume _ => true
  | _ => false

/-- Updates the static aggregate-representation mask and rejects aggregate reuse candidates. -/
def stepRepOp (op : RepOp width) (binding : Local width) (isAgg : Mask width) : Option (Mask width) :=
  match op with
  | .dropReuse obj =>
      if isAgg obj then none else some (update isAgg binding false)
  | .reuse recv rep .. =>
      if rep == .aggregate || isAgg recv then none else some (update isAgg binding false)
  | .borrow recv .aggregate =>
      some (update (update isAgg recv true) binding false)
  | _ =>
      some (update isAgg binding (repResultIsAgg op))

/-- Representation discipline check across every instruction and both branch successors. -/
def checkRepOk : RepBlock width → Mask width → Bool
  | .ret _, _ => true
  | .seq (.dup _) next, isAgg => checkRepOk next isAgg
  | .seq (.drop _) next, isAgg => checkRepOk next isAgg
  | .seq (.let_ binding op) next, isAgg =>
      match stepRepOp op binding isAgg with
      | none => false
      | some isAgg' => checkRepOk next isAgg'
  | .branch _ yes no, isAgg => checkRepOk yes isAgg && checkRepOk no isAgg

/-- Representation-aware validator state: token counts, definedness, and static aggregate mask. -/
structure RepState (width : Nat) where
  tokens : Local width → Nat
  defined : Local width → Bool
  isAgg : Mask width

def RepState.toPassState (s : RepState width) : Pass.State width :=
  ⟨s.tokens, s.defined⟩

def unitRepState (owned defined isAgg : Mask width) : RepState width :=
  ⟨fun name => if owned name then 1 else 0, defined, isAgg⟩

theorem unitRepState_toPass (owned defined isAgg : Mask width) :
    (unitRepState owned defined isAgg).toPassState = unitState owned defined := rfl

/-- Representation-aware static token validator mirroring `flick_ir::memory::validate`. -/
def checkRepTokens (block : RepBlock width) (state : RepState width) : Bool :=
  checkTokens (eraseBlock block) state.toPassState && checkRepOk block state.isAgg

/-- Whole-function representation-aware validator mirroring `checkFunction` with
duplicate-parameter rejection (`parameters.Nodup`) and incoming aggregate mask. -/
def checkRepFunction (parameters : List (Local width)) (block : RepBlock width)
    (paramIsAgg : Mask width) : Bool :=
  match runEvents (parameters.map Event.bind) initial with
  | none => false
  | some state => checkRepTokens block ⟨state.tokens, state.defined, paramIsAgg⟩

def checkRepSource (block : RepBlock width) (defined : Mask width) : Bool :=
  checkSource (eraseBlock block) defined

def liveRep (block : RepBlock width) : Mask width :=
  live (eraseBlock block)

def pathRep (block : RepBlock width) (choices : List Bool) : List (Event width) :=
  path (eraseBlock block) choices

theorem checkRepTokens_rejects_explicit_agg_reuse (binding recv : Local width)
    (fields evidence : List (Local width)) (next : RepBlock width) (state : RepState width) :
    checkRepTokens (.seq (.let_ binding (.reuse recv .aggregate fields evidence)) next) state = false := by
  simp [checkRepTokens, checkRepOk, stepRepOp]

theorem checkRepTokens_rejects_tracked_agg_reuse (binding recv : Local width)
    (rep : ValueRep) (fields evidence : List (Local width)) (next : RepBlock width)
    (state : RepState width) (hagg : state.isAgg recv = true) :
    checkRepTokens (.seq (.let_ binding (.reuse recv rep fields evidence)) next) state = false := by
  simp [checkRepTokens, checkRepOk, stepRepOp, hagg]

theorem checkRepTokens_rejects_agg_dropReuse (binding obj : Local width)
    (next : RepBlock width) (state : RepState width) (hagg : state.isAgg obj = true) :
    checkRepTokens (.seq (.let_ binding (.dropReuse obj)) next) state = false := by
  simp [checkRepTokens, checkRepOk, stepRepOp, hagg]

theorem checkRepTokens_sound (block : RepBlock width) (state : RepState width)
    (accepted : checkRepTokens block state = true) (choices : List Bool) :
    checkRepOk block state.isAgg = true ∧
      ∃ final, runEvents (pathRep block choices) state.toPassState = some final ∧
        ∀ name, final.tokens name = 0 := by
  simp only [checkRepTokens, Bool.and_eq_true] at accepted
  exact ⟨accepted.2, checkTokens_sound (eraseBlock block) state.toPassState accepted.1 choices⟩

theorem checkRepTokens_prefix_safe (block : RepBlock width) (state : RepState width)
    (accepted : checkRepTokens block state = true) (choices : List Bool)
    (before after : List (Event width)) (split : pathRep block choices = before ++ after) :
    ∃ next, runEvents before state.toPassState = some next := by
  simp only [checkRepTokens, Bool.and_eq_true] at accepted
  exact checkTokens_prefix_safe (eraseBlock block) state.toPassState accepted.1 choices before after split

def copyRepInsts (op : RepOp width) (after : Mask width) : List (RepInst width) :=
  (List.finRange width).flatMap fun name =>
    List.replicate (copies ((consumed (eraseOp op)).count name) (after name)) (.dup name)

def trimRepInsts (owned needed : Mask width) : List (RepInst width) :=
  (trimNames owned needed).map RepInst.drop

def borrowRepDrops (op : RepOp width) (after : Mask width) (defer : Bool) : List (RepInst width) :=
  match borrowed (eraseOp op) with
  | none => []
  | some receiver => if !after receiver && !defer then [.drop receiver] else []

def sourceRepOp : RepOp width → Bool
  | .dropReuse .. | .reuse .. => false
  | _ => true

theorem sourceRepOp_eq (op : RepOp width) :
    sourceRepOp op = sourceOperation (eraseOp op) := by
  cases op <;> rfl

theorem eraseInst_copyRepInsts (op : RepOp width) (after : Mask width) :
    (copyRepInsts op after).map eraseInst = copyInstructions (eraseOp op) after := by
  unfold copyRepInsts copyInstructions
  rw [List.map_flatMap]
  congr 1
  funext name
  induction copies ((consumed (eraseOp op)).count name) (after name) with
  | zero => rfl
  | succ k ih => simp [List.replicate_succ, eraseInst, ih]

theorem eraseInst_trimRepInsts (owned needed : Mask width) :
    (trimRepInsts owned needed).map eraseInst = trimInstructions owned needed := by
  unfold trimRepInsts trimInstructions
  rw [List.map_map]
  rfl

theorem eraseInst_borrowRepDrops (op : RepOp width) (after : Mask width) (defer : Bool) :
    (borrowRepDrops op after defer).map eraseInst = borrowDrops (eraseOp op) after defer := by
  unfold borrowRepDrops borrowDrops
  cases borrowed (eraseOp op) with
  | none => rfl
  | some r =>
      by_cases g : (!after r && !defer) = true <;> simp [g, eraseInst]

theorem checkRepOk_pure_insts (insts : List (RepInst width)) (b : RepBlock width)
    (isAgg : Mask width)
    (hpure : ∀ i ∈ insts, (∃ n, i = .dup n) ∨ (∃ n, i = .drop n)) :
    checkRepOk (prependRep insts b) isAgg = checkRepOk b isAgg := by
  induction insts with
  | nil => rfl
  | cons i rest ih =>
      have hi := hpure i (List.mem_cons_self ..)
      have hrest := ih (fun x hx => hpure x (List.mem_cons_of_mem i hx))
      rcases hi with ⟨n, rfl⟩ | ⟨n, rfl⟩
      · exact hrest
      · exact hrest

theorem copyRepInsts_pure (op : RepOp width) (after : Mask width) :
    ∀ i ∈ copyRepInsts op after, (∃ n, i = .dup n) ∨ (∃ n, i = .drop n) := by
  intro i hi
  simp only [copyRepInsts, List.mem_flatMap, List.mem_replicate] at hi
  rcases hi with ⟨n, _, _, rfl⟩
  exact Or.inl ⟨n, rfl⟩

theorem trimRepInsts_pure (owned needed : Mask width) :
    ∀ i ∈ trimRepInsts owned needed, (∃ n, i = .dup n) ∨ (∃ n, i = .drop n) := by
  intro i hi
  simp only [trimRepInsts, List.mem_map] at hi
  rcases hi with ⟨n, _, rfl⟩
  exact Or.inr ⟨n, rfl⟩

theorem borrowRepDrops_pure (op : RepOp width) (after : Mask width) (defer : Bool) :
    ∀ i ∈ borrowRepDrops op after defer, (∃ n, i = .dup n) ∨ (∃ n, i = .drop n) := by
  intro i hi
  unfold borrowRepDrops at hi
  cases h : borrowed (eraseOp op) with
  | none => simp [h] at hi
  | some r =>
      rw [h] at hi
      by_cases g : (!after r && !defer) = true
      · simp only [g, ite_true, List.mem_singleton] at hi
        exact Or.inr ⟨r, hi⟩
      · simp [g] at hi

end Flick.Memory.Aggregate
