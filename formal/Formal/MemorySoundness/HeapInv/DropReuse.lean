import Formal.MemorySoundness.HeapInv.Box

/-!
# HeapBalanced preservation for recursive DropReuse (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Aggregate

/-! ## Recursive `DropReuse` and `Reuse` preserving `HeapBalanced` -/

/-- Recursive `DropReuse`: rejects aggregates (`none`), releases old children
recursively via `dropIdsFuel` on a unique `tagged` cell leaving a `reuseToken`,
or drops and returns `.imm` on shared / non-reusable cells. -/
def dropReuseRec (fuel : Nat) (h : AggHeap) : Aggregate.Value → Option (AggHeap × Aggregate.Value)
  | .agg _ _ => none
  | .imm => some (h, .imm)
  | .boxed id =>
      match h id with
      | none => none
      | some ⟨rc, .tagged tag layout oldFields⟩ =>
          if rc = 1 then
            some (dropIdsFuel fuel (setAggObj h id (some ⟨1, .reuseToken layout⟩))
              (boxedLeavesList oldFields), .boxed id)
          else if 2 ≤ rc then
            some (setAggObj h id (some ⟨rc - 1, .tagged tag layout oldFields⟩), .imm)
          else none
      | some _ => some (dropIdFuel fuel h id, .imm)

