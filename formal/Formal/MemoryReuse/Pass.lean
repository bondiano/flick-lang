import Formal.MemoryReuse.Defs

/-!
# Frame-limited FBIP reuse pass: main preservation and prefix theorems (phase M3)

Proves whole-block token-balance preservation and prefix safety for
`DropReuse` + `pairReuse` across arbitrary finite local width and arbitrary
branch nesting.
-/

namespace Flick.Memory.Reuse

open Flick.Memory.Pass

variable {width : Nat}

/-- Pairing a fresh reuse token `r` across any token-valid block preserves
independent token validation on every branch. -/
theorem pairReuse_preserves_checkTokens (b : Block width) (r : Local width) (kind : TargetKind)
    (unused : localUnused r b = true) :
    ∀ state, state.tokens r = 0 → checkTokens b state = true →
      checkTokens (pairReuse r kind b) (withFreshToken state r) = true := by
  induction b with
  | ret v =>
      intro state hzero h
      simp only [pairReuse, checkTokens, instructionEvents]
      rw [take_freshToken state r hzero]
      exact checkTokens_withDefined (.ret v) r unused state h
  | seq inst next ih =>
      intro state hzero h
      simp only [localUnused, Bool.and_eq_true] at unused
      cases inst with
      | dup n =>
          simp only [pairReuse, checkTokens] at h ⊢
          cases hr : runEvents (instructionEvents (.dup n)) state with
          | none => simp [hr] at h
          | some after =>
              have hne := instEvents_unused (.dup n) r unused.1
              rw [runEvents_withFreshToken _ r hne state after hr]
              have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
              exact ih unused.2 after hz (by simpa [hr] using h)
      | drop n =>
          simp only [pairReuse, checkTokens] at h ⊢
          cases hr : runEvents (instructionEvents (.drop n)) state with
          | none => simp [hr] at h
          | some after =>
              have hne := instEvents_unused (.drop n) r unused.1
              rw [runEvents_withFreshToken _ r hne state after hr]
              have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
              exact ih unused.2 after hz (by simpa [hr] using h)
      | let_ binding op =>
          simp only [pairReuse]
          by_cases hm : matchesAlloc kind op = true
          · simp only [hm, ite_true, checkTokens] at h ⊢
            cases hr : runEvents (instructionEvents (.let_ binding op)) state with
            | none => simp [hr] at h
            | some after =>
                have hne := instEvents_unused (.let_ binding op) r unused.1
                have hev : instructionEvents (.let_ binding (rewriteAlloc r op)) =
                    [.take r] ++ instructionEvents (.let_ binding op) := by
                  simp [instructionEvents, rewriteAlloc_events r kind op hm]
                rw [hev, runEvents_append, take_freshToken state r hzero, Option.bind_some,
                  runEvents_withDefined _ r hne state after hr]
                exact checkTokens_withDefined next r unused.2 after (by simpa [hr] using h)
          · have hmf : matchesAlloc kind op = false := Bool.eq_false_iff.mpr hm
            simp only [hmf, Bool.false_eq_true, ite_false, checkTokens] at h ⊢
            cases hr : runEvents (instructionEvents (.let_ binding op)) state with
            | none => simp [hr] at h
            | some after =>
                have hne := instEvents_unused (.let_ binding op) r unused.1
                rw [runEvents_withFreshToken _ r hne state after hr]
                have hz := runEvents_preserves_zero_token _ r hne state after hzero hr
                exact ih unused.2 after hz (by simpa [hr] using h)
  | branch c yes no ihY ihN =>
      intro state hzero h
      simp only [localUnused, Bool.and_eq_true, bne_iff_ne] at unused
      simp only [pairReuse]
      split
      · simp only [checkTokens] at h ⊢
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
              · simp only [checkTokens, instructionEvents, take_freshToken after r hz]
                exact checkTokens_withDefined yes r unused.1.2 after h.1
            · split
              · exact ihN unused.2 after hz h.2
              · simp only [checkTokens, instructionEvents, take_freshToken after r hz]
                exact checkTokens_withDefined no r unused.2 after h.2
      · simp only [checkTokens, instructionEvents, take_freshToken state r hzero]
        exact checkTokens_withDefined (.branch c yes no) r
          (by simp [localUnused, unused.1.1, unused.1.2, unused.2]) state h

