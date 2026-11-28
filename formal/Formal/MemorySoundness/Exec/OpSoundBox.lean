import Formal.MemorySoundness.Exec.OpSoundCommon

/-!
# Operational soundness for construct, boxAggregate, and unboxBorrow (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMemOp_sound_construct {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (rep : ValueRep) (tag : Nat) (layout : CellLayout) (fields evidence : List (Local width))
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.construct rep tag layout fields evidence)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.construct rep tag layout fields evidence) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.construct rep tag layout fields evidence) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.construct rep tag layout fields evidence) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.construct rep tag layout fields evidence) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents] at hrunOp
  simp only [evalMemOp] at heval
  cases htake : takeOperands env (fields ++ evidence) state.tokens with
  | none => simp [htake] at heval
  | some p =>
      rcases p with ⟨vals, tokTaken⟩
      simp only [htake] at heval
      obtain ⟨htokEq, hframeEq, svals, hsvals, hdvals⟩ :=
        takeOperands_spec (fields ++ evidence) env h senv state afterPass vals tokTaken
          hrunOp htake hden
      subst tokTaken
      cases rep with
      | aggregate =>
          simp only [Option.some.injEq, Prod.mk.injEq] at heval
          rcases heval with ⟨rfl, rfl, rfl, rfl⟩
          have hb' : HeapBalanced nextId h rank (bound + 1)
              (fun id => frameRefs (update env b (some (.agg tag vals)))
                (update afterPass.tokens b 1) id + otherRoots id) := by
            apply (hb.weakenBound (Nat.le_succ bound)).congrRoots
            intro id
            rw [frameRefs_bind_fresh env afterPass.tokens b (.agg tag vals) hbZero id]
            have hf := hframeEq id
            change frameRefs env state.tokens id + otherRoots id =
              frameRefs env afterPass.tokens id + valsTokens vals id + otherRoots id
            omega
          refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsvals]) hb'
            (fun _ hc => by cases hc)
            (fun _ _ _ _ h1 h2 => by rw [h1] at h2; cases h2; rfl) (.agg tag vals svals hdvals)⟩
      | cell =>
          simp only [Option.some.injEq, Prod.mk.injEq] at heval
          rcases heval with ⟨rfl, rfl, rfl, rfl⟩
          have hcnt : ∀ k, (slotBoxedLeaves (.tagged tag layout vals)).count k ≤
              frameRefs env state.tokens k + otherRoots k := by
            intro k; have hf := hframeEq k; simp [slotBoxedLeaves]; change valsTokens vals k ≤ _; omega
          obtain ⟨hbAlloc, hsameAlloc⟩ := heapBalanced_allocFresh (.tagged tag layout vals) hb hcnt
          have hb' : HeapBalanced (nextId + 1)
              (setAggObj h nextId (some ⟨1, .tagged tag layout vals⟩))
              (fun k => if k = nextId then bound else rank k) (bound + 1)
              (fun id => frameRefs (update env b (some (.boxed nextId)))
                (update afterPass.tokens b 1) id + otherRoots id) := by
            apply hbAlloc.congrRoots
            intro id
            rw [frameRefs_bind_fresh env afterPass.tokens b (.boxed nextId) hbZero id]
            have hf := hframeEq id
            simp only [slotBoxedLeaves]
            change frameRefs env state.tokens id + otherRoots id - valsTokens vals id +
              (if id = nextId then 1 else 0) =
              frameRefs env afterPass.tokens id + boxedTokens (.boxed nextId) id + otherRoots id
            by_cases hid : id = nextId
            · subst id; simp [boxedTokens, boxedLeaves]; omega
            · have hbeq : (nextId == id) = false := beq_false_of_ne (Ne.symm hid)
              simp [boxedTokens, boxedLeaves, List.count_cons, hbeq, hid]; omega
          have hdVals' : DenotesList (setAggObj h nextId (some ⟨1, .tagged tag layout vals⟩))
              vals svals := by
            apply denotesList_preserve_ext hb hbAlloc none (fun _ hc => by cases hc)
              (fun id o1 o2 _ h1 h2 => hsameAlloc id o1 o2 h1 h2) hdvals
            intro id hid
            refine ⟨by simp, ?_⟩
            have hrc := hbAlloc.conserve id
            have hint := hbAlloc.child_internal_pos (p := nextId) (c := id)
              (obj := ⟨1, .tagged tag layout vals⟩) (by simp [setAggObj]) hid
            omega
          have hdCell : Denotes (setAggObj h nextId (some ⟨1, .tagged tag layout vals⟩))
              (.boxed nextId) (.node tag svals) :=
            .tagged nextId 1 tag layout vals svals (by simp [setAggObj]) (by omega) hdVals'
          refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsvals]) hb'
            (fun _ hc => by cases hc)
            (fun id o1 o2 _ h1 h2 => hsameAlloc id o1 o2 h1 h2) hdCell⟩

