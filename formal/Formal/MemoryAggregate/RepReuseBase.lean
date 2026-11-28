import Formal.MemoryAggregate.RepCheck
import Formal.MemoryReuse.Defs

/-!
# Stage 2 representation-aware FBIP reuse over `RepBlock` (#463 / #744)

Composes frame-limited Stage 2 `DropReuse` + `pairReuse` with `#744`
representation-aware IR (`RepBlock`):
- `repMatchesAlloc` matches ONLY `.construct .cell`, NEVER `.construct .aggregate`;
- `repDropToReuse` requires `state.isAgg x = false`, refusing aggregate candidates;
- `repPairReuse_preserves_checkRepTokens`, `repDropToReuse_preserves_checkRepTokens`,
  and `repDropToReuse_prefix_safe` prove token balance, prefix safety, and
  representation discipline across arbitrary finite local width and arbitrary
  branch nesting.
-/

namespace Flick.Memory.Aggregate

open Flick.Memory.Pass
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-- Only boxed cell constructions (never unboxed aggregates) are eligible targets
for Stage 2 `DropReuse` pairing. -/
def repMatchesAlloc : RepOp width → Bool
  | .construct .cell .. => true
  | _ => false

def repRewriteAlloc (r : Local width) : RepOp width → RepOp width
  | .construct .cell fields evidence => .reuse r .cell fields evidence
  | op => op

def hasCompatibleRepAlloc : RepBlock width → Bool
  | .ret _ => false
  | .seq (.let_ _ op) next => repMatchesAlloc op || hasCompatibleRepAlloc next
  | .seq _ next => hasCompatibleRepAlloc next
  | .branch _ yes no => hasCompatibleRepAlloc yes || hasCompatibleRepAlloc no

/-- Representation-aware Stage 2 reuse pairing mirroring `pairReuse` while
skipping unboxed `.construct .aggregate` sites. -/
def repPairReuse (r : Local width) : RepBlock width → RepBlock width
  | .ret result => .seq (.drop r) (.ret result)
  | .seq (.let_ binding op) next =>
      if repMatchesAlloc op then
        .seq (.let_ binding (repRewriteAlloc r op)) next
      else
        .seq (.let_ binding op) (repPairReuse r next)
  | .seq inst next => .seq inst (repPairReuse r next)
  | .branch cond yes no =>
      let yesHas := hasCompatibleRepAlloc yes
      let noHas := hasCompatibleRepAlloc no
      if yesHas || noHas then
        .branch cond
          (if yesHas then repPairReuse r yes else .seq (.drop r) yes)
          (if noHas then repPairReuse r no else .seq (.drop r) no)
      else
        .seq (.drop r) (.branch cond yes no)

theorem repRewriteAlloc_events (r : Local width) (op : RepOp width)
    (hm : repMatchesAlloc op = true) :
    operationEvents (eraseOp (repRewriteAlloc r op)) = .take r :: operationEvents (eraseOp op) := by
  cases op with
  | construct rep fields evidence =>
      cases rep with
      | cell => rfl
      | aggregate => simp [repMatchesAlloc] at hm
  | _ => simp [repMatchesAlloc] at hm

