import Formal.MemorySoundness.Exec.Prefix

/-!
# Stage 2 representation-aware reuse lifting and pipeline soundness (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-- Normal-execution soundness and source-equivalence theorem: executing any
`checkRepTokens`-accepted `MemBlock` to normal return yields a value denoting
the exact pure `evalSource` result, conserves global refcounts with zero leaked
frame tokens, and preserves every external alias bit-for-bit (`DenotesList`). -/
theorem evalMem_preserves_balance_and_denote (dropFuel : Nat)
    (block : MemBlock width) (choices : List Bool) (nextId bound : Nat) (h : AggHeap)
    (rank otherRoots : ObjId → Nat)
    (env : Local width → Option Aggregate.Value)
    (state : RepState width) (senv : Local width → Option SemVal)
    (ext : List Aggregate.Value) (sext : List SemVal)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value)
    (hchk : checkRepTokens (toRepBlock block) state = true)
    (heval : evalMemPrefix dropFuel .success (blockSteps block) block choices nextId h env
      state.tokens = some (.returned nextId' h' res))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state.toPassState)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    (hfuel : bound + blockSteps block ≤ dropFuel) :
    ∃ rank' sv,
      evalSource block choices senv = some sv ∧
      Denotes h' res sv ∧
      HeapBalanced nextId' h' rank' (bound + blockSteps block)
        (fun id => boxedTokens res id + otherRoots id) ∧
      DenotesList h' ext sext :=
  evalMemPrefix_sound dropFuel .success (blockSteps block) block choices nextId bound h rank
    otherRoots env state senv ext sext (.returned nextId' h' res) hchk heval hb henv hden hext
    hextRoots hfuel


/-! ## Lifting Stage 2 representation-aware reuse to `MemBlock` and pipeline corollary -/

def memMatchesAlloc : MemOp width → Bool
  | .construct .cell .. => true
  | _ => false

def memRewriteAlloc (r : Local width) : MemOp width → MemOp width
  | .construct .cell tag layout fields evidence => .reuse r .cell tag layout fields evidence
  | op => op

def hasCompatibleMemAlloc : MemBlock width → Bool
  | .ret _ => false
  | .seq (.let_ _ op) next => memMatchesAlloc op || hasCompatibleMemAlloc next
  | .seq _ next => hasCompatibleMemAlloc next
  | .branch _ yes no => hasCompatibleMemAlloc yes || hasCompatibleMemAlloc no

def memPairReuse (r : Local width) : MemBlock width → MemBlock width
  | .ret result => .seq (.drop r) (.ret result)
  | .seq (.let_ binding op) next =>
      if memMatchesAlloc op then
        .seq (.let_ binding (memRewriteAlloc r op)) next
      else
        .seq (.let_ binding op) (memPairReuse r next)
  | .seq inst next => .seq inst (memPairReuse r next)
  | .branch cond yes no =>
      let yesHas := hasCompatibleMemAlloc yes
      let noHas := hasCompatibleMemAlloc no
      if yesHas || noHas then
        .branch cond
          (if yesHas then memPairReuse r yes else .seq (.drop r) yes)
          (if noHas then memPairReuse r no else .seq (.drop r) no)
      else
        .seq (.drop r) (.branch cond yes no)

theorem toRepOp_memMatchesAlloc (op : MemOp width) :
    memMatchesAlloc op = repMatchesAlloc (toRepOp op) := by
  cases op with
  | construct rep _ _ _ _ => cases rep <;> rfl
  | _ => rfl

theorem toRepBlock_hasCompatible (b : MemBlock width) :
    hasCompatibleMemAlloc b = hasCompatibleRepAlloc (toRepBlock b) := by
  induction b with
  | ret _ => rfl
  | seq inst next ih =>
      cases inst with
      | dup _ | drop _ => exact ih
      | let_ _ op => simp [hasCompatibleMemAlloc, hasCompatibleRepAlloc, toRepBlock, toRepInst,
          toRepOp_memMatchesAlloc, ih]
  | branch _ yes no ihY ihN =>
      simp [hasCompatibleMemAlloc, hasCompatibleRepAlloc, toRepBlock, ihY, ihN]

