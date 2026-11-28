import Formal.MemorySoundness.HeapInv.DropAux

/-!
# HeapBalanced preservation for recursive drop (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Aggregate

theorem heapBalanced_dropIdFuel (fuel : Nat) :
    (∀ {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat} (id : ObjId),
      HeapBalanced nextId h rank bound roots →
      0 < roots id →
      rank id < fuel →
      HeapBalanced nextId (dropIdFuel fuel h id) rank bound
        (fun k => if k = id then roots id - 1 else roots k) ∧
      (∀ k o1 o2, h k = some o1 → dropIdFuel fuel h id k = some o2 → o1.slot = o2.slot)) ∧
    (∀ {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat} (xs : List ObjId),
      HeapBalanced nextId h rank bound roots →
      (∀ k, xs.count k ≤ roots k) →
      (∀ x ∈ xs, rank x < fuel) →
      HeapBalanced nextId (dropIdsFuel fuel h xs) rank bound (fun k => roots k - xs.count k) ∧
      (∀ k o1 o2, h k = some o1 → dropIdsFuel fuel h xs k = some o2 → o1.slot = o2.slot)) := by
  induction fuel with
  | zero =>
      constructor
      · intro _ _ _ _ _ id _ _ hlt; omega
      · intro nextId bound h rank roots xs hb _ hlt
        cases xs with
        | nil =>
            refine ⟨?_, fun k o1 o2 h1 h2 => by rw [dropIdsFuel, h1] at h2; cases h2; rfl⟩
            simpa [dropIdsFuel] using hb
        | cons x _ => have := hlt x (List.mem_cons_self ..); omega
  | succ f ih =>
      have hstep : ∀ {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat} (id : ObjId),
          HeapBalanced nextId h rank bound roots →
          0 < roots id →
          rank id < f + 1 →
          HeapBalanced nextId (dropIdFuel (f + 1) h id) rank bound
            (fun k => if k = id then roots id - 1 else roots k) ∧
          (∀ k o1 o2, h k = some o1 → dropIdFuel (f + 1) h id k = some o2 → o1.slot = o2.slot) := by
        intro nextId bound h rank roots id hb hpos hlt
        obtain ⟨⟨rc, slot⟩, hobj, hrcPos⟩ := hb.live_of_pos_roots hpos
        dsimp only at hrcPos
        have hidLt : id < nextId := hb.live_lt hobj
        by_cases hle : rc ≤ 1
        · have hrc1 : rc = 1 := by omega
          subst hrc1
          have hconId := hb.conserve id
          simp only [rcOf, hobj] at hconId
          have hrId1 : roots id = 1 := by omega
          let h1 := setAggObj h id none
          let r1 : ObjId → Nat := fun k =>
            (if k = id then roots id - 1 else roots k) + (slotBoxedLeaves slot).count k
          have hb1 : HeapBalanced nextId h1 rank bound r1 := by
            refine ⟨?_, ?_, ?_, ?_, ?_⟩
            · intro k hk
              by_cases hkid : k = id
              · subst k; simp [h1, setAggObj]
              · simp [h1, setAggObj, hkid, hb.bounded k hk]
            · intro k obj hk
              by_cases hkid : k = id
              · subst k; simp [h1, setAggObj] at hk
              · simp only [h1, setAggObj, hkid, ite_false] at hk
                exact hb.posRc k obj hk
            · intro p obj hp c hc
              by_cases hpid : p = id
              · subst p; simp [h1, setAggObj] at hp
              · simp only [h1, setAggObj, hpid, ite_false] at hp
                exact hb.acyclic p obj hp c hc
            · intro k obj hk
              by_cases hkid : k = id
              · subst k; simp [h1, setAggObj] at hk
              · simp only [h1, setAggObj, hkid, ite_false] at hk
                exact hb.rankBound k obj hk
            · intro k
              have hset := heapInternalRefs_set_lt nextId h id k none hidLt
              simp only [slotRefCount, hobj, optSlotCount, Nat.add_zero] at hset
              have hcon := hb.conserve k
              by_cases hkid : k = id
              · subst k
                have hnotself : id ∉ slotBoxedLeaves slot := by
                  intro hmem
                  have := hb.acyclic id ⟨1, slot⟩ hobj id hmem
                  omega
                have hcnt0 : (slotBoxedLeaves slot).count id = 0 := List.count_eq_zero.mpr hnotself
                simp only [h1, rcOf, setAggObj, ite_true, r1, hrId1, hcnt0, Nat.sub_self,
                  Nat.add_zero]
                omega
              · simp only [h1, rcOf, setAggObj, hkid, ite_false, r1]
                change rcOf h k = roots k + (slotBoxedLeaves slot).count k +
                  heapInternalRefs nextId (setAggObj h id none) k
                omega
          have hcntLe : ∀ k, (slotBoxedLeaves slot).count k ≤ r1 k := by
            intro k; simp only [r1]; omega
          have hchRank : ∀ c ∈ slotBoxedLeaves slot, rank c < f := by
            intro c hc
            have := hb.acyclic id ⟨1, slot⟩ hobj c hc
            omega
          obtain ⟨hb2, hsame2⟩ := ih.2 (slotBoxedLeaves slot) hb1 hcntLe hchRank
          have hrEq : (fun k => r1 k - (slotBoxedLeaves slot).count k) =
              (fun k => if k = id then roots id - 1 else roots k) := by
            funext k; simp only [r1]; omega
          rw [hrEq] at hb2
          refine ⟨by simpa [dropIdFuel, hobj, h1] using hb2, ?_⟩
          intro k o1 o2 h1k h2k
          simp only [dropIdFuel, hobj, Nat.le_refl, ite_true] at h2k
          by_cases hkid : k = id
          · subst k
            have hnone : h1 id = none := by simp [h1, setAggObj]
            have hnone2 := (dropFuel_none_mono f).2 h1 (slotBoxedLeaves slot) id hnone
            rw [hnone2] at h2k; cases h2k
          · have h1k' : h1 k = some o1 := by simp [h1, setAggObj, hkid, h1k]
            exact hsame2 k o1 o2 h1k' h2k
        · let h1 := setAggObj h id (some ⟨rc - 1, slot⟩)
          have hb1 : HeapBalanced nextId h1 rank bound
              (fun k => if k = id then roots id - 1 else roots k) := by
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
                subst obj
                exact hb.acyclic id ⟨rc, slot⟩ hobj c hc
              · simp only [h1, setAggObj, hpid, ite_false] at hp
                exact hb.acyclic p obj hp c hc
            · intro k obj hk
              by_cases hkid : k = id
              · subst k
                exact hb.rankBound id ⟨rc, slot⟩ hobj
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
                simp only [h1, rcOf, setAggObj, ite_true]
                simp only [rcOf, hobj] at hcon
                omega
              · simp only [h1, rcOf, setAggObj, hkid, ite_false]
                exact hcon
          refine ⟨by simpa [dropIdFuel, hobj, hle, h1] using hb1, ?_⟩
          intro k o1 o2 h1k h2k
          simp only [dropIdFuel, hobj, hle, ite_false] at h2k
          by_cases hkid : k = id
          · subst k
            rw [hobj] at h1k; cases h1k
            simp only [setAggObj, ite_true, Option.some.injEq] at h2k
            cases h2k; rfl
          · simp only [setAggObj, hkid, ite_false] at h2k
            rw [h1k] at h2k; cases h2k; rfl
      refine ⟨hstep, ?_⟩
      intro nextId bound h rank roots xs
      induction xs generalizing h roots with
      | nil =>
          intro hb _ _
          refine ⟨by simpa [dropIdsFuel] using hb,
            fun k o1 o2 h1 h2 => by rw [dropIdsFuel, h1] at h2; cases h2; rfl⟩
      | cons x rest ihList =>
          intro hb hcnt hlt
          have hxPos : 0 < roots x := by
            have := hcnt x
            simp [List.count_cons_self] at this
            omega
          obtain ⟨hb1, hsame1⟩ := hstep x hb hxPos (hlt x (List.mem_cons_self ..))
          let r1 : ObjId → Nat := fun k => if k = x then roots x - 1 else roots k
          have hcnt1 : ∀ k, rest.count k ≤ r1 k := by
            intro k
            have hk := hcnt k
            by_cases hkx : k = x
            · subst k
              simp only [List.count_cons_self] at hk
              simp only [r1, ite_true]; omega
            · have hbeq : (x == k) = false := beq_false_of_ne (Ne.symm hkx)
              simp only [List.count_cons, hbeq] at hk
              simp only [r1, hkx, ite_false]; omega
          have hlt1 : ∀ y ∈ rest, rank y < f + 1 :=
            fun y hy => hlt y (List.mem_cons_of_mem x hy)
          obtain ⟨hb2, hsame2⟩ := ihList hb1 hcnt1 hlt1
          have hrEq : (fun k => r1 k - rest.count k) = (fun k => roots k - (x :: rest).count k) := by
            funext k
            by_cases hkx : k = x
            · subst k; simp only [r1, ite_true, List.count_cons_self]; omega
            · have hbeq : (x == k) = false := beq_false_of_ne (Ne.symm hkx)
              simp [r1, hkx, List.count_cons, hbeq]
          rw [hrEq] at hb2
          refine ⟨by simpa only [dropIdsFuel] using hb2, ?_⟩
          intro k o1 o2 h1k h2k
          simp only [dropIdsFuel] at h2k
          cases hmid : dropIdFuel (f + 1) h x k with
          | none =>
              have hnone := (dropFuel_none_mono (f + 1)).2 (dropIdFuel (f + 1) h x) rest k hmid
              rw [hnone] at h2k; cases h2k
          | some oMid =>
              exact (hsame1 k o1 oMid h1k hmid).trans (hsame2 k oMid o2 hmid h2k)