theorem evalMemOp_sound_boxAggregate {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (val : Local width) (layout : CellLayout)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.boxAggregate val layout)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.boxAggregate val layout) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.boxAggregate val layout) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.boxAggregate val layout) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.boxAggregate val layout) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents] at hrunOp
  simp only [evalMemOp] at heval
  cases htake : takeOperands env [val] state.tokens with
  | none => simp [htake] at heval
  | some p =>
      rcases p with ⟨vals, tokTaken⟩
      cases vals with
      | nil => simp [htake] at heval
      | cons v rest =>
          cases rest with
          | cons _ _ => simp [htake] at heval
          | nil =>
              simp only [htake, boxAggregateHeap, Option.some.injEq, Prod.mk.injEq] at heval
              rcases heval with ⟨rfl, rfl, rfl, rfl⟩
              obtain ⟨htokEq, hframeEq, svals, hsvals, hdvals⟩ :=
                takeOperands_spec [val] env h senv state afterPass [v] tokTaken hrunOp htake hden
              subst tokTaken
              cases hdvals with
              | cons _ sv _ _ hdv hdnil =>
                  cases hdnil
                  have hsv : senv val = some sv := by
                    simp only [List.mapM_cons, List.mapM_nil, Option.pure_def,
                      Option.bind_eq_bind] at hsvals
                    cases hs : senv val with
                    | none => simp [hs] at hsvals
                    | some s =>
                        simp only [hs, Option.bind_some, Option.some.injEq, List.cons.injEq] at hsvals
                        rw [hsvals.1]
                  have hcnt : ∀ k, (slotBoxedLeaves (.boxedAgg v layout)).count k ≤
                      frameRefs env state.tokens k + otherRoots k := by
                    intro k; have hf := hframeEq k
                    simp only [valsTokens_cons, valsTokens_nil, Nat.add_zero] at hf
                    simp only [slotBoxedLeaves]; change boxedTokens v k ≤ _; omega
                  obtain ⟨hbAlloc, hsameAlloc⟩ := heapBalanced_allocFresh (.boxedAgg v layout) hb hcnt
                  have hb' : HeapBalanced (nextId + 1)
                      (setAggObj h nextId (some ⟨1, .boxedAgg v layout⟩))
                      (fun k => if k = nextId then bound else rank k) (bound + 1)
                      (fun id => frameRefs (update env b (some (.boxed nextId)))
                        (update afterPass.tokens b 1) id + otherRoots id) := by
                    apply hbAlloc.congrRoots
                    intro id
                    rw [frameRefs_bind_fresh env afterPass.tokens b (.boxed nextId) hbZero id]
                    have hf := hframeEq id
                    simp only [valsTokens_cons, valsTokens_nil, Nat.add_zero] at hf
                    simp only [slotBoxedLeaves]
                    change frameRefs env state.tokens id + otherRoots id - boxedTokens v id +
                      (if id = nextId then 1 else 0) =
                      frameRefs env afterPass.tokens id + boxedTokens (.boxed nextId) id + otherRoots id
                    by_cases hid : id = nextId
                    · subst id
                      have hbt : boxedTokens (.boxed nextId) nextId = 1 := by
                        simp [boxedTokens, boxedLeaves]
                      rw [hbt]; simp only [ite_true]; omega
                    · have hbt : boxedTokens (.boxed nextId) id = 0 := by
                        have hbeq : (nextId == id) = false := beq_false_of_ne (Ne.symm hid)
                        simp [boxedTokens, boxedLeaves, List.count_cons, hbeq]
                      rw [hbt]; simp only [hid, ite_false]; omega
                  have hdv' : Denotes (setAggObj h nextId (some ⟨1, .boxedAgg v layout⟩)) v sv := by
                    apply denotes_preserve_of_balanced hb hbAlloc none (fun _ hc => by cases hc)
                      (fun id o1 o2 _ h1 h2 => hsameAlloc id o1 o2 h1 h2) hdv
                    intro id hid
                    refine ⟨by simp, ?_⟩
                    have hrc := hbAlloc.conserve id
                    have hint := hbAlloc.child_internal_pos (p := nextId) (c := id)
                      (obj := ⟨1, .boxedAgg v layout⟩) (by simp [setAggObj]) hid
                    omega
                  have hdBox : Denotes (setAggObj h nextId (some ⟨1, .boxedAgg v layout⟩))
                      (.boxed nextId) sv :=
                    .boxedAgg nextId 1 v layout sv (by simp [setAggObj]) (by omega) hdv'
                  refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsv]) hb'
                    (fun _ hc => by cases hc)
                    (fun id o1 o2 _ h1 h2 => hsameAlloc id o1 o2 h1 h2) hdBox⟩