/-- Main M3 pass theorem: replacing a dead `Drop x` with `r = DropReuse x`
paired via `pairReuse` preserves independent token validation on all branches. -/
theorem dropToReuse_preserves_checkTokens (x r : Local width) (kind : TargetKind)
    (next : Block width) (state : State width)
    (hxr : x ≠ r)
    (hzero : state.tokens r = 0)
    (hfresh : state.defined r = false)
    (unused : localUnused r next = true)
    (accepted : checkTokens (.seq (.drop x) next) state = true) :
    checkTokens (.seq (.let_ r (.dropReuse x)) (pairReuse r kind next)) state = true := by
  simp only [checkTokens, instructionEvents, operationEvents] at accepted ⊢
  cases hr : runEvents [.take x] state with
  | none => simp [hr] at accepted
  | some after =>
      have acceptedNext : checkTokens next after = true := by simpa [hr] using accepted
      have hne : ∀ e ∈ ([.take x] : List (Event width)), eventLocal e ≠ r := by
        intro e he; simp at he; subst he; exact hxr
      have hafterZero : after.tokens r = 0 :=
        runEvents_preserves_zero_token [.take x] r hne state after hzero hr
      have hafterFresh : after.defined r = false := by
        simp only [runEvents, Pass.step] at hr
        split at hr
        · contradiction
        · simp only [Option.bind_some, Option.some.injEq] at hr
          subst after
          exact hfresh
      have hstep : runEvents ([.take x] ++ [.bind r]) state = some (withFreshToken after r) := by
        rw [runEvents_append, hr, Option.bind_some]
        simp [runEvents, Pass.step, hafterFresh, withFreshToken]
      rw [hstep]
      exact pairReuse_preserves_checkTokens next r kind unused after hafterZero acceptedNext

/-- Prefix safety corollary: every selected branch path and every event prefix
of a `DropReuse`-rewritten block executes without token underflow and leaves
zero leftover tokens after transferring `Return`. -/
theorem dropToReuse_prefix_safe (x r : Local width) (kind : TargetKind)
    (next : Block width) (state : State width) (choices : List Bool)
    (hxr : x ≠ r) (hzero : state.tokens r = 0) (hfresh : state.defined r = false)
    (unused : localUnused r next = true)
    (accepted : checkTokens (.seq (.drop x) next) state = true)
    (before after : List (Event width))
    (split : path (.seq (.let_ r (.dropReuse x)) (pairReuse r kind next)) choices = before ++ after) :
    ∃ mid, runEvents before state = some mid := by
  have ok := dropToReuse_preserves_checkTokens x r kind next state hxr hzero hfresh unused accepted
  exact checkTokens_prefix_safe _ state ok choices before after split

-- Executable regression: `DropReuse` + `pairReuse` across a partial-match branch.
example :
    let body : Block 4 := .seq (.drop 0)
      (.seq (.dup 1) (.branch 1 (.seq (.let_ 2 (.construct [1] [])) (.ret 2)) (.ret 1)))
    let rewritten : Block 4 := .seq (.let_ 3 (.dropReuse 0))
      (pairReuse 3 .tagged (.seq (.dup 1) (.branch 1 (.seq (.let_ 2 (.construct [1] [])) (.ret 2)) (.ret 1))))
    checkFunction [0, 1] body = true ∧ checkFunction [0, 1] rewritten = true := by decide

-- Executable regression: `DropReuse` + `pairReuse` for a record rebuild.
example :
    let body : Block 4 := .seq (.drop 0) (.seq (.let_ 2 (.record [1])) (.ret 2))
    let rewritten : Block 4 := .seq (.let_ 3 (.dropReuse 0))
      (pairReuse 3 .record (.seq (.let_ 2 (.record [1])) (.ret 2)))
    checkFunction [0, 1] body = true ∧ checkFunction [0, 1] rewritten = true := by decide

end Flick.Memory.Reuse