theorem heapBalanced_dropValFuel {nextId bound fuel : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat} (v : Aggregate.Value)
    (hb : HeapBalanced nextId h rank bound roots)
    (hcnt : ∀ k, boxedTokens v k ≤ roots k)
    (hfuel : bound ≤ fuel) :
    HeapBalanced nextId (dropValFuel fuel h v) rank bound (fun k => roots k - boxedTokens v k) ∧
    (∀ k o1 o2, h k = some o1 → dropValFuel fuel h v k = some o2 → o1.slot = o2.slot) := by
  apply (heapBalanced_dropIdFuel fuel).2 (boxedLeaves v) hb hcnt
  intro x hx
  have hxPos : 0 < roots x := by
    have h1 := List.count_pos_iff.mpr hx
    have h2 := hcnt x
    unfold boxedTokens at h2
    omega
  obtain ⟨obj, hobj, _⟩ := hb.live_of_pos_roots hxPos
  exact Nat.lt_of_lt_of_le (hb.rankBound x obj hobj) hfuel

/-- Total boxed tokens across a list of values. -/
def valsTokens (vs : List Aggregate.Value) (id : ObjId) : Nat :=
  (boxedLeavesList vs).count id

@[simp] theorem valsTokens_nil (id : ObjId) : valsTokens [] id = 0 := rfl

