import Formal.MemorySoundness.Exec.OpSoundCommon

/-!
# Operational soundness for literal and projection ops (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMemOp_sound_lit {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.lit)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.lit) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.lit) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.lit) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.lit) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents, List.map_nil, runEvents, Option.some.injEq] at hrunOp
  simp only [evalMemOp, Option.some.injEq, Prod.mk.injEq] at heval
  subst afterPass
  rcases heval with ⟨rfl, rfl, rfl, rfl⟩
  refine ⟨rfl, ?_⟩
  have hb' : HeapBalanced nextId h rank (bound + 1)
      (fun id => frameRefs (update env b (some .imm)) (update state.tokens b 1) id + otherRoots id) := by
    apply (hb.weakenBound (Nat.le_succ bound)).congrRoots
    intro id
    rw [frameRefs_bind_fresh env state.tokens b .imm hbZero id]
    simp [boxedTokens, boxedLeaves]
  exact hfinish none rfl hb' (fun _ hc => by cases hc)
    (fun _ _ _ _ h1 h2 => by rw [h1] at h2; cases h2; rfl) .imm

theorem evalMemOp_sound_project {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (recv : Local width) (rep : ValueRep) (idx : Nat)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.project recv rep idx)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.project recv rep idx) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.project recv rep idx) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.project recv rep idx) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.project recv rep idx) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents, runEvents, Pass.step] at hrunOp
  by_cases hz : (state.tokens recv == 0) = true
  · simp [hz] at hrunOp
  · have hzf : (state.tokens recv == 0) = false := Bool.eq_false_iff.mpr hz
    have hpos : 0 < state.tokens recv := by
      have : state.tokens recv ≠ 0 := by intro h0; simp [h0] at hzf
      omega
    simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some, Option.some.injEq] at hrunOp
    subst afterPass
    simp only [evalMemOp, hzf, Bool.false_eq_true, ite_false] at heval
    obtain ⟨vrecv, svrecv, hvrecv, hsvrecv, hdrecv⟩ := hden recv hpos
    simp only [hvrecv] at heval
    cases rep with
    | aggregate =>
        cases vrecv with
        | imm | boxed _ => simp at heval
        | agg tag fields =>
            cases hf : fields[idx]? with
            | none => simp [hf] at heval
            | some field =>
                simp only [hf, Option.some.injEq, Prod.mk.injEq] at heval
                rcases heval with ⟨rfl, rfl, rfl, rfl⟩
                cases hdrecv with
                | agg _ _ sfields hds =>
                    obtain ⟨sfield, hsfield, hdfield⟩ := hds.getElem? idx field hf
                    have hlive : ∀ id ∈ boxedLeaves field, ∃ obj, h id = some obj := by
                      intro id hid
                      have h1 : 0 < boxedTokens field id := List.count_pos_iff.mpr hid
                      have h2 := boxedTokens_field_le tag fields idx field hf id
                      have h3 := frameRefs_tok_le env state.tokens recv (.agg tag fields) hvrecv hpos id
                      have hposRoot : 0 < frameRefs env state.tokens id + otherRoots id := by omega
                      obtain ⟨obj, hobj, _⟩ := hb.live_of_pos_roots hposRoot
                      exact ⟨obj, hobj⟩
                    obtain ⟨hbDup, hsameDup⟩ := heapBalanced_dupValue hb field hlive
                    have hb' : HeapBalanced nextId (dupValue h field) rank (bound + 1)
                        (fun id => frameRefs (update env b (some field)) (update state.tokens b 1) id +
                          otherRoots id) := by
                      apply (hbDup.weakenBound (Nat.le_succ bound)).congrRoots
                      intro id
                      rw [frameRefs_bind_fresh env state.tokens b field hbZero id]
                      omega
                    have hdField' : Denotes (dupValue h field) field sfield := by
                      apply denotes_preserve_of_balanced hb hbDup none (fun _ hc => by cases hc)
                        (fun id o1 o2 _ h1 h2 => hsameDup id o1 o2 h1 h2) hdfield
                      intro id hid
                      refine ⟨by simp, ?_⟩
                      have hrc := hbDup.conserve id
                      have h1 : 0 < boxedTokens field id := List.count_pos_iff.mpr hid
                      omega
                    refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsvrecv, hsfield]) hb'
                      (fun _ hc => by cases hc)
                      (fun id o1 o2 _ h1 h2 => hsameDup id o1 o2 h1 h2) hdField'⟩
    | cell =>
        cases vrecv with
        | imm | agg _ _ => simp at heval
        | boxed id =>
            cases hobj : h id with
            | none => simp [hobj] at heval
            | some obj =>
                rcases obj with ⟨rc, slot⟩
                cases slot with
                | boxedAgg _ _ | reuseToken _ | «opaque» => simp [hobj] at heval
                | tagged tag layout fields =>
                    have hrcPos : 0 < rc := hb.posRc id _ hobj
                    simp only [hobj, hrcPos, ite_true] at heval
                    cases hf : fields[idx]? with
                    | none => simp [hf] at heval
                    | some field =>
                        simp only [hf, Option.some.injEq, Prod.mk.injEq] at heval
                        rcases heval with ⟨rfl, rfl, rfl, rfl⟩
                        cases hdrecv with
                        | tagged _ _ _ _ _ sfields hobj2 _ hds =>
                            rw [hobj] at hobj2; cases hobj2
                            obtain ⟨sfield, hsfield, hdfield⟩ := hds.getElem? idx field hf
                            have hlive : ∀ k ∈ boxedLeaves field, ∃ o, h k = some o := by
                              intro k hk
                              have h1 : 0 < (boxedLeaves field).count k := List.count_pos_iff.mpr hk
                              have h2 := boxedLeavesList_getElem_count_le fields idx field hf k
                              have hposCnt : 0 < (boxedLeavesList fields).count k := by omega
                              have hmem : k ∈ slotBoxedLeaves (.tagged tag layout fields) :=
                                List.count_pos_iff.mp hposCnt
                              have hint := hb.child_internal_pos hobj hmem
                              have hcon := hb.conserve k
                              have hrcK : 0 < rcOf h k := by omega
                              obtain ⟨o, ho, _⟩ := hb.live_of_pos_rc hrcK
                              exact ⟨o, ho⟩
                            obtain ⟨hbDup, hsameDup⟩ := heapBalanced_dupValue hb field hlive
                            have hb' : HeapBalanced nextId (dupValue h field) rank (bound + 1)
                                (fun k => frameRefs (update env b (some field)) (update state.tokens b 1) k +
                                  otherRoots k) := by
                              apply (hbDup.weakenBound (Nat.le_succ bound)).congrRoots
                              intro k
                              rw [frameRefs_bind_fresh env state.tokens b field hbZero k]
                              omega
                            have hdField' : Denotes (dupValue h field) field sfield := by
                              apply denotes_preserve_of_balanced hb hbDup none (fun _ hc => by cases hc)
                                (fun k o1 o2 _ h1 h2 => hsameDup k o1 o2 h1 h2) hdfield
                              intro k hk
                              refine ⟨by simp, ?_⟩
                              have hrc := hbDup.conserve k
                              have h1 : 0 < boxedTokens field k := List.count_pos_iff.mpr hk
                              omega
                            refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsvrecv, hsfield]) hb'
                              (fun _ hc => by cases hc)
                              (fun k o1 o2 _ h1 h2 => hsameDup k o1 o2 h1 h2) hdField'⟩
                        | boxedAgg _ _ _ _ _ hobj2 _ _ => rw [hobj] at hobj2; cases hobj2
                        | opaque_ _ _ hobj2 _ => rw [hobj] at hobj2; cases hobj2
                        | reuseToken _ _ _ hobj2 _ => rw [hobj] at hobj2; cases hobj2