theorem heapBalanced_dropReuseRec {nextId bound fuel : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat} (v : Aggregate.Value)
    (hb : HeapBalanced nextId h rank bound roots)
    (hcnt : ∀ k, boxedTokens v k ≤ roots k)
    (hnotAgg : ∀ tag fs, v ≠ .agg tag fs)
    (hfuel : bound ≤ fuel) :
    ∃ h' res excl,
      dropReuseRec fuel h v = some (h', res) ∧
      HeapBalanced nextId h' rank bound
        (fun k => roots k - boxedTokens v k + boxedTokens res k) ∧
      (∀ c, excl = some c → heapInternalRefs nextId h c = 0 ∧
        roots c = boxedTokens v c ∧ boxedTokens res c = 1) ∧
      (∀ k o1 o2, some k ≠ excl → h k = some o1 → h' k = some o2 → o1.slot = o2.slot) := by
  cases v with
  | agg tag fs => exact (hnotAgg tag fs rfl).elim
  | imm =>
      refine ⟨h, .imm, none, rfl, hb.congrRoots (fun k => by simp [boxedTokens, boxedLeaves]),
        (fun _ hc => by cases hc),
        fun k o1 o2 _ h1 h2 => by rw [h1] at h2; cases h2; rfl⟩
  | boxed id =>
      have hidRoot : 0 < roots id := by
        have := hcnt id; simp [boxedTokens, boxedLeaves] at this; omega
      obtain ⟨⟨rc, slot⟩, hobj, hrcPos⟩ := hb.live_of_pos_roots hidRoot
      dsimp only at hrcPos
      have hidLt : id < nextId := hb.live_lt hobj
      cases slot with
      | tagged tag layout oldFields =>
          by_cases hrc1 : rc = 1
          · subst hrc1
            have hconId := hb.conserve id
            simp only [rcOf, hobj] at hconId
            have hrId1 : roots id = 1 := by omega
            have hintId0 : heapInternalRefs nextId h id = 0 := by omega
            let h1 := setAggObj h id (some ⟨1, .reuseToken layout⟩)
            let r1 : ObjId → Nat := fun k => roots k + (boxedLeavesList oldFields).count k
            have hb1 : HeapBalanced nextId h1 rank bound r1 := by
              refine ⟨?_, ?_, ?_, ?_, ?_⟩
              · intro k hk
                have hne : k ≠ id := Nat.ne_of_gt (Nat.lt_of_lt_of_le hidLt hk)
                simp [h1, setAggObj, hne, hb.bounded k hk]
              · intro k obj hk
                by_cases hkid : k = id
                · subst k
                  simp only [h1, setAggObj, ite_true, Option.some.injEq] at hk
                  subst obj; simp
                · simp only [h1, setAggObj, hkid, ite_false] at hk
                  exact hb.posRc k obj hk
              · intro p obj hp c hc
                by_cases hpid : p = id
                · subst p
                  simp only [h1, setAggObj, ite_true, Option.some.injEq] at hp
                  subst obj; simp [slotBoxedLeaves] at hc
                · simp only [h1, setAggObj, hpid, ite_false] at hp
                  exact hb.acyclic p obj hp c hc
              · intro k obj hk
                by_cases hkid : k = id
                · subst k; exact hb.rankBound id ⟨1, .tagged tag layout oldFields⟩ hobj
                · simp only [h1, setAggObj, hkid, ite_false] at hk
                  exact hb.rankBound k obj hk
              · intro k
                have hset := heapInternalRefs_set_lt nextId h id k
                  (some ⟨1, .reuseToken layout⟩) hidLt
                simp only [slotRefCount, hobj, slotBoxedLeaves, optSlotCount,
                  List.count_nil, Nat.add_zero] at hset
                have hcon := hb.conserve k
                by_cases hkid : k = id
                · subst k
                  have hnotSelf : id ∉ boxedLeavesList oldFields := by
                    intro hmem
                    have := hb.acyclic id ⟨1, .tagged tag layout oldFields⟩ hobj id hmem
                    omega
                  have hcnt0 : (boxedLeavesList oldFields).count id = 0 :=
                    List.count_eq_zero.mpr hnotSelf
                  simp only [h1, rcOf, setAggObj, ite_true, r1, hrId1, hcnt0, Nat.add_zero]
                  omega
                · simp only [h1, rcOf, setAggObj, hkid, ite_false, r1]
                  change rcOf h k = roots k + (boxedLeavesList oldFields).count k +
                    heapInternalRefs nextId (setAggObj h id (some ⟨1, .reuseToken layout⟩)) k
                  omega
            have hchCnt : ∀ k, (boxedLeavesList oldFields).count k ≤ r1 k := by
              intro k; simp only [r1]; omega
            have hchRank : ∀ c ∈ boxedLeavesList oldFields, rank c < fuel := by
              intro c hc
              have h1 := hb.acyclic id ⟨1, .tagged tag layout oldFields⟩ hobj c hc
              have h2 := hb.rankBound id ⟨1, .tagged tag layout oldFields⟩ hobj
              omega
            obtain ⟨hb2, hsame2⟩ := (heapBalanced_dropIdFuel fuel).2 (boxedLeavesList oldFields)
              hb1 hchCnt hchRank
            have hrEq : ∀ k, r1 k - (boxedLeavesList oldFields).count k =
                roots k - boxedTokens (.boxed id) k + boxedTokens (.boxed id) k := by
              intro k
              simp only [r1]
              by_cases hkid : k = id
              · subst k; simp [boxedTokens, boxedLeaves]; omega
              · have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
                simp [boxedTokens, boxedLeaves, List.count_cons, hbeq]
            refine ⟨_, .boxed id, some id, (by simp [dropReuseRec, hobj, h1]), hb2.congrRoots hrEq, ?_, ?_⟩
            · intro c hc; cases hc
              exact ⟨hintId0, by simp [boxedTokens, boxedLeaves, hrId1],
                by simp [boxedTokens, boxedLeaves]⟩
            · intro k o1 o2 hne h1k h2k
              have hkid : k ≠ id := fun eq => hne (congrArg some eq)
              have h1k' : h1 k = some o1 := by simp [h1, setAggObj, hkid, h1k]
              exact hsame2 k o1 o2 h1k' h2k
          · have hshared : 2 ≤ rc := by omega
            let h1 := setAggObj h id (some ⟨rc - 1, .tagged tag layout oldFields⟩)
            have hb1 : HeapBalanced nextId h1 rank bound
                (fun k => roots k - boxedTokens (.boxed id) k + boxedTokens .imm k) := by
              refine ⟨?_, ?_, ?_, ?_, ?_⟩
              · intro k hk
                have hne : k ≠ id := Nat.ne_of_gt (Nat.lt_of_lt_of_le hidLt hk)
                simp [h1, setAggObj, hne, hb.bounded k hk]
              · intro k obj hk
                by_cases hkid : k = id
                · subst k
                  simp only [h1, setAggObj, ite_true, Option.some.injEq] at hk
                  subst obj; dsimp only; omega
                · simp only [h1, setAggObj, hkid, ite_false] at hk
                  exact hb.posRc k obj hk
              · intro p obj hp c hc
                by_cases hpid : p = id
                · subst p
                  simp only [h1, setAggObj, ite_true, Option.some.injEq] at hp
                  subst obj; exact hb.acyclic id ⟨rc, .tagged tag layout oldFields⟩ hobj c hc
                · simp only [h1, setAggObj, hpid, ite_false] at hp
                  exact hb.acyclic p obj hp c hc
              · intro k obj hk
                by_cases hkid : k = id
                · subst k; exact hb.rankBound id ⟨rc, .tagged tag layout oldFields⟩ hobj
                · simp only [h1, setAggObj, hkid, ite_false] at hk
                  exact hb.rankBound k obj hk
              · intro k
                have hint : heapInternalRefs nextId h1 k = heapInternalRefs nextId h k := by
                  apply heapInternalRefs_same_slot
                  intro p _
                  by_cases hp : p = id
                  · subst p; simp [slotRefCount, h1, hobj, setAggObj]
                  · simp [slotRefCount, h1, setAggObj, hp]
                have hcon := hb.conserve k
                rw [hint]
                by_cases hkid : k = id
                · subst k
                  simp only [h1, rcOf, setAggObj, ite_true, boxedTokens, boxedLeaves,
                    List.count_cons_self, List.count_nil, Nat.add_zero]
                  simp only [rcOf, hobj] at hcon
                  omega
                · have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
                  simp only [h1, rcOf, setAggObj, hkid, ite_false, boxedTokens, boxedLeaves,
                    List.count_cons, hbeq, List.count_nil, Nat.add_zero]
                  exact hcon
            refine ⟨h1, .imm, none, (by simp [dropReuseRec, hobj, hrc1, hshared, h1]), hb1,
              (fun _ hc => by cases hc), ?_⟩
            intro k o1 o2 _ h1k h2k
            by_cases hkid : k = id
            · subst k
              rw [hobj] at h1k; cases h1k
              simp only [h1, setAggObj, ite_true, Option.some.injEq] at h2k
              cases h2k; rfl
            · simp only [h1, setAggObj, hkid, ite_false] at h2k
              rw [h1k] at h2k; cases h2k; rfl
      | boxedAgg _ _ | reuseToken _ | «opaque» =>
          have hrank : rank id < fuel := Nat.lt_of_lt_of_le (hb.rankBound id _ hobj) hfuel
          obtain ⟨hb1, hsame1⟩ := (heapBalanced_dropIdFuel fuel).1 id hb hidRoot hrank
          have hrEq : ∀ k, (if k = id then roots id - 1 else roots k) =
              roots k - boxedTokens (.boxed id) k + boxedTokens .imm k := by
            intro k
            by_cases hkid : k = id
            · subst k; simp [boxedTokens, boxedLeaves]
            · have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
              simp [boxedTokens, boxedLeaves, List.count_cons, hbeq, hkid]
          refine ⟨dropIdFuel fuel h id, .imm, none, by simp [dropReuseRec, hobj],
            hb1.congrRoots hrEq, (fun _ hc => by cases hc),
            fun k o1 o2 _ h1k h2k => hsame1 k o1 o2 h1k h2k⟩

end Flick.Memory.Soundness
