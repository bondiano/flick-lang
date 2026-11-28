import Formal.MemorySoundness.Exec.PrefixSteps

/-!
# Whole-block operational soundness and source equivalence (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-- Main operational soundness theorem over any `checkRepTokens`-accepted
`MemBlock`, any branch selection `choices`, and any step-prefix budget `steps`:
- Normal return (`Outcome.returned`): produces a value denoting the exact pure
  `evalSource` result, leaves zero frame tokens, conserves all global reference
  counts (`HeapBalanced`), and preserves every external alias's `Denotes`;
- Abnormal prefix exit (`Outcome.aborted exit`): preserves exact global RC
  conservation (`HeapBalanced`) over the live frame `(env', tok')` and all
  external/arena roots, and preserves every external alias's `Denotes`. -/
theorem evalMemPrefix_sound (dropFuel : Nat) (exit : MemoryScopes.ExitKind)
    (steps : Nat) (block : MemBlock width) :
    ∀ (choices : List Bool) (nextId bound : Nat) (h : AggHeap)
      (rank otherRoots : ObjId → Nat)
      (env : Local width → Option Aggregate.Value)
      (state : RepState width) (senv : Local width → Option SemVal)
      (ext : List Aggregate.Value) (sext : List SemVal) (out : Outcome width),
      checkRepTokens (toRepBlock block) state = true →
      evalMemPrefix dropFuel exit steps block choices nextId h env state.tokens = some out →
      HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id) →
      EnvTokensOk env state.toPassState →
      EnvDenotes h env state.tokens senv →
      DenotesList h ext sext →
      (∀ id, valsTokens ext id ≤ otherRoots id) →
      bound + blockSteps block ≤ dropFuel →
      match out with
      | .returned nextId' h' res =>
          ∃ rank' sv,
            evalSource block choices senv = some sv ∧
            Denotes h' res sv ∧
            HeapBalanced nextId' h' rank' (bound + blockSteps block)
              (fun id => boxedTokens res id + otherRoots id) ∧
            DenotesList h' ext sext
      | .aborted exit' nextId' h' env' tok' =>
          exit' = exit ∧
          ∃ rank',
            HeapBalanced nextId' h' rank' (bound + blockSteps block)
              (fun id => frameRefs env' tok' id + otherRoots id) ∧
            DenotesList h' ext sext := by
  induction steps generalizing block with
  | zero =>
      intro choices nextId bound h rank otherRoots env state senv ext sext out
        _ heval hb _ _ hext _ _
      simp only [evalMemPrefix, Option.some.injEq] at heval
      subst out
      exact ⟨rfl, rank, hb.weakenBound (Nat.le_add_right bound _), hext⟩
  | succ s ih =>
      intro choices nextId bound h rank otherRoots env state senv ext sext out
        hchk heval hb henv hden hext hextRoots hfuel
      simp only [checkRepTokens, Bool.and_eq_true] at hchk
      have htokChk : checkTokens (eraseBlock (toRepBlock block)) state.toPassState = true := hchk.1
      have hrepChk : checkRepOk (toRepBlock block) state.isAgg = true := hchk.2
      cases block with
      | ret r =>
          simp only [toRepBlock, eraseBlock, checkTokens, RepState.toPassState, runEvents,
            Pass.step] at htokChk
          by_cases hz : (state.tokens r == 0) = true
          · simp [hz] at htokChk
          · have hzf : (state.tokens r == 0) = false := Bool.eq_false_iff.mpr hz
            have hrPos : 0 < state.tokens r := by
              have : state.tokens r ≠ 0 := by intro h0; simp [h0] at hzf
              omega
            simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some] at htokChk
            have hempty : ∀ x, update state.tokens r (state.tokens r - 1) x = 0 := by
              intro x
              have hall := (List.all_eq_true.mp htokChk) x (List.mem_finRange x)
              simpa [emptyTokens] using hall
            have hr1 : state.tokens r = 1 := by
              have h := hempty r
              simp only [update, beq_self_eq_true, ite_true] at h
              omega
            have hother0 : ∀ x, x ≠ r → state.tokens x = 0 := by
              intro x hxr
              have h := hempty x
              have hbeq : (x == r) = false := beq_false_of_ne hxr
              simpa [update, hbeq] using h
            obtain ⟨vr, svr, hvr, hsvr, hdrv⟩ := hden r hrPos
            simp only [evalMemPrefix, hzf, Bool.false_eq_true, ite_false, hvr,
              Option.some.injEq] at heval
            subst out
            refine ⟨rank, svr, by simp [evalSource, hsvr], hdrv, ?_, hext⟩
            apply (hb.weakenBound (Nat.le_add_right bound _)).congrRoots
            intro id
            rw [frameRefs_singleton env state.tokens r vr hvr hr1 hother0 id]
      | seq inst next =>
          cases inst with
          | dup x =>
              simp only [toRepBlock, toRepInst, eraseBlock, eraseInst, checkTokens,
                RepState.toPassState, instructionEvents, runEvents, Pass.step] at htokChk
              by_cases hz : (state.tokens x == 0) = true
              · simp [hz] at htokChk
              · have hzf : (state.tokens x == 0) = false := Bool.eq_false_iff.mpr hz
                have hxPos : 0 < state.tokens x := by
                  have : state.tokens x ≠ 0 := by intro h0; simp [h0] at hzf
                  omega
                simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some] at htokChk
                obtain ⟨vx, svx, hvx, hsvx, hdx⟩ := hden x hxPos
                simp only [evalMemPrefix, hzf, Bool.false_eq_true, ite_false, hvx] at heval
                let st' : RepState width :=
                  ⟨update state.tokens x (state.tokens x + 1), state.defined, state.isAgg⟩
                obtain ⟨hbNext, henvNext, hdenNext, hextNext⟩ :=
                  evalMem_step_dup_local x vx hxPos hvx hb henv hden hext hextRoots
                have hchkNext : checkRepTokens (toRepBlock next) st' = true := by
                  simp only [checkRepTokens, Bool.and_eq_true]
                  exact ⟨htokChk, hrepChk⟩
                have hfuelNext : bound + 1 + blockSteps next ≤ dropFuel := by
                  simp only [blockSteps] at hfuel; omega
                have hres := ih next choices nextId (bound + 1) (dupValue h vx) rank otherRoots
                  env st' senv ext sext out hchkNext heval hbNext henvNext hdenNext hextNext
                  hextRoots hfuelNext
                have hboundEq : bound + 1 + blockSteps next = bound + blockSteps (.seq (.dup x) next) := by
                  simp only [blockSteps]; omega
                cases out with
                | returned nextId' h' res =>
                    rcases hres with ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                    rw [hboundEq] at hbOut
                    exact ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                | aborted exit' nextId' h' env' tok' =>
                    rcases hres with ⟨heq, rank', hbOut, hextOut⟩
                    rw [hboundEq] at hbOut
                    exact ⟨heq, rank', hbOut, hextOut⟩
          | drop x =>
              simp only [toRepBlock, toRepInst, eraseBlock, eraseInst, checkTokens,
                RepState.toPassState, instructionEvents, runEvents, Pass.step] at htokChk
              by_cases hz : (state.tokens x == 0) = true
              · simp [hz] at htokChk
              · have hzf : (state.tokens x == 0) = false := Bool.eq_false_iff.mpr hz
                have hxPos : 0 < state.tokens x := by
                  have : state.tokens x ≠ 0 := by intro h0; simp [h0] at hzf
                  omega
                simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some] at htokChk
                obtain ⟨vx, svx, hvx, hsvx, hdx⟩ := hden x hxPos
                simp only [evalMemPrefix, hzf, Bool.false_eq_true, ite_false, hvx] at heval
                let st' : RepState width :=
                  ⟨update state.tokens x (state.tokens x - 1), state.defined, state.isAgg⟩
                have hfuelB : bound ≤ dropFuel := by simp only [blockSteps] at hfuel; omega
                obtain ⟨hbNext, henvNext, hdenNext, hextNext⟩ :=
                  evalMem_step_drop_local x vx hxPos hvx hb henv hden hext hextRoots hfuelB
                have hchkNext : checkRepTokens (toRepBlock next) st' = true := by
                  simp only [checkRepTokens, Bool.and_eq_true]
                  exact ⟨htokChk, hrepChk⟩
                have hfuelNext : bound + 1 + blockSteps next ≤ dropFuel := by
                  simp only [blockSteps] at hfuel; omega
                have hres := ih next choices nextId (bound + 1) (dropValFuel dropFuel h vx) rank
                  otherRoots env st' senv ext sext out hchkNext heval hbNext henvNext hdenNext
                  hextNext hextRoots hfuelNext
                have hboundEq : bound + 1 + blockSteps next = bound + blockSteps (.seq (.drop x) next) := by
                  simp only [blockSteps]; omega
                cases out with
                | returned nextId' h' res =>
                    rcases hres with ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                    rw [hboundEq] at hbOut
                    exact ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                | aborted exit' nextId' h' env' tok' =>
                    rcases hres with ⟨heq, rank', hbOut, hextOut⟩
                    rw [hboundEq] at hbOut
                    exact ⟨heq, rank', hbOut, hextOut⟩
          | let_ b op =>
              simp only [toRepBlock, toRepInst, eraseBlock, eraseInst, checkTokens,
                instructionEvents] at htokChk
              rw [runEvents_append] at htokChk
              cases hrunOp : runEvents (operationEvents (eraseOp (toRepOp op))) state.toPassState with
              | none => simp [hrunOp] at htokChk
              | some afterOp =>
                  simp only [hrunOp, Option.bind_some, runEvents, Pass.step] at htokChk
                  by_cases hdefB : afterOp.defined b = true
                  · simp [hdefB] at htokChk
                  · have hfreshB : afterOp.defined b = false := Bool.eq_false_iff.mpr hdefB
                    simp only [hfreshB, Bool.false_eq_true, ite_false, Option.bind_some] at htokChk
                    simp only [toRepBlock, toRepInst, checkRepOk] at hrepChk
                    cases hstepRep : stepRepOp (toRepOp op) b state.isAgg with
                    | none => simp [hstepRep] at hrepChk
                    | some isAgg' =>
                        simp only [hstepRep] at hrepChk
                        simp only [evalMemPrefix] at heval
                        cases hop : evalMemOp dropFuel nextId h env state.tokens op with
                        | none => simp [hop] at heval
                        | some q =>
                            rcases q with ⟨nextId', h', resV, tok'⟩
                            simp only [hop] at heval
                            have hfuelB : bound ≤ dropFuel := by simp only [blockSteps] at hfuel; omega
                            obtain ⟨htokEq, rank', sv, hsrcOp, hbNext, henvNext, hdenNext, hextNext⟩ :=
                              evalMemOp_sound b op nextId' h' resV tok' hrunOp hfreshB hop hb henv
                                hden hext hextRoots hfuelB
                            subst tok'
                            let st' : RepState width :=
                              ⟨update afterOp.tokens b 1, update afterOp.defined b true, isAgg'⟩
                            have hchkNext : checkRepTokens (toRepBlock next) st' = true := by
                              simp only [checkRepTokens, Bool.and_eq_true]
                              exact ⟨htokChk, hrepChk⟩
                            have hfuelNext : bound + 1 + blockSteps next ≤ dropFuel := by
                              simp only [blockSteps] at hfuel; omega
                            have hres := ih next choices nextId' (bound + 1) h' rank' otherRoots
                              (update env b (some resV)) st' (update senv b (some sv)) ext sext out
                              hchkNext heval hbNext henvNext hdenNext hextNext hextRoots hfuelNext
                            have hboundEq : bound + 1 + blockSteps next =
                                bound + blockSteps (.seq (.let_ b op) next) := by
                              simp only [blockSteps]; omega
                            cases out with
                            | returned nextId'' h'' res =>
                                rcases hres with ⟨rank'', svFinal, hsrcNext, hdRes, hbOut, hextOut⟩
                                rw [hboundEq] at hbOut
                                exact ⟨rank'', svFinal, by simp [evalSource, hsrcOp, hsrcNext],
                                  hdRes, hbOut, hextOut⟩
                            | aborted exit' nextId'' h'' env'' tok'' =>
                                rcases hres with ⟨heq, rank'', hbOut, hextOut⟩
                                rw [hboundEq] at hbOut
                                exact ⟨heq, rank'', hbOut, hextOut⟩
      | branch c yes no =>
          simp only [toRepBlock, eraseBlock, checkTokens, RepState.toPassState, runEvents,
            Pass.step] at htokChk
          by_cases hz : (state.tokens c == 0) = true
          · simp [hz] at htokChk
          · have hzf : (state.tokens c == 0) = false := Bool.eq_false_iff.mpr hz
            have hcPos : 0 < state.tokens c := by
              have : state.tokens c ≠ 0 := by intro h0; simp [h0] at hzf
              omega
            simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some,
              Bool.and_eq_true] at htokChk
            simp only [toRepBlock, checkRepOk, Bool.and_eq_true] at hrepChk
            obtain ⟨vc, svc, hvc, hsvc, hdc⟩ := hden c hcPos
            simp only [evalMemPrefix, hzf, Bool.false_eq_true, ite_false, hvc] at heval
            let st' : RepState width :=
              ⟨update state.tokens c (state.tokens c - 1), state.defined, state.isAgg⟩
            have hfuelB : bound ≤ dropFuel := by simp only [blockSteps] at hfuel; omega
            obtain ⟨hbNext, henvNext, hdenNext, hextNext⟩ :=
              evalMem_step_drop_local c vc hcPos hvc hb henv hden hext hextRoots hfuelB
            cases choices with
            | nil =>
                have hchkNo : checkRepTokens (toRepBlock no) st' = true := by
                  simp only [checkRepTokens, Bool.and_eq_true]; exact ⟨htokChk.2, hrepChk.2⟩
                have hfuelNo : bound + 1 + blockSteps no ≤ dropFuel := by
                  simp only [blockSteps] at hfuel; omega
                have hres := ih no [] nextId (bound + 1) (dropValFuel dropFuel h vc) rank otherRoots
                  env st' senv ext sext out hchkNo heval hbNext henvNext hdenNext hextNext
                  hextRoots hfuelNo
                have hleBound : bound + 1 + blockSteps no ≤ bound + blockSteps (.branch c yes no) := by
                  simp only [blockSteps]; omega
                cases out with
                | returned nextId' h' res =>
                    rcases hres with ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                    exact ⟨rank', sv, by simp [evalSource, hsvc, hsrc], hdRes,
                      hbOut.weakenBound hleBound, hextOut⟩
                | aborted exit' nextId' h' env' tok' =>
                    rcases hres with ⟨heq, rank', hbOut, hextOut⟩
                    exact ⟨heq, rank', hbOut.weakenBound hleBound, hextOut⟩
            | cons b rest =>
                cases b with
                | true =>
                    have hchkYes : checkRepTokens (toRepBlock yes) st' = true := by
                      simp only [checkRepTokens, Bool.and_eq_true]; exact ⟨htokChk.1, hrepChk.1⟩
                    have hfuelYes : bound + 1 + blockSteps yes ≤ dropFuel := by
                      simp only [blockSteps] at hfuel; omega
                    have hres := ih yes rest nextId (bound + 1) (dropValFuel dropFuel h vc) rank
                      otherRoots env st' senv ext sext out hchkYes heval hbNext henvNext hdenNext
                      hextNext hextRoots hfuelYes
                    have hleBound : bound + 1 + blockSteps yes ≤ bound + blockSteps (.branch c yes no) := by
                      simp only [blockSteps]; omega
                    cases out with
                    | returned nextId' h' res =>
                        rcases hres with ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                        exact ⟨rank', sv, by simp [evalSource, hsvc, hsrc], hdRes,
                          hbOut.weakenBound hleBound, hextOut⟩
                    | aborted exit' nextId' h' env' tok' =>
                        rcases hres with ⟨heq, rank', hbOut, hextOut⟩
                        exact ⟨heq, rank', hbOut.weakenBound hleBound, hextOut⟩
                | false =>
                    have hchkNo : checkRepTokens (toRepBlock no) st' = true := by
                      simp only [checkRepTokens, Bool.and_eq_true]; exact ⟨htokChk.2, hrepChk.2⟩
                    have hfuelNo : bound + 1 + blockSteps no ≤ dropFuel := by
                      simp only [blockSteps] at hfuel; omega
                    have hres := ih no rest nextId (bound + 1) (dropValFuel dropFuel h vc) rank
                      otherRoots env st' senv ext sext out hchkNo heval hbNext henvNext hdenNext
                      hextNext hextRoots hfuelNo
                    have hleBound : bound + 1 + blockSteps no ≤ bound + blockSteps (.branch c yes no) := by
                      simp only [blockSteps]; omega
                    cases out with
                    | returned nextId' h' res =>
                        rcases hres with ⟨rank', sv, hsrc, hdRes, hbOut, hextOut⟩
                        exact ⟨rank', sv, by simp [evalSource, hsvc, hsrc], hdRes,
                          hbOut.weakenBound hleBound, hextOut⟩
                    | aborted exit' nextId' h' env' tok' =>
                        rcases hres with ⟨heq, rank', hbOut, hextOut⟩
                        exact ⟨heq, rank', hbOut.weakenBound hleBound, hextOut⟩

end Flick.Memory.Soundness