theorem evalMemOp_sound_projectPath {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (recv : Local width) (path : List Nat)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.projectPath recv path)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.projectPath recv path) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.projectPath recv path) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.projectPath recv path) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.projectPath recv path) nextId' h' res hrunOp hb henv hden hext hextRoots
  simp only [toRepOp, eraseOp, operationEvents, runEvents, Pass.step] at hrunOp
  by_cases hz : (state.tokens recv == 0) = true
  · simp [hz] at hrunOp
  · have hzf : (state.tokens recv == 0) = false := Bool.eq_false_iff.mpr hz
    have hpos : 0 < state.tokens recv := by
      have : state.tokens recv ≠ 0 := by intro h0; simp [h0] at hzf
      omega
    simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some, Option.some.injEq] at hrunOp
    subst afterPass
    simp only [evalMemOp, hzf, Bool.false_eq_true, ite_false] at heval
    obtain ⟨vrecv, svrecv, hvrecv, hsvrecv, hdrecv⟩ := hden recv hpos
    simp only [hvrecv] at heval
    unfold Aggregate.projectPath at heval
    cases hl : Aggregate.lookupPath vrecv path with
    | none => simp [hl] at heval
    | some field =>
        simp only [hl, Option.map_some, Option.some.injEq, Prod.mk.injEq] at heval
        rcases heval with ⟨rfl, rfl, rfl, rfl⟩
        obtain ⟨sfield, hsfield, hdfield⟩ := hdrecv.lookupPath_sound path field hl
        have hlive : ∀ id ∈ boxedLeaves field, ∃ obj, h id = some obj := by
          intro id hid
          have h1 : 0 < boxedTokens field id := List.count_pos_iff.mpr hid
          have h2 := boxedTokens_lookupPath_le vrecv path field hl id
          have h3 := frameRefs_tok_le env state.tokens recv vrecv hvrecv hpos id
          have hposRoot : 0 < frameRefs env state.tokens id + otherRoots id := by omega
          obtain ⟨obj, hobj, _⟩ := hb.live_of_pos_roots hposRoot
          exact ⟨obj, hobj⟩
        obtain ⟨hbDup, hsameDup⟩ := heapBalanced_dupValue hb field hlive
        have hb' : HeapBalanced nextId (dupValue h field) rank (bound + 1)
            (fun id => frameRefs (update env b (some field)) (update state.tokens b 1) id +
              otherRoots id) := by
          apply (hbDup.weakenBound (Nat.le_succ bound)).congrRoots
          intro id
          rw [frameRefs_bind_fresh env state.tokens b field hbZero id]
          omega
        have hdField' : Denotes (dupValue h field) field sfield := by
          apply denotes_preserve_of_balanced hb hbDup none (fun _ hc => by cases hc)
            (fun id o1 o2 _ h1 h2 => hsameDup id o1 o2 h1 h2) hdfield
          intro id hid
          refine ⟨by simp, ?_⟩
          have hrc := hbDup.conserve id
          have h1 : 0 < boxedTokens field id := List.count_pos_iff.mpr hid
          omega
        refine ⟨rfl, hfinish none (by simp [evalSourceOp, hsvrecv, hsfield]) hb'
          (fun _ hc => by cases hc)
          (fun id o1 o2 _ h1 h2 => hsameDup id o1 o2 h1 h2) hdField'⟩

end Flick.Memory.Soundness