theorem evalMemOp_sound_unboxBorrow {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (obj : Local width)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.unboxBorrow obj)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.unboxBorrow obj) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.unboxBorrow obj) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.unboxBorrow obj) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.unboxBorrow obj) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents, runEvents, Pass.step] at hrunOp
  by_cases hz : (state.tokens obj == 0) = true
  · simp [hz] at hrunOp
  · have hzf : (state.tokens obj == 0) = false := Bool.eq_false_iff.mpr hz
    have hpos : 0 < state.tokens obj := by
      have : state.tokens obj ≠ 0 := by intro h0; simp [h0] at hzf
      omega
    simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some, Option.some.injEq] at hrunOp
    subst afterPass
    simp only [evalMemOp, hzf, Bool.false_eq_true, ite_false] at heval
    obtain ⟨vobj, svobj, hvobj, hsvobj, hdobj⟩ := hden obj hpos
    simp only [hvobj] at heval
    cases vobj with
    | imm | agg _ _ => simp at heval
    | boxed id =>
        unfold unboxBorrowHeap at heval
        cases hcell : h id with
        | none => simp [hcell] at heval
        | some cell =>
            rcases cell with ⟨rc, slot⟩
            cases slot with
            | tagged _ _ _ | reuseToken _ | «opaque» => simp [hcell] at heval
            | boxedAgg inner layout =>
                have hrcPos : 0 < rc := hb.posRc id _ hcell
                simp only [hcell, hrcPos, ite_true, Option.some.injEq, Prod.mk.injEq] at heval
                rcases heval with ⟨rfl, rfl, rfl, rfl⟩
                cases hdobj with
                | boxedAgg _ _ _ _ _ hcell2 _ hdinner =>
                    rw [hcell] at hcell2; cases hcell2
                    have hlive : ∀ k ∈ boxedLeaves inner, ∃ o, h k = some o := by
                      intro k hk
                      have hint := hb.child_internal_pos hcell hk
                      have hcon := hb.conserve k
                      have hrcK : 0 < rcOf h k := by omega
                      obtain ⟨o, ho, _⟩ := hb.live_of_pos_rc hrcK
                      exact ⟨o, ho⟩
                    obtain ⟨hbDup, hsameDup⟩ := heapBalanced_dupValue hb inner hlive
                    have hb' : HeapBalanced nextId (dupValue h inner) rank (bound + 1)
                        (fun k => frameRefs (update env b (some inner)) (update state.tokens b 1) k +
                          otherRoots k) := by
                      apply (hbDup.weakenBound (Nat.le_succ bound)).congrRoots
                      intro k
                      rw [frameRefs_bind_fresh env state.tokens b inner hbZero k]
                      omega
                    have hdInner' : Denotes (dupValue h inner) inner svobj := by
                      apply denotes_preserve_of_balanced hb hbDup none (fun _ hc => by cases hc)
                        (fun k o1 o2 _ h1 h2 => hsameDup k o1 o2 h1 h2) hdinner
                      intro k hk
                      refine ⟨by simp, ?_⟩
                      have hrc := hbDup.conserve k
                      have h1 : 0 < boxedTokens inner k := List.count_pos_iff.mpr hk
                      omega
                    refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsvobj]) hb'
                      (fun _ hc => by cases hc)
                      (fun k o1 o2 _ h1 h2 => hsameDup k o1 o2 h1 h2) hdInner'⟩
                | tagged _ _ _ _ _ _ hcell2 _ _ => rw [hcell] at hcell2; cases hcell2
                | opaque_ _ _ hcell2 _ => rw [hcell] at hcell2; cases hcell2
                | reuseToken _ _ _ hcell2 _ => rw [hcell] at hcell2; cases hcell2

end Flick.Memory.Soundness
