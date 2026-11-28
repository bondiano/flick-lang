import Formal.MemorySoundness.Exec.OpSoundCommon

/-!
# Operational soundness for unboxConsume (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMemOp_sound_unboxConsume {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (obj : Local width)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.unboxConsume obj)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.unboxConsume obj) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.unboxConsume obj) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.unboxConsume obj) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.unboxConsume obj) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents] at hrunOp
  simp only [evalMemOp] at heval
  cases htake : takeOperands env [obj] state.tokens with
  | none => simp [htake] at heval
  | some p =>
      rcases p with ⟨vals, tokTaken⟩
      cases vals with
      | nil => simp [htake] at heval
      | cons v rest =>
          cases rest with
          | cons _ _ => simp [htake] at heval
          | nil =>
              cases v with
              | imm | agg _ _ => simp [htake] at heval
              | boxed id =>
                  simp only [htake] at heval
                  obtain ⟨htokEq, hframeEq, svals, hsvals, hdvals⟩ :=
                    takeOperands_spec [obj] env h senv state afterPass [.boxed id] tokTaken
                      hrunOp htake hden
                  subst tokTaken
                  cases hdvals with
                  | cons _ sv _ _ hdv hdnil =>
                      cases hdnil
                      have hsv : senv obj = some sv := by
                        simp only [List.mapM_cons, List.mapM_nil, Option.pure_def,
                          Option.bind_eq_bind] at hsvals
                        cases hs : senv obj with
                        | none => simp [hs] at hsvals
                        | some s =>
                            simp only [hs, Option.bind_some, Option.some.injEq,
                              List.cons.injEq] at hsvals
                            rw [hsvals.1]
                      have hidRoot : 0 < frameRefs env state.tokens id + otherRoots id := by
                        have hf := hframeEq id
                        simp [valsTokens_cons, valsTokens_nil, boxedTokens, boxedLeaves] at hf
                        omega
                      cases hunb : unboxConsumeHeap h id with
                      | none => simp [hunb] at heval
                      | some q =>
                          rcases q with ⟨hUnb, inner⟩
                          simp only [hunb, Option.some.injEq, Prod.mk.injEq] at heval
                          rcases heval with ⟨rfl, rfl, rfl, rfl⟩
                          cases hcell : h id with
                          | none => simp [unboxConsumeHeap, hcell] at hunb
                          | some cell =>
                              rcases cell with ⟨rc, slot⟩
                              cases slot with
                              | tagged _ _ _ | reuseToken _ | «opaque» =>
                                  simp [unboxConsumeHeap, hcell] at hunb
                              | boxedAgg inner0 layout =>
                                  obtain ⟨hUnb2, hunb2, hbUnb, hsameUnb⟩ :=
                                    heapBalanced_unboxConsume id rc inner0 layout hb hcell hidRoot
                                  rw [hunb] at hunb2
                                  simp only [Option.some.injEq, Prod.mk.injEq] at hunb2
                                  rcases hunb2 with ⟨rfl, rfl⟩
                                  cases hdv with
                                  | boxedAgg _ _ _ _ _ hcell2 _ hdinner =>
                                      rw [hcell] at hcell2; cases hcell2
                                      have hb' : HeapBalanced nextId hUnb rank (bound + 1)
                                          (fun k => frameRefs (update env b (some inner))
                                            (update afterPass.tokens b 1) k + otherRoots k) := by
                                        apply (hbUnb.weakenBound (Nat.le_succ bound)).congrRoots
                                        intro k
                                        rw [frameRefs_bind_fresh env afterPass.tokens b inner hbZero k]
                                        have hf := hframeEq k
                                        simp only [valsTokens_cons, valsTokens_nil, Nat.add_zero] at hf
                                        by_cases hkid : k = id
                                        · subst k
                                          simp only [boxedTokens, boxedLeaves, List.count_cons_self,
                                            List.count_nil] at hf
                                          simp only [ite_true]; omega
                                        · have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
                                          simp only [boxedTokens, boxedLeaves, List.count_cons, hbeq,
                                            List.count_nil, Bool.false_eq_true, ite_false, Nat.add_zero] at hf
                                          simp only [hkid, ite_false]; omega
                                      have hdInner' : Denotes hUnb inner sv := by
                                        apply denotes_preserve_of_balanced hb hbUnb none
                                          (fun _ hc => by cases hc)
                                          (fun k o1 o2 _ h1 h2 => hsameUnb k o1 o2 h1 h2) hdinner
                                        intro k hk
                                        refine ⟨by simp, ?_⟩
                                        have hrc := hbUnb.conserve k
                                        have h1 : 0 < boxedTokens inner k := List.count_pos_iff.mpr hk
                                        omega
                                      refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsv]) hb'
                                        (fun _ hc => by cases hc)
                                        (fun k o1 o2 _ h1 h2 => hsameUnb k o1 o2 h1 h2) hdInner'⟩
                                  | tagged _ _ _ _ _ _ hcell2 _ _ => rw [hcell] at hcell2; cases hcell2
                                  | opaque_ _ _ hcell2 _ => rw [hcell] at hcell2; cases hcell2
                                  | reuseToken _ _ _ hcell2 _ => rw [hcell] at hcell2; cases hcell2

end Flick.Memory.Soundness