theorem toRepBlock_memPairReuse (r : Local width) (b : MemBlock width) :
    toRepBlock (memPairReuse r b) = repPairReuse r (toRepBlock b) := by
  induction b with
  | ret _ => rfl
  | seq inst next ih =>
      cases inst with
      | dup _ | drop _ => simp [memPairReuse, repPairReuse, toRepBlock, toRepInst, ih]
      | let_ binding op =>
          simp only [memPairReuse, repPairReuse, toRepBlock, toRepInst, toRepOp_memMatchesAlloc]
          by_cases hm : repMatchesAlloc (toRepOp op) = true
          · simp only [hm, ite_true, toRepBlock, toRepInst]
            cases op with
            | construct rep tag layout fields evidence =>
                cases rep with
                | cell => rfl
                | aggregate => simp [toRepOp, repMatchesAlloc] at hm
            | _ => simp [toRepOp, repMatchesAlloc] at hm
          · have hmf : repMatchesAlloc (toRepOp op) = false := Bool.eq_false_iff.mpr hm
            simp [hmf, toRepBlock, toRepInst, ih]
  | branch cond yes no ihY ihN =>
      simp only [memPairReuse, repPairReuse, toRepBlock, toRepBlock_hasCompatible]
      by_cases h : (hasCompatibleRepAlloc (toRepBlock yes) || hasCompatibleRepAlloc (toRepBlock no)) = true
      · simp only [h, ite_true, toRepBlock]
        congr 1
        · by_cases hy : hasCompatibleRepAlloc (toRepBlock yes) = true <;> simp [hy, toRepBlock, toRepInst, ihY]
        · by_cases hn : hasCompatibleRepAlloc (toRepBlock no) = true <;> simp [hn, toRepBlock, toRepInst, ihN]
      · have hf : (hasCompatibleRepAlloc (toRepBlock yes) || hasCompatibleRepAlloc (toRepBlock no)) = false :=
          Bool.eq_false_iff.mpr h
        simp [hf, toRepBlock, toRepInst]

