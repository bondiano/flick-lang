import Formal.MemorySoundness.Exec.OpSoundCommon

/-!
# Operational soundness for dropReuse and reuse (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMemOp_sound_dropReuse {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (obj : Local width)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.dropReuse obj)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.dropReuse obj) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    (hfuel : bound ≤ dropFuel) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.dropReuse obj) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.dropReuse obj) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.dropReuse obj) nextId' h' res hrunOp hb henv hden hext hextRoots
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
              simp only [htake] at heval
              obtain ⟨htokEq, hframeEq, svals, hsvals, hdvals⟩ :=
                takeOperands_spec [obj] env h senv state afterPass [v] tokTaken hrunOp htake hden
              subst tokTaken
              cases hdr : dropReuseRec dropFuel h v with
              | none => simp [hdr] at heval
              | some q =>
                  rcases q with ⟨hDr, resDr⟩
                  simp only [hdr, Option.some.injEq, Prod.mk.injEq] at heval
                  rcases heval with ⟨rfl, rfl, rfl, rfl⟩
                  have hnotAgg : ∀ tag fs, v ≠ .agg tag fs := by
                    intro tag fs eq; subst eq; simp [dropReuseRec] at hdr
                  have hcnt : ∀ k, boxedTokens v k ≤ frameRefs env state.tokens k + otherRoots k := by
                    intro k; have hf := hframeEq k
                    simp only [valsTokens_cons, valsTokens_nil, Nat.add_zero] at hf; omega
                  obtain ⟨hDr2, resDr2, excl, hdr2, hbDr, hexcl, hsameDr⟩ :=
                    heapBalanced_dropReuseRec v hb hcnt hnotAgg hfuel
                  rw [hdr] at hdr2
                  simp only [Option.some.injEq, Prod.mk.injEq] at hdr2
                  rcases hdr2 with ⟨rfl, rfl⟩
                  have hb' : HeapBalanced nextId hDr rank (bound + 1)
                      (fun k => frameRefs (update env b (some resDr))
                        (update afterPass.tokens b 1) k + otherRoots k) := by
                    apply (hbDr.weakenBound (Nat.le_succ bound)).congrRoots
                    intro k
                    rw [frameRefs_bind_fresh env afterPass.tokens b resDr hbZero k]
                    have hf := hframeEq k
                    simp only [valsTokens_cons, valsTokens_nil, Nat.add_zero] at hf
                    omega
                  have hdDr : Denotes hDr resDr .imm := by
                    cases v with
                    | agg tag fs => exact (hnotAgg tag fs rfl).elim
                    | imm =>
                        simp only [dropReuseRec, Option.some.injEq, Prod.mk.injEq] at hdr
                        rcases hdr with ⟨_, rfl⟩
                        exact .imm
                    | boxed id =>
                        cases hobj : h id with
                        | none => simp [dropReuseRec, hobj] at hdr
                        | some obj =>
                            rcases obj with ⟨rc, slot⟩
                            cases slot with
                            | tagged tag layout oldFields =>
                                by_cases hrc1 : rc = 1
                                · subst hrc1
                                  simp only [dropReuseRec, hobj, ite_true, Option.some.injEq,
                                    Prod.mk.injEq] at hdr
                                  rcases hdr with ⟨hhDr, rfl⟩
                                  have hacyc0 : ∀ p o,
                                      setAggObj h id (some ⟨1, .reuseToken layout⟩) p = some o →
                                      ∀ c ∈ slotBoxedLeaves o.slot, rank c < rank p := by
                                    intro p o hp c hc
                                    by_cases hpid : p = id
                                    · subst p
                                      simp only [setAggObj, ite_true, Option.some.injEq] at hp
                                      subst o; simp [slotBoxedLeaves] at hc
                                    · simp only [setAggObj, hpid, ite_false] at hp
                                      exact hb.acyclic p o hp c hc
                                  have hchLt : ∀ c ∈ boxedLeavesList oldFields, rank c < rank id :=
                                    fun c hc => hb.acyclic id ⟨1, .tagged tag layout oldFields⟩ hobj c hc
                                  have htok : hDr id = some ⟨1, .reuseToken layout⟩ := by
                                    rw [← hhDr, (dropFuel_preserves_gt_rank dropFuel rank).2 _ _ id
                                      hacyc0 hchLt]
                                    simp [setAggObj]
                                  exact .reuseToken id 1 layout htok (by omega)
                                · by_cases hsh : 2 ≤ rc
                                  · simp only [dropReuseRec, hobj, hrc1, hsh, ite_false,
                                      ite_true, Option.some.injEq, Prod.mk.injEq] at hdr
                                    rcases hdr with ⟨_, rfl⟩; exact .imm
                                  · simp [dropReuseRec, hobj, hrc1, hsh] at hdr
                            | boxedAgg _ _ | reuseToken _ | «opaque» =>
                                simp only [dropReuseRec, hobj, Option.some.injEq,
                                  Prod.mk.injEq] at hdr
                                rcases hdr with ⟨_, rfl⟩; exact .imm
                  have hexcl' : ∀ c, excl = some c → heapInternalRefs nextId h c = 0 ∧
                      frameRefs env afterPass.tokens c = 0 ∧ otherRoots c = 0 := by
                    intro c hc
                    have ⟨hint0, hrootsEq, _⟩ := hexcl c hc
                    have hf := hframeEq c
                    simp only [valsTokens_cons, valsTokens_nil, Nat.add_zero] at hf
                    refine ⟨hint0, by omega, by omega⟩
                  refine ⟨rfl, hfinish excl rfl hb' hexcl' hsameDr hdDr⟩

