import Formal.MemorySoundness.HeapInv.Drop

/-!
# HeapBalanced preservation for allocation and unboxConsume (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Aggregate

/-! ## Fresh cell allocation (`Construct .cell` and `BoxAggregate`) -/

theorem heapBalanced_allocFresh {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat}
    (slot : AggSlot)
    (hb : HeapBalanced nextId h rank bound roots)
    (hcnt : ∀ k, (slotBoxedLeaves slot).count k ≤ roots k) :
    let h' := setAggObj h nextId (some ⟨1, slot⟩)
    let rank' := fun k => if k = nextId then bound else rank k
    let roots' := fun k => roots k - (slotBoxedLeaves slot).count k + (if k = nextId then 1 else 0)
    HeapBalanced (nextId + 1) h' rank' (bound + 1) roots' ∧
    (∀ k o1 o2, h k = some o1 → h' k = some o2 → o1.slot = o2.slot) := by
  let h' := setAggObj h nextId (some ⟨1, slot⟩)
  let rank' : ObjId → Nat := fun k => if k = nextId then bound else rank k
  let roots' : ObjId → Nat := fun k =>
    roots k - (slotBoxedLeaves slot).count k + (if k = nextId then 1 else 0)
  have hnotMem : nextId ∉ slotBoxedLeaves slot := by
    intro hmem
    have hpos : 0 < roots nextId := by
      have h1 := List.count_pos_iff.mpr hmem
      have h2 := hcnt nextId
      omega
    obtain ⟨obj, hobj, _⟩ := hb.live_of_pos_roots hpos
    have hlt := hb.live_lt hobj
    exact Nat.lt_irrefl nextId hlt
  have hcntNext : (slotBoxedLeaves slot).count nextId = 0 := List.count_eq_zero.mpr hnotMem
  have hrootsNext : roots nextId = 0 := by
    have hcon := hb.conserve nextId
    have hnone := hb.bounded nextId (Nat.le_refl _)
    simp only [rcOf, hnone] at hcon
    omega
  have hintNext : heapInternalRefs nextId h nextId = 0 := by
    have hcon := hb.conserve nextId
    have hnone := hb.bounded nextId (Nat.le_refl _)
    simp only [rcOf, hnone] at hcon
    omega
  refine ⟨⟨?_, ?_, ?_, ?_, ?_⟩, ?_⟩
  · intro id hge
    have hne : id ≠ nextId := Nat.ne_of_gt (Nat.lt_of_lt_of_le (Nat.lt_succ_self nextId) hge)
    simp [setAggObj, hne, hb.bounded id (Nat.le_of_succ_le hge)]
  · intro id obj hobj
    by_cases hid : id = nextId
    · subst id
      simp only [setAggObj, ite_true, Option.some.injEq] at hobj
      subst obj; simp
    · simp only [setAggObj, hid, ite_false] at hobj
      exact hb.posRc id obj hobj
  · intro p obj hobj c hc
    by_cases hp : p = nextId
    · subst p
      simp only [setAggObj, ite_true, Option.some.injEq] at hobj
      subst obj
      have hcNe : c ≠ nextId := fun eq => hnotMem (eq ▸ hc)
      have hcPos : 0 < roots c := by
        have h1 : 0 < (slotBoxedLeaves slot).count c := List.count_pos_iff.mpr hc
        have h2 := hcnt c
        omega
      obtain ⟨cobj, hcobj, _⟩ := hb.live_of_pos_roots hcPos
      simp only [ite_true, hcNe, ite_false]
      exact hb.rankBound c cobj hcobj
    · simp only [setAggObj, hp, ite_false] at hobj
      have hcNe : c ≠ nextId := by
        intro eq; subst c
        have := hb.child_internal_pos hobj hc
        omega
      simp only [hp, hcNe, ite_false]
      exact hb.acyclic p obj hobj c hc
  · intro id obj hobj
    by_cases hid : id = nextId
    · subst id; simp
    · simp only [setAggObj, hid, ite_false] at hobj
      simp only [hid, ite_false]
      exact Nat.lt_succ_of_lt (hb.rankBound id obj hobj)
  · intro id
    rw [heapInternalRefs_alloc_fresh]
    by_cases hid : id = nextId
    · subst id
      simp only [rcOf, setAggObj, ite_true, hrootsNext, hcntNext, hintNext]
    · have hcon := hb.conserve id
      have hle := hcnt id
      simp only [rcOf, setAggObj, hid, ite_false]
      change rcOf h id = roots id - (slotBoxedLeaves slot).count id + 0 +
        (heapInternalRefs nextId h id + (slotBoxedLeaves slot).count id)
      omega
  · intro k o1 o2 h1k h2k
    have hne : k ≠ nextId := by
      intro eq; subst k
      rw [hb.bounded nextId (Nat.le_refl _)] at h1k; cases h1k
    simp only [setAggObj, hne, ite_false] at h2k
    rw [h1k] at h2k; cases h2k; rfl

/-! ## `unboxConsume` (unique and shared) -/

theorem heapBalanced_unboxConsume {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat}
    (id : ObjId) (rc : Nat) (inner : Aggregate.Value) (layout : CellLayout)
    (hb : HeapBalanced nextId h rank bound roots)
    (hobj : h id = some ⟨rc, .boxedAgg inner layout⟩)
    (hroot : 0 < roots id) :
    ∃ h', unboxConsumeHeap h id = some (h', inner) ∧
      HeapBalanced nextId h' rank bound
        (fun k => (if k = id then roots id - 1 else roots k) + boxedTokens inner k) ∧
      (∀ k o1 o2, h k = some o1 → h' k = some o2 → o1.slot = o2.slot) := by
  have hrcPos : 0 < rc := hb.posRc id ⟨rc, .boxedAgg inner layout⟩ hobj
  have hidLt : id < nextId := hb.live_lt hobj
  by_cases hrc1 : rc = 1
  · subst hrc1
    have hconId := hb.conserve id
    simp only [rcOf, hobj] at hconId
    have hrId1 : roots id = 1 := by omega
    have hnotSelf : id ∉ boxedLeaves inner := by
      intro hmem
      have := hb.acyclic id ⟨1, .boxedAgg inner layout⟩ hobj id hmem
      omega
    have htokSelf : boxedTokens inner id = 0 := by
      unfold boxedTokens; exact List.count_eq_zero.mpr hnotSelf
    refine ⟨setAggObj h id none, by simp [unboxConsumeHeap, hobj], ⟨?_, ?_, ?_, ?_, ?_⟩, ?_⟩
    · intro k hk
      by_cases hkid : k = id
      · subst k; simp [setAggObj]
      · simp [setAggObj, hkid, hb.bounded k hk]
    · intro k obj hk
      by_cases hkid : k = id
      · subst k; simp [setAggObj] at hk
      · simp only [setAggObj, hkid, ite_false] at hk
        exact hb.posRc k obj hk
    · intro p obj hp c hc
      by_cases hpid : p = id
      · subst p; simp [setAggObj] at hp
      · simp only [setAggObj, hpid, ite_false] at hp
        exact hb.acyclic p obj hp c hc
    · intro k obj hk
      by_cases hkid : k = id
      · subst k; simp [setAggObj] at hk
      · simp only [setAggObj, hkid, ite_false] at hk
        exact hb.rankBound k obj hk
    · intro k
      have hset := heapInternalRefs_set_lt nextId h id k none hidLt
      simp only [slotRefCount, hobj, slotBoxedLeaves, optSlotCount, Nat.add_zero] at hset
      have hcon := hb.conserve k
      by_cases hkid : k = id
      · subst k
        simp only [rcOf, setAggObj, ite_true, hrId1, htokSelf, Nat.sub_self, Nat.add_zero]
        omega
      · simp only [rcOf, setAggObj, hkid, ite_false, boxedTokens]
        change rcOf h k = roots k + (boxedLeaves inner).count k +
          heapInternalRefs nextId (setAggObj h id none) k
        omega
    · intro k o1 o2 h1k h2k
      by_cases hkid : k = id
      · subst k; simp [setAggObj] at h2k
      · simp only [setAggObj, hkid, ite_false] at h2k
        rw [h1k] at h2k; cases h2k; rfl
  · have hshared : 2 ≤ rc := by omega
    have hlive : ∀ x ∈ boxedLeaves inner, ∃ obj, h x = some obj := by
      intro x hx
      have hpos := hb.child_internal_pos hobj hx
      have hcon := hb.conserve x
      have hrc : 0 < rcOf h x := by omega
      obtain ⟨obj, hox, _⟩ := hb.live_of_pos_rc hrc
      exact ⟨obj, hox⟩
    have hnotSelf : id ∉ boxedLeaves inner := by
      intro hmem
      have := hb.acyclic id ⟨rc, .boxedAgg inner layout⟩ hobj id hmem
      omega
    obtain ⟨hbDup, hsameDup⟩ := heapBalanced_dupValue hb inner hlive
    have hdupId : dupValue h inner id = some ⟨rc, .boxedAgg inner layout⟩ := by
      rw [dupValue, incRefs_preserves_not_mem (boxedLeaves inner) h id hnotSelf, hobj]
    let h' := setAggObj (dupValue h inner) id (some ⟨rc - 1, .boxedAgg inner layout⟩)
    refine ⟨h', by simp [unboxConsumeHeap, hobj, hrc1, hshared, h'], ⟨?_, ?_, ?_, ?_, ?_⟩, ?_⟩
    · intro k hk
      have hne : k ≠ id := Nat.ne_of_gt (Nat.lt_of_lt_of_le hidLt hk)
      simp [h', setAggObj, hne, hbDup.bounded k hk]
    · intro k obj hk
      by_cases hkid : k = id
      · subst k
        simp only [h', setAggObj, ite_true, Option.some.injEq] at hk
        subst obj; dsimp only; omega
      · simp only [h', setAggObj, hkid, ite_false] at hk
        exact hbDup.posRc k obj hk
    · intro p obj hp c hc
      by_cases hpid : p = id
      · subst p
        simp only [h', setAggObj, ite_true, Option.some.injEq] at hp
        subst obj
        exact hbDup.acyclic id ⟨rc, .boxedAgg inner layout⟩ hdupId c hc
      · simp only [h', setAggObj, hpid, ite_false] at hp
        exact hbDup.acyclic p obj hp c hc
    · intro k obj hk
      by_cases hkid : k = id
      · subst k
        exact hbDup.rankBound id ⟨rc, .boxedAgg inner layout⟩ hdupId
      · simp only [h', setAggObj, hkid, ite_false] at hk
        exact hbDup.rankBound k obj hk
    · intro k
      have hint : heapInternalRefs nextId h' k = heapInternalRefs nextId (dupValue h inner) k := by
        apply heapInternalRefs_same_slot
        intro p _
        by_cases hp : p = id
        · subst p; simp [slotRefCount, h', hdupId, setAggObj]
        · simp [slotRefCount, h', setAggObj, hp]
      have hcon := hbDup.conserve k
      rw [hint]
      by_cases hkid : k = id
      · subst k
        have htokSelf : boxedTokens inner id = 0 := by
          unfold boxedTokens; exact List.count_eq_zero.mpr hnotSelf
        simp only [h', rcOf, setAggObj, ite_true, htokSelf, Nat.add_zero]
        simp only [rcOf, hdupId] at hcon
        omega
      · simp only [h', rcOf, setAggObj, hkid, ite_false]
        exact hcon
    · intro k o1 o2 h1k h2k
      by_cases hkid : k = id
      · subst k
        rw [hobj] at h1k; cases h1k
        simp only [h', setAggObj, ite_true, Option.some.injEq] at h2k
        cases h2k; rfl
      · simp only [h', setAggObj, hkid, ite_false] at h2k
        exact hsameDup k o1 o2 h1k h2k

end Flick.Memory.Soundness