private theorem evalSourceOp_update_unused (op : MemOp width) (r : Local width)
    (senv : Local width → Option SemVal) (sv : Option SemVal)
    (hunused : localUnusedInOp r (eraseOp (toRepOp op)) = true) :
    evalSourceOp op (update senv r sv) = evalSourceOp op senv := by
  simp only [localUnusedInOp, Bool.not_eq_true'] at hunused
  have hnotmem : r ∉ operands (eraseOp (toRepOp op)) := by
    intro hmem
    have hc : (operands (eraseOp (toRepOp op))).contains r = true := List.contains_iff_mem.mpr hmem
    rw [hc] at hunused; cases hunused
  have hmapM : ∀ (xs : List (Local width)), r ∉ xs →
      xs.mapM (update senv r sv) = xs.mapM senv := by
    intro xs
    induction xs with
    | nil => intro _; rfl
    | cons x rest ih =>
        intro hnr
        have hne : x ≠ r := fun eq => hnr (eq ▸ List.mem_cons_self ..)
        have hbeq : (x == r) = false := beq_false_of_ne hne
        simp only [List.mapM_cons, update, hbeq, Bool.false_eq_true, ite_false]
        rw [ih (fun h => hnr (List.mem_cons_of_mem x h))]
  cases op with
  | lit => rfl
  | dropReuse _ => rfl
  | project recv rep idx =>
      have hne : recv ≠ r := fun eq => hnotmem (eq ▸ by simp [toRepOp, eraseOp, operands, borrowed])
      simp [evalSourceOp, update, beq_false_of_ne hne]
  | projectPath recv path =>
      have hne : recv ≠ r := fun eq => hnotmem (eq ▸ by simp [toRepOp, eraseOp, operands, borrowed])
      simp [evalSourceOp, update, beq_false_of_ne hne]
  | construct rep tag layout fields evidence =>
      have hnr : r ∉ fields ++ evidence := fun h => by
        apply hnotmem
        simp only [toRepOp, eraseOp, operands, consumed, borrowed, Option.toList_none, List.append_nil]
        exact h
      simp [evalSourceOp, hmapM (fields ++ evidence) hnr]
  | boxAggregate val layout =>
      have hne : val ≠ r := fun eq => hnotmem (eq ▸ by simp [toRepOp, eraseOp, operands, consumed])
      simp [evalSourceOp, update, beq_false_of_ne hne]
  | unboxBorrow obj =>
      have hne : obj ≠ r := fun eq => hnotmem (eq ▸ by simp [toRepOp, eraseOp, operands, borrowed])
      simp [evalSourceOp, update, beq_false_of_ne hne]
  | unboxConsume obj =>
      have hne : obj ≠ r := fun eq => hnotmem (eq ▸ by simp [toRepOp, eraseOp, operands, consumed])
      simp [evalSourceOp, update, beq_false_of_ne hne]
  | reuse recv rep tag layout fields evidence =>
      have hnr : r ∉ fields ++ evidence := fun h => by
        apply hnotmem
        simp only [toRepOp, eraseOp, operands, consumed, borrowed, Option.toList_none,
          List.append_nil, List.mem_cons]
        exact Or.inr h
      simp [evalSourceOp, hmapM (fields ++ evidence) hnr]

theorem evalSource_update_unused (b : MemBlock width) (r : Local width)
    (unused : localUnused r (eraseBlock (toRepBlock b)) = true) :
    ∀ choices senv sv, evalSource b choices (update senv r sv) = evalSource b choices senv := by
  induction b with
  | ret v =>
      intro choices senv sv
      have hne : v ≠ r := by simpa [toRepBlock, eraseBlock, localUnused] using unused
      simp [evalSource, update, beq_false_of_ne hne]
  | seq inst next ih =>
      intro choices senv sv
      simp only [toRepBlock, eraseBlock, localUnused, Bool.and_eq_true] at unused
      cases inst with
      | dup _ | drop _ => exact ih unused.2 choices senv sv
      | let_ binding op =>
          simp only [toRepInst, eraseInst, localUnusedInInst, Bool.and_eq_true, bne_iff_ne] at unused
          have hop := evalSourceOp_update_unused op r senv sv unused.1.2
          simp only [evalSource, hop]
          cases hs : evalSourceOp op senv with
          | none => rfl
          | some svB =>
              dsimp only
              have hcomm : update (update senv r sv) binding (some svB) =
                  update (update senv binding (some svB)) r sv := by
                funext k
                by_cases hkb : k = binding <;> by_cases hkr : k = r
                · subst k; exact (unused.1.1 hkr).elim
                · subst k; simp [update, beq_false_of_ne unused.1.1]
                · subst k; simp [update, beq_false_of_ne (Ne.symm unused.1.1)]
                · simp [update, beq_false_of_ne hkb, beq_false_of_ne hkr]
              rw [hcomm]
              exact ih unused.2 choices (update senv binding (some svB)) sv
  | branch c yes no ihY ihN =>
      intro choices senv sv
      simp only [toRepBlock, eraseBlock, localUnused, Bool.and_eq_true, bne_iff_ne] at unused
      have hbeq : (c == r) = false := beq_false_of_ne unused.1.1
      simp only [evalSource, update, hbeq, Bool.false_eq_true, ite_false]
      cases senv c with
      | none => rfl
      | some _ =>
          cases choices with
          | nil => exact ihN unused.2 [] senv sv
          | cons b rest =>
              cases b with
              | true => exact ihY unused.1.2 rest senv sv
              | false => exact ihN unused.2 rest senv sv

theorem memPairReuse_preserves_evalSource (b : MemBlock width) (r : Local width) :
    ∀ choices senv, evalSource (memPairReuse r b) choices senv = evalSource b choices senv := by
  induction b with
  | ret _ => intros; rfl
  | seq inst next ih =>
      intro choices senv
      cases inst with
      | dup _ | drop _ => exact ih choices senv
      | let_ binding op =>
          simp only [memPairReuse]
          by_cases hm : memMatchesAlloc op = true
          · simp only [hm, ite_true]
            cases op with
            | construct rep tag layout fields evidence =>
                cases rep with
                | cell => rfl
                | aggregate => simp [memMatchesAlloc] at hm
            | _ => simp [memMatchesAlloc] at hm
          · have hmf : memMatchesAlloc op = false := Bool.eq_false_iff.mpr hm
            simp only [hmf, Bool.false_eq_true, ite_false, evalSource]
            cases evalSourceOp op senv with
            | none => rfl
            | some sv => exact ih choices (update senv binding (some sv))
  | branch c yes no ihY ihN =>
      intro choices senv
      simp only [memPairReuse]
      by_cases h : (hasCompatibleMemAlloc yes || hasCompatibleMemAlloc no) = true
      · simp only [h, ite_true, evalSource]
        cases senv c with
        | none => rfl
        | some _ =>
            cases choices with
            | nil =>
                by_cases hn : hasCompatibleMemAlloc no = true
                · simp only [hn, ite_true]; exact ihN [] senv
                · simp [hn, evalSource]
            | cons b rest =>
                cases b with
                | true =>
                    by_cases hy : hasCompatibleMemAlloc yes = true
                    · simp only [hy, ite_true]; exact ihY rest senv
                    · simp [hy, evalSource]
                | false =>
                    by_cases hn : hasCompatibleMemAlloc no = true
                    · simp only [hn, ite_true]; exact ihN rest senv
                    · simp [hn, evalSource]
      · simp [h, evalSource]

/-- End-to-end Stage 2 reuse + counted execution + source equivalence corollary:
rewriting a dead non-aggregate `Drop x` into `r = DropReuse x` paired via
`memPairReuse` (never touching `.construct .aggregate`) passes `checkRepTokens`,
evaluates to a value denoting the exact `evalSource` result of the ORIGINAL
block before reuse, preserves global RC balance (`HeapBalanced`), and preserves
every external alias bit-for-bit. -/
theorem pipeline_source_equiv_and_balanced (dropFuel : Nat)
    (x r : Local width) (next : MemBlock width) (choices : List Bool)
    (nextId bound : Nat) (h : AggHeap) (rank otherRoots : ObjId → Nat)
    (env : Local width → Option Aggregate.Value)
    (state : RepState width) (senv : Local width → Option SemVal)
    (ext : List Aggregate.Value) (sext : List SemVal)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value)
    (hxr : x ≠ r)
    (hzero : state.tokens r = 0)
    (hfresh : state.defined r = false)
    (hnotAggX : state.isAgg x = false)
    (unused : localUnused r (eraseBlock (toRepBlock next)) = true)
    (accepted : checkRepTokens (toRepBlock (.seq (.drop x) next)) state = true)
    (heval : evalMemPrefix dropFuel .success
      (blockSteps (.seq (.let_ r (.dropReuse x)) (memPairReuse r next)))
      (.seq (.let_ r (.dropReuse x)) (memPairReuse r next))
      choices nextId h env state.tokens = some (.returned nextId' h' res))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state.toPassState)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    (hfuel : bound + blockSteps (.seq (.let_ r (.dropReuse x)) (memPairReuse r next)) ≤ dropFuel) :
    let rewritten : MemBlock width := .seq (.let_ r (.dropReuse x)) (memPairReuse r next)
    checkRepTokens (toRepBlock rewritten) state = true ∧
    ∃ rank' sv,
      evalSource (.seq (.drop x) next) choices senv = some sv ∧
      Denotes h' res sv ∧
      HeapBalanced nextId' h' rank' (bound + blockSteps rewritten)
        (fun id => boxedTokens res id + otherRoots id) ∧
      DenotesList h' ext sext := by
  let rewritten : MemBlock width := .seq (.let_ r (.dropReuse x)) (memPairReuse r next)
  have hchkRew : checkRepTokens (toRepBlock rewritten) state = true := by
    change checkRepTokens (.seq (.let_ r (.dropReuse x)) (toRepBlock (memPairReuse r next))) state = true
    rw [toRepBlock_memPairReuse]
    exact repDropToReuse_preserves_checkRepTokens x r (toRepBlock next) state
      hxr hzero hfresh hnotAggX unused accepted
  obtain ⟨rank', sv, hsrcRew, hdRes, hbOut, hextOut⟩ :=
    evalMem_preserves_balance_and_denote dropFuel rewritten choices nextId bound h rank otherRoots
      env state senv ext sext nextId' h' res hchkRew heval hb henv hden hext hextRoots hfuel
  have hsrcEq : evalSource rewritten choices senv = evalSource (.seq (.drop x) next) choices senv := by
    simp only [rewritten, evalSource, evalSourceOp]
    rw [memPairReuse_preserves_evalSource next r choices (update senv r (some .imm)),
      evalSource_update_unused next r unused choices senv (some .imm)]
  rw [hsrcEq] at hsrcRew
  exact ⟨hchkRew, rank', sv, hsrcRew, hdRes, hbOut, hextOut⟩

end Flick.Memory.Soundness