theorem evalMemOp_sound_reuse {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (recv : Local width) (rep : ValueRep) (tag : Nat) (layout : CellLayout) (fields evidence : List (Local width))
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp (MemOp.reuse recv rep tag layout fields evidence)))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens (MemOp.reuse recv rep tag layout fields evidence) = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    (hfuel : bound ≤ dropFuel) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp (MemOp.reuse recv rep tag layout fields evidence) senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hbZero := evalMemOp_hbZero b (MemOp.reuse recv rep tag layout fields evidence) hrunOp hfreshB henv
  have hfinish := @evalMemOp_finish width nextId bound h rank otherRoots env state afterPass senv ext sext
    b (MemOp.reuse recv rep tag layout fields evidence) nextId' h' res hrunOp hb henv hden hext hextRoots
  cases rep with
  | aggregate => simp [evalMemOp] at heval
  | cell =>
      simp only [toRepOp, eraseOp, operationEvents] at hrunOp
      simp only [evalMemOp] at heval
      cases htake : takeOperands env (recv :: (fields ++ evidence)) state.tokens with
      | none => simp [htake] at heval
      | some p =>
          rcases p with ⟨vals, tokTaken⟩
          cases vals with
          | nil => simp [htake] at heval
          | cons cand fvals =>
              simp only [htake] at heval
              obtain ⟨htokEq, hframeEq, svals, hsvals, hdvals⟩ :=
                takeOperands_spec (recv :: (fields ++ evidence)) env h senv state afterPass
                  (cand :: fvals) tokTaken hrunOp htake hden
              subst tokTaken
              cases hdvals with
              | cons _ scand _ sfvals hdcand hdfvals =>
                  have hsfvals : (fields ++ evidence).mapM senv = some sfvals := by
                    simp only [List.mapM_cons, Option.bind_eq_bind] at hsvals
                    cases hs : senv recv with
                    | none => simp [hs] at hsvals
                    | some s =>
                        simp only [hs, Option.bind_some] at hsvals
                        cases hrest : (fields ++ evidence).mapM senv with
                        | none => simp [hrest] at hsvals
                        | some rs =>
                            simp only [hrest, Option.bind_some, Option.pure_def,
                              Option.some.injEq, List.cons.injEq] at hsvals
                            rw [hsvals.2]
                  cases hre : reuseTypedCellRec dropFuel nextId h cand tag layout fvals with
                  | none => simp [hre] at heval
                  | some q =>
                      rcases q with ⟨nextIdR, hR, resR, reused⟩
                      simp only [hre, Option.some.injEq, Prod.mk.injEq] at heval
                      rcases heval with ⟨rfl, rfl, rfl, rfl⟩
                      have hnotAgg : ∀ t fs, cand ≠ .agg t fs := by
                        intro t fs eq; subst eq; simp [reuseTypedCellRec] at hre
                      have hcnt : ∀ k, boxedTokens cand k + valsTokens fvals k ≤
                          frameRefs env state.tokens k + otherRoots k := by
                        intro k; have hf := hframeEq k
                        simp only [valsTokens_cons] at hf; omega
                      obtain ⟨n2, h2, rank2, res2, reused2, excl, hre2, hbR, hexcl, hsameR,
                        id', hresId, hslotNew⟩ :=
                        heapBalanced_reuseTypedCellRec cand tag layout fvals hb hcnt hnotAgg hfuel
                      rw [hre] at hre2
                      simp only [Option.some.injEq, Prod.mk.injEq] at hre2
                      rcases hre2 with ⟨rfl, rfl, rfl, rfl⟩
                      have hb' : HeapBalanced nextIdR hR rank2 (bound + 1)
                          (fun k => frameRefs (update env b (some resR))
                            (update afterPass.tokens b 1) k + otherRoots k) := by
                        apply hbR.congrRoots
                        intro k
                        rw [frameRefs_bind_fresh env afterPass.tokens b resR hbZero k]
                        have hf := hframeEq k
                        simp only [valsTokens_cons] at hf
                        omega
                      have hexcl' : ∀ c, excl = some c → heapInternalRefs nextId h c = 0 ∧
                          frameRefs env afterPass.tokens c = 0 ∧ otherRoots c = 0 := by
                        intro c hc
                        have ⟨hint0, hrootsEq, _, _⟩ := hexcl c hc
                        have hf := hframeEq c
                        simp only [valsTokens_cons] at hf
                        refine ⟨hint0, by omega, by omega⟩
                      have hdFvals' : DenotesList hR fvals sfvals := by
                        apply denotesList_preserve_ext hb hbR excl (fun c hc => (hexcl c hc).1)
                          hsameR hdfvals
                        intro k hk
                        refine ⟨?_, ?_⟩
                        · intro heq
                          have hfs0 := (hexcl k heq.symm).2.2.2
                          have hpos : 0 < valsTokens fvals k := List.count_pos_iff.mpr hk
                          omega
                        · have hrc := hbR.conserve k
                          have hint := hbR.child_internal_pos (p := id') (c := k)
                            (obj := ⟨1, .tagged tag layout fvals⟩) hslotNew hk
                          omega
                      have hdRes : Denotes hR resR (.node tag sfvals) := by
                        subst hresId
                        exact .tagged id' 1 tag layout fvals sfvals hslotNew (by omega) hdFvals'
                      refine ⟨rfl, hfinish excl (by simp [evalSourceOp, hsfvals]) hb'
                        hexcl' hsameR hdRes⟩


end Flick.Memory.Soundness