@[simp] theorem valsTokens_cons (v : Aggregate.Value) (vs : List Aggregate.Value) (id : ObjId) :
    valsTokens (v :: vs) id = boxedTokens v id + valsTokens vs id := by
  simp [valsTokens, boxedLeavesList, boxedTokens, List.count_append]

@[simp] theorem valsTokens_append (xs ys : List Aggregate.Value) (id : ObjId) :
    valsTokens (xs ++ ys) id = valsTokens xs id + valsTokens ys id := by
  induction xs with
  | nil => simp
  | cons v rest ih => simp [ih, Nat.add_assoc]

theorem heapBalanced_dropValsFuel {nextId bound fuel : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat} (vs : List Aggregate.Value)
    (hb : HeapBalanced nextId h rank bound roots)
    (hcnt : ∀ k, valsTokens vs k ≤ roots k)
    (hfuel : bound ≤ fuel) :
    HeapBalanced nextId (dropValsFuel fuel h vs) rank bound (fun k => roots k - valsTokens vs k) ∧
    (∀ k o1 o2, h k = some o1 → dropValsFuel fuel h vs k = some o2 → o1.slot = o2.slot) := by
  induction vs generalizing h roots with
  | nil =>
      refine ⟨by simpa [dropValsFuel] using hb,
        fun k o1 o2 h1 h2 => by rw [dropValsFuel, h1] at h2; cases h2; rfl⟩
  | cons v rest ih =>
      have hvCnt : ∀ k, boxedTokens v k ≤ roots k := by
        intro k; have := hcnt k; simp only [valsTokens_cons] at this; omega
      obtain ⟨hb1, hsame1⟩ := heapBalanced_dropValFuel v hb hvCnt hfuel
      have hrestCnt : ∀ k, valsTokens rest k ≤ roots k - boxedTokens v k := by
        intro k; have := hcnt k; simp only [valsTokens_cons] at this; omega
      obtain ⟨hb2, hsame2⟩ := ih hb1 hrestCnt
      have hrEq : ∀ k, roots k - boxedTokens v k - valsTokens rest k =
          roots k - valsTokens (v :: rest) k := by
        intro k; simp only [valsTokens_cons]; omega
      refine ⟨hb2.congrRoots hrEq, ?_⟩
      intro k o1 o2 h1k h2k
      cases hmid : dropValFuel fuel h v k with
      | none =>
          have hnone : ∀ (ws : List Aggregate.Value) (h0 : AggHeap),
              h0 k = none → dropValsFuel fuel h0 ws k = none := by
            intro ws
            induction ws with
            | nil => intro h0 hk; exact hk
            | cons w ws ihw =>
                intro h0 hk
                exact ihw (dropValFuel fuel h0 w) ((dropFuel_none_mono fuel).2 h0 (boxedLeaves w) k hk)
          rw [dropValsFuel, hnone rest (dropValFuel fuel h v) hmid] at h2k
          cases h2k
      | some oMid =>
          exact (hsame1 k o1 oMid h1k hmid).trans (hsame2 k oMid o2 hmid h2k)

end Flick.Memory.Soundness
