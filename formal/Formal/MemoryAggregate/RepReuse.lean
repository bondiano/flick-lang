import Formal.MemoryAggregate.RepReuseBase

/-!
# Stage 2 representation-aware FBIP reuse preservation theorems (#463 / #744)
-/

namespace Flick.Memory.Aggregate

open Flick.Memory.Pass
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-- Pairing a non-aggregate reuse token `r` across any representation-valid
`RepBlock` preserves `checkRepOk` (never reusing an aggregate). -/
theorem repPairReuse_preserves_checkRepOk (b : RepBlock width) (r : Local width)
    (unused : localUnused r (eraseBlock b) = true) :
    ∀ isAgg, isAgg r = false → checkRepOk b isAgg = true →
      checkRepOk (repPairReuse r b) isAgg = true := by
  induction b with
  | ret _ => intro _ _ _; rfl
  | seq inst next ih =>
      intro isAgg hnotAgg h
      simp only [eraseBlock, localUnused, Bool.and_eq_true] at unused
      cases inst with
      | dup _ | drop _ => exact ih unused.2 isAgg hnotAgg h
      | let_ binding op =>
          simp only [repPairReuse]
          by_cases hm : repMatchesAlloc op = true
          · simp only [hm, ite_true]
            cases op with
            | construct rep fields evidence =>
                cases rep with
                | aggregate => simp [repMatchesAlloc] at hm
                | cell =>
                    simp only [repRewriteAlloc, checkRepOk, stepRepOp, repResultIsAgg,
                      hnotAgg] at h ⊢
                    exact h
            | _ => simp [repMatchesAlloc] at hm
          · have hmf : repMatchesAlloc op = false := Bool.eq_false_iff.mpr hm
            simp only [hmf, Bool.false_eq_true, ite_false, checkRepOk] at h ⊢
            cases hs : stepRepOp op binding isAgg with
            | none => simp [hs] at h
            | some isAgg' =>
                simp only [hs] at h ⊢
                have hnotAgg' : isAgg' r = false := by
                  rw [stepRepOp_unused_preserves_mask op binding r isAgg isAgg' unused.1 hs]
                  exact hnotAgg
                exact ih unused.2 isAgg' hnotAgg' h
  | branch cond yes no ihY ihN =>
      intro isAgg hnotAgg h
      simp only [eraseBlock, localUnused, Bool.and_eq_true] at unused
      simp only [checkRepOk, Bool.and_eq_true] at h
      simp only [repPairReuse]
      split
      · simp only [checkRepOk, Bool.and_eq_true]
        constructor
        · split
          · exact ihY unused.1.2 isAgg hnotAgg h.1
          · exact h.1
        · split
          · exact ihN unused.2 isAgg hnotAgg h.2
          · exact h.2
      · simp only [checkRepOk, Bool.and_eq_true]
        exact h

/-- Pairing a fresh reuse token `r` across any token-valid `RepBlock` preserves
`checkTokens` on its erased token projection. -/
theorem repPairReuse_preserves_checkTokens (b : RepBlock width) (r : Local width)
    (unused : localUnused r (eraseBlock b) = true) :
    ∀ state, state.tokens r = 0 → checkTokens (eraseBlock b) state = true →
      checkTokens (eraseBlock (repPairReuse r b)) (withFreshToken state r) = true := by
  induction b with
  | ret v =>
      intro state hzero h
      simp only [repPairReuse, eraseBlock, eraseInst, checkTokens, instructionEvents]
      rw [take_freshToken state r hzero]
      exact checkTokens_withDefined (.ret v) r unused state h
  | seq inst next ih =>
      intro state hzero h
      simp only [eraseBlock, localUnused, Bool.and_eq_true] at unused
      cases inst with
      | dup n =>
          simp only [repPairReuse, eraseBlock, eraseInst, checkTokens] at h ⊢
          cases hr : runEvents (instructionEvents (.dup n)) state with
          | none => simp [hr] at h
          | some after =>
              have hne := instEvents_unused (.dup n) r unused.1
              rw [runEvents_withFreshToken _ r hne state after hr]
              have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
              exact ih unused.2 after hz (by simpa [hr] using h)
      | drop n =>
          simp only [repPairReuse, eraseBlock, eraseInst, checkTokens] at h ⊢
          cases hr : runEvents (instructionEvents (.drop n)) state with
          | none => simp [hr] at h
          | some after =>
              have hne := instEvents_unused (.drop n) r unused.1
              rw [runEvents_withFreshToken _ r hne state after hr]
              have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
              exact ih unused.2 after hz (by simpa [hr] using h)
      | let_ binding op =>
          simp only [repPairReuse]
          by_cases hm : repMatchesAlloc op = true
          · simp only [hm, ite_true, eraseBlock, eraseInst, checkTokens] at h ⊢
            cases hr : runEvents (instructionEvents (.let_ binding (eraseOp op))) state with
            | none => simp [hr] at h
            | some after =>
                have hne := instEvents_unused (.let_ binding (eraseOp op)) r unused.1
                have hev : instructionEvents (.let_ binding (eraseOp (repRewriteAlloc r op))) =
                    [.take r] ++ instructionEvents (.let_ binding (eraseOp op)) := by
                  simp [instructionEvents, repRewriteAlloc_events r op hm]
                rw [hev, runEvents_append, take_freshToken state r hzero, Option.bind_some,
                  runEvents_withDefined _ r hne state after hr]
                exact checkTokens_withDefined (eraseBlock next) r unused.2 after (by simpa [hr] using h)
          · have hmf : repMatchesAlloc op = false := Bool.eq_false_iff.mpr hm
            simp only [hmf, Bool.false_eq_true, ite_false, eraseBlock, eraseInst, checkTokens] at h ⊢
            cases hr : runEvents (instructionEvents (.let_ binding (eraseOp op))) state with
            | none => simp [hr] at h
            | some after =>
                have hne := instEvents_unused (.let_ binding (eraseOp op)) r unused.1
                rw [runEvents_withFreshToken _ r hne state after hr]
                have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
                exact ih unused.2 after hz (by simpa [hr] using h)
  | branch c yes no ihY ihN =>
      intro state hzero h
      simp only [eraseBlock, localUnused, Bool.and_eq_true, bne_iff_ne] at unused
      simp only [repPairReuse]
      split
      · simp only [eraseBlock, checkTokens] at h ⊢
        cases hr : runEvents [.take c] state with
        | none => simp [hr] at h
        | some after =>
            have hne : ∀ e ∈ ([.take c] : List (Event width)), eventLocal e ≠ r := by
              intro e he; simp at he; subst he; exact unused.1.1
            rw [runEvents_withFreshToken _ r hne state after hr]
            have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
            simp only [hr, Bool.and_eq_true] at h
            simp only [Bool.and_eq_true]
            constructor
            · split
              · exact ihY unused.1.2 after hz h.1
              · simp only [eraseBlock, eraseInst, checkTokens, instructionEvents,
                  take_freshToken after r hz]
                exact checkTokens_withDefined (eraseBlock yes) r unused.1.2 after h.1
            · split
              · exact ihN unused.2 after hz h.2
              · simp only [eraseBlock, eraseInst, checkTokens, instructionEvents,
                  take_freshToken after r hz]
                exact checkTokens_withDefined (eraseBlock no) r unused.2 after h.2
      · simp only [eraseBlock, eraseInst, checkTokens, instructionEvents,
          take_freshToken state r hzero]
        exact checkTokens_withDefined (eraseBlock (.branch c yes no)) r
          (by simp [eraseBlock, localUnused, unused.1.1, unused.1.2, unused.2]) state h

/-- Combined Stage 2 representation-aware pairing theorem: preserves both token
balance and representation discipline (`checkRepTokens`). -/
theorem repPairReuse_preserves_checkRepTokens (b : RepBlock width) (r : Local width)
    (state : RepState width)
    (unused : localUnused r (eraseBlock b) = true)
    (hzero : state.tokens r = 0)
    (hnotAgg : state.isAgg r = false)
    (accepted : checkRepTokens b state = true) :
    checkRepTokens (repPairReuse r b)
      ⟨(withFreshToken state.toPassState r).tokens,
       (withFreshToken state.toPassState r).defined,
       state.isAgg⟩ = true := by
  simp only [checkRepTokens, Bool.and_eq_true] at accepted ⊢
  exact ⟨repPairReuse_preserves_checkTokens b r unused state.toPassState hzero accepted.1,
    repPairReuse_preserves_checkRepOk b r unused state.isAgg hnotAgg accepted.2⟩

/-- Main Stage 2 `#744`-aware FBIP theorem: replacing a dead non-aggregate
`Drop x` (`state.isAgg x = false`) with `r = DropReuse x` paired via
`repPairReuse` (never rewriting `.construct .aggregate`) preserves full
representation-aware static validation `checkRepTokens`. -/
theorem repDropToReuse_preserves_checkRepTokens (x r : Local width)
    (next : RepBlock width) (state : RepState width)
    (hxr : x ≠ r)
    (hzero : state.tokens r = 0)
    (hfresh : state.defined r = false)
    (hnotAggX : state.isAgg x = false)
    (unused : localUnused r (eraseBlock next) = true)
    (accepted : checkRepTokens (.seq (.drop x) next) state = true) :
    checkRepTokens (.seq (.let_ r (.dropReuse x)) (repPairReuse r next)) state = true := by
  simp only [checkRepTokens, Bool.and_eq_true] at accepted ⊢
  have htok : checkTokens (.seq (.drop x) (eraseBlock next)) state.toPassState = true := accepted.1
  have hrep : checkRepOk next state.isAgg = true := accepted.2
  constructor
  · simp only [eraseBlock, eraseInst, eraseOp, checkTokens, instructionEvents, operationEvents] at htok ⊢
    cases hr : runEvents [.take x] state.toPassState with
    | none => simp [hr] at htok
    | some after =>
        have hnextTok : checkTokens (eraseBlock next) after = true := by simpa [hr] using htok
        have hne : ∀ e ∈ ([.take x] : List (Event width)), eventLocal e ≠ r := by
          intro e he; simp at he; subst he; exact hxr
        have hafterZero : after.tokens r = 0 :=
          runEvents_preserves_zero_token [.take x] r hne state.toPassState after hzero hr
        have hafterFresh : after.defined r = false := by
          simp only [runEvents, Pass.step] at hr
          split at hr
          · contradiction
          · simp only [Option.bind_some, Option.some.injEq] at hr
            subst after
            exact hfresh
        have hstep : runEvents ([.take x] ++ [.bind r]) state.toPassState =
            some (withFreshToken after r) := by
          rw [runEvents_append, hr, Option.bind_some]
          simp [runEvents, Pass.step, hafterFresh, withFreshToken]
        rw [hstep]
        exact repPairReuse_preserves_checkTokens next r unused after hafterZero hnextTok
  · simp only [checkRepOk, stepRepOp, hnotAggX, Bool.false_eq_true, ite_false]
    have hrepUpd : checkRepOk next (update state.isAgg r false) = true :=
      checkRepOk_update_unused next r false unused state.isAgg hrep
    exact repPairReuse_preserves_checkRepOk next r unused (update state.isAgg r false)
      (by simp [update]) hrepUpd

/-- Prefix safety corollary for Stage 2 representation-aware FBIP reuse. -/
theorem repDropToReuse_prefix_safe (x r : Local width)
    (next : RepBlock width) (state : RepState width) (choices : List Bool)
    (hxr : x ≠ r) (hzero : state.tokens r = 0) (hfresh : state.defined r = false)
    (hnotAggX : state.isAgg x = false)
    (unused : localUnused r (eraseBlock next) = true)
    (accepted : checkRepTokens (.seq (.drop x) next) state = true)
    (before after : List (Event width))
    (split : pathRep (.seq (.let_ r (.dropReuse x)) (repPairReuse r next)) choices = before ++ after) :
    ∃ mid, runEvents before state.toPassState = some mid := by
  have ok := repDropToReuse_preserves_checkRepTokens x r next state hxr hzero hfresh hnotAggX unused accepted
  exact checkRepTokens_prefix_safe _ state ok choices before after split

end Flick.Memory.Aggregate