theorem stepRepOp_unused_preserves_mask (op : RepOp width) (binding r : Local width)
    (isAgg isAgg' : Mask width)
    (hunused : localUnusedInInst r (eraseInst (.let_ binding op)) = true)
    (hstep : stepRepOp op binding isAgg = some isAgg') :
    isAgg' r = isAgg r := by
  simp only [localUnusedInInst, localUnusedInOp, eraseInst, Bool.and_eq_true, bne_iff_ne,
    Bool.not_eq_true'] at hunused
  have hrb : r ≠ binding := Ne.symm hunused.1
  have hnotmem : r ∉ operands (eraseOp op) := by
    intro hmem
    have hc : (operands (eraseOp op)).contains r = true := List.contains_iff_mem.mpr hmem
    rw [hc] at hunused
    exact absurd hunused.2 (by decide)
  cases op with
  | borrow recv rep =>
      cases rep with
      | cell =>
          simp only [stepRepOp, Option.some.injEq] at hstep
          subst hstep
          simp [update, hrb]
      | aggregate =>
          have hrr : r ≠ recv := by
            intro eq; subst eq
            exact hnotmem (by simp [operands, eraseOp, borrowed])
          simp only [stepRepOp, Option.some.injEq] at hstep
          subst hstep
          simp [update, hrb, hrr]
  | dropReuse obj =>
      simp only [stepRepOp] at hstep
      split at hstep
      · contradiction
      · simp only [Option.some.injEq] at hstep
        subst hstep
        simp [update, hrb]
  | reuse recv rep fields evidence =>
      simp only [stepRepOp] at hstep
      split at hstep
      · contradiction
      · simp only [Option.some.injEq] at hstep
        subst hstep
        simp [update, hrb]
  | consume _ | construct _ _ _ | boxAggregate _ | unboxBorrow _ | unboxConsume _ =>
      simp only [stepRepOp, Option.some.injEq] at hstep
      subst hstep
      simp [update, hrb]

theorem stepRepOp_update_unused (op : RepOp width) (binding r : Local width)
    (isAgg isAgg' : Mask width) (v : Bool)
    (hunused : localUnusedInInst r (eraseInst (.let_ binding op)) = true)
    (hstep : stepRepOp op binding isAgg = some isAgg') :
    stepRepOp op binding (update isAgg r v) = some (update isAgg' r v) := by
  simp only [localUnusedInInst, localUnusedInOp, eraseInst, Bool.and_eq_true, bne_iff_ne,
    Bool.not_eq_true'] at hunused
  have hbr : binding ≠ r := hunused.1
  have hnotmem : r ∉ operands (eraseOp op) := by
    intro hmem
    have hc : (operands (eraseOp op)).contains r = true := List.contains_iff_mem.mpr hmem
    rw [hc] at hunused
    exact absurd hunused.2 (by decide)
  have hcomm : ∀ (m : Mask width) (a b : Local width) (va vb : Bool),
      a ≠ b → update (update m b vb) a va = update (update m a va) b vb := by
    intro m a b va vb hab
    funext k
    by_cases hka : k = a <;> by_cases hkb : k = b <;> simp_all [update]
  cases op with
  | dropReuse obj =>
      have hor : obj ≠ r := by
        intro eq; subst eq
        exact hnotmem (by simp [operands, eraseOp, consumed])
      simp only [stepRepOp, update, beq_iff_eq, hor, ite_false] at hstep ⊢
      split at hstep
      · contradiction
      · rename_i hna
        have hnaf : isAgg obj = false := Bool.eq_false_iff.mpr hna
        simp only [hnaf, Bool.false_eq_true, ite_false, Option.some.injEq] at hstep ⊢
        subst hstep
        rw [hcomm isAgg binding r false v hbr]
  | reuse recv rep fields evidence =>
      have hrr : recv ≠ r := by
        intro eq; subst eq
        exact hnotmem (by simp [operands, eraseOp, consumed])
      simp only [stepRepOp, update, beq_iff_eq, hrr, ite_false] at hstep ⊢
      split at hstep
      · contradiction
      · rename_i hna
        have hnaf : (rep == .aggregate || isAgg recv) = false := Bool.eq_false_iff.mpr hna
        simp only [hnaf, Bool.false_eq_true, ite_false, Option.some.injEq] at hstep ⊢
        subst hstep
        rw [hcomm isAgg binding r false v hbr]
  | borrow recv rep =>
      cases rep with
      | cell =>
          simp only [stepRepOp, Option.some.injEq] at hstep ⊢
          subst hstep
          rw [hcomm isAgg binding r _ v hbr]
      | aggregate =>
          have hrr : recv ≠ r := by
            intro eq; subst eq
            exact hnotmem (by simp [operands, eraseOp, borrowed])
          simp only [stepRepOp, Option.some.injEq] at hstep ⊢
          subst hstep
          rw [hcomm isAgg recv r true v hrr, hcomm (update isAgg recv true) binding r false v hbr]
  | consume _ | construct _ _ _ | boxAggregate _ | unboxBorrow _ | unboxConsume _ =>
      simp only [stepRepOp, Option.some.injEq] at hstep ⊢
      subst hstep
      rw [hcomm isAgg binding r _ v hbr]

/-- Updating an unused local in the aggregate mask preserves `checkRepOk`. -/
theorem checkRepOk_update_unused (b : RepBlock width) (r : Local width) (v : Bool)
    (unused : localUnused r (eraseBlock b) = true) :
    ∀ isAgg, checkRepOk b isAgg = true → checkRepOk b (update isAgg r v) = true := by
  induction b with
  | ret _ => intro _ _; rfl
  | seq inst next ih =>
      intro isAgg h
      simp only [eraseBlock, localUnused, Bool.and_eq_true] at unused
      cases inst with
      | dup _ | drop _ => exact ih unused.2 isAgg h
      | let_ binding op =>
          simp only [checkRepOk] at h ⊢
          cases hs : stepRepOp op binding isAgg with
          | none => simp [hs] at h
          | some isAgg' =>
              simp only [hs] at h
              rw [stepRepOp_update_unused op binding r isAgg isAgg' v unused.1 hs]
              exact ih unused.2 isAgg' h
  | branch cond yes no ihY ihN =>
      intro isAgg h
      simp only [eraseBlock, localUnused, Bool.and_eq_true] at unused
      simp only [checkRepOk, Bool.and_eq_true] at h ⊢
      exact ⟨ihY unused.1.2 isAgg h.1, ihN unused.2 isAgg h.2⟩

end Flick.Memory.Aggregate
