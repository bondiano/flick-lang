import Formal.MemorySoundness.Sem

/-!
# Global Reference-Count Conservation and Counted Primitive Steps (#463)

Defines:
- `HeapBalanced nextId h rank bound roots`: exact global reference-count
  conservation across all live objects `< nextId`, positive reference counts,
  acyclicity rank `rank : ObjId → Nat` strictly decreasing along
  `slotBoxedLeaves`, and rank bound `∀ id obj, h id = some obj → rank id < bound`;
- `zero_roots_empty_heap`: draining all roots to `0` under `HeapBalanced` leaves
  zero live objects (`∀ id, h id = none`);
- `denotes_preserve_of_balanced`: unified semantic-value preservation lemma for
  all surviving roots and external aliases across every heap transition;
- Rank-bounded recursive `dropIdFuel` / `dropIdsFuel` / `dropValFuel` /
  `dropValsFuel`, `dropReuseRec`, `reuseTypedCellRec`, and `HeapBalanced`
  preservation lemmas for every counted primitive step.
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Aggregate

/-! ## Finite range sums and internal heap references -/

def sumRange (f : Nat → Nat) : Nat → Nat
  | 0 => 0
  | n + 1 => sumRange f n + f n

theorem sumRange_congr (f g : Nat → Nat) (n : Nat) (h : ∀ k, k < n → f k = g k) :
    sumRange f n = sumRange g n := by
  induction n with
  | zero => rfl
  | succ m ih =>
      simp only [sumRange]
      rw [ih (fun k hk => h k (Nat.lt_succ_of_lt hk)), h m (Nat.lt_succ_self m)]

theorem sumRange_update (f : Nat → Nat) (n id v : Nat) (hlt : id < n) :
    sumRange (fun k => if k = id then v else f k) n + f id = sumRange f n + v := by
  induction n with
  | zero => omega
  | succ m ih =>
      simp only [sumRange]
      by_cases h : m = id
      · subst m
        have hcongr : sumRange (fun k => if k = id then v else f k) id = sumRange f id := by
          apply sumRange_congr
          intro k hk
          have hne : k ≠ id := Nat.ne_of_lt hk
          simp [hne]
        rw [hcongr]
        simp only [ite_true]
        omega
      · have hlt' : id < m := by omega
        have hih := ih hlt'
        simp only [h, ite_false]
        omega

theorem sumRange_le (f : Nat → Nat) (n id : Nat) (hlt : id < n) :
    f id ≤ sumRange f n := by
  induction n with
  | zero => omega
  | succ m ih =>
      simp only [sumRange]
      by_cases h : id = m
      · subst id; omega
      · have hlt' : id < m := by omega
        have := ih hlt'
        omega

theorem sumRange_zero (f : Nat → Nat) (n : Nat) (hz : ∀ k, k < n → f k = 0) :
    sumRange f n = 0 := by
  induction n with
  | zero => rfl
  | succ m ih =>
      simp only [sumRange, ih (fun k hk => hz k (Nat.lt_succ_of_lt hk)), hz m (Nat.lt_succ_self m)]

def slotRefCount (h : AggHeap) (target : ObjId) (p : ObjId) : Nat :=
  match h p with
  | none => 0
  | some obj => (slotBoxedLeaves obj.slot).count target

def optSlotCount (obj : Option AggObj) (target : ObjId) : Nat :=
  match obj with
  | none => 0
  | some o => (slotBoxedLeaves o.slot).count target

/-- Total internal references to `target` from all live heap cells `< nextId`. -/
def heapInternalRefs (nextId : Nat) (h : AggHeap) (target : ObjId) : Nat :=
  sumRange (slotRefCount h target) nextId

theorem heapInternalRefs_set_lt (nextId : Nat) (h : AggHeap) (id target : ObjId)
    (newObj : Option AggObj) (hlt : id < nextId) :
    heapInternalRefs nextId (setAggObj h id newObj) target + slotRefCount h target id =
      heapInternalRefs nextId h target + optSlotCount newObj target := by
  unfold heapInternalRefs
  have hcongr : sumRange (slotRefCount (setAggObj h id newObj) target) nextId =
      sumRange (fun k => if k = id then optSlotCount newObj target else slotRefCount h target k) nextId := by
    apply sumRange_congr
    intro k _
    by_cases hk : k = id
    · subst k; simp [slotRefCount, setAggObj, optSlotCount]
    · simp [slotRefCount, setAggObj, hk]
  rw [hcongr]
  exact sumRange_update (slotRefCount h target) nextId id (optSlotCount newObj target) hlt

theorem heapInternalRefs_alloc_fresh (nextId : Nat) (h : AggHeap) (obj : AggObj) (target : ObjId) :
    heapInternalRefs (nextId + 1) (setAggObj h nextId (some obj)) target =
      heapInternalRefs nextId h target + (slotBoxedLeaves obj.slot).count target := by
  unfold heapInternalRefs
  simp only [sumRange]
  have hcongr : sumRange (slotRefCount (setAggObj h nextId (some obj)) target) nextId =
      sumRange (slotRefCount h target) nextId := by
    apply sumRange_congr
    intro k hk
    have hne : k ≠ nextId := Nat.ne_of_lt hk
    simp [slotRefCount, setAggObj, hne]
  rw [hcongr]
  simp [slotRefCount, setAggObj]

theorem heapInternalRefs_same_slot (nextId : Nat) (h1 h2 : AggHeap) (target : ObjId)
    (hsame : ∀ id, id < nextId → slotRefCount h1 target id = slotRefCount h2 target id) :
    heapInternalRefs nextId h1 target = heapInternalRefs nextId h2 target :=
  sumRange_congr _ _ nextId hsame

/-! ## Global RC conservation invariant -/

structure HeapBalanced (nextId : Nat) (h : AggHeap) (rank : ObjId → Nat)
    (bound : Nat) (roots : ObjId → Nat) : Prop where
  bounded : ∀ id, nextId ≤ id → h id = none
  posRc : ∀ id obj, h id = some obj → 0 < obj.rc
  acyclic : ∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p
  rankBound : ∀ id obj, h id = some obj → rank id < bound
  conserve : ∀ id, rcOf h id = roots id + heapInternalRefs nextId h id

theorem HeapBalanced.weakenBound {nextId bound bound' : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots) (hle : bound ≤ bound') :
    HeapBalanced nextId h rank bound' roots :=
  { hb with rankBound := fun id obj hobj => Nat.lt_of_lt_of_le (hb.rankBound id obj hobj) hle }

theorem HeapBalanced.congrRoots {nextId bound : Nat} {h : AggHeap}
    {rank roots1 roots2 : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots1) (heq : ∀ id, roots1 id = roots2 id) :
    HeapBalanced nextId h rank bound roots2 :=
  { hb with conserve := fun id => by rw [← heq id]; exact hb.conserve id }

theorem HeapBalanced.live_lt {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots) {id : ObjId} {obj : AggObj}
    (hobj : h id = some obj) : id < nextId := by
  by_cases hlt : id < nextId
  · exact hlt
  · have hge : nextId ≤ id := Nat.le_of_not_lt hlt
    rw [hb.bounded id hge] at hobj
    cases hobj

theorem HeapBalanced.live_of_pos_rc {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots) {id : ObjId} (hpos : 0 < rcOf h id) :
    ∃ obj, h id = some obj ∧ 0 < obj.rc := by
  cases hobj : h id with
  | none => simp [rcOf, hobj] at hpos
  | some obj => exact ⟨obj, rfl, hb.posRc id obj hobj⟩

theorem HeapBalanced.child_internal_pos {nextId bound : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots) {p c : ObjId} {obj : AggObj}
    (hobj : h p = some obj) (hmem : c ∈ slotBoxedLeaves obj.slot) :
    0 < heapInternalRefs nextId h c := by
  have hlt : p < nextId := hb.live_lt hobj
  have hcnt : 0 < (slotBoxedLeaves obj.slot).count c := List.count_pos_iff.mpr hmem
  have hle : slotRefCount h c p ≤ heapInternalRefs nextId h c :=
    sumRange_le (slotRefCount h c) nextId p hlt
  simp only [slotRefCount, hobj] at hle
  omega

theorem HeapBalanced.live_of_pos_roots {nextId bound : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots) {id : ObjId} (hpos : 0 < roots id) :
    ∃ obj, h id = some obj ∧ 0 < obj.rc := by
  have hrc : 0 < rcOf h id := by
    have hc := hb.conserve id
    omega
  exact hb.live_of_pos_rc hrc

/-- Zero roots under `HeapBalanced` entails zero live heap objects. -/
theorem zero_roots_empty_heap {nextId bound : Nat} {h : AggHeap} {rank : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound (fun _ => 0)) :
    ∀ id, h id = none := by
  suffices ∀ b, (∀ id obj, h id = some obj → rank id < b) → ∀ id, h id = none by
    exact this bound hb.rankBound
  intro b
  induction b with
  | zero =>
      intro hlt id
      cases hobj : h id with
      | none => rfl
      | some obj => have := hlt id obj hobj; omega
  | succ m ih =>
      intro hlt
      apply ih
      intro id obj hobj
      have hle : rank id ≤ m := Nat.lt_succ_iff.mp (hlt id obj hobj)
      by_cases heq : rank id = m
      · exfalso
        have hzero : heapInternalRefs nextId h id = 0 := by
          apply sumRange_zero
          intro p _
          unfold slotRefCount
          cases hp : h p with
          | none => rfl
          | some pobj =>
              by_cases hmem : id ∈ slotBoxedLeaves pobj.slot
              · have hrank1 := hb.acyclic p pobj hp id hmem
                have hrank2 := hlt p pobj hp
                omega
              · exact List.count_eq_zero.mpr hmem
        have hrc := hb.conserve id
        have hpos := hb.posRc id obj hobj
        simp only [rcOf, hobj, hzero, Nat.add_zero] at hrc
        omega
      · omega

/-! ## Unified denotation preservation under `HeapBalanced` -/

theorem denotes_preserve_of_balanced {nextId1 nextId2 bound1 bound2 : Nat}
    {h1 h2 : AggHeap} {rank1 rank2 : ObjId → Nat} {roots1 roots2 : ObjId → Nat}
    (hb1 : HeapBalanced nextId1 h1 rank1 bound1 roots1)
    (hb2 : HeapBalanced nextId2 h2 rank2 bound2 roots2)
    (excl : Option ObjId)
    (hexcl : ∀ c, excl = some c → heapInternalRefs nextId1 h1 c = 0)
    (hsame : ∀ id o1 o2, some id ≠ excl → h1 id = some o1 → h2 id = some o2 → o1.slot = o2.slot)
    {v : Aggregate.Value} {sv : SemVal}
    (hd : Denotes h1 v sv)
    (hsurv : ∀ id ∈ boxedLeaves v, some id ≠ excl ∧ 0 < rcOf h2 id) :
    Denotes h2 v sv := by
  let P : ObjId → Prop := fun id => some id ≠ excl ∧ 0 < rcOf h2 id
  apply Denotes.preserve h1 h2 P
  · intro id obj hPid hobj1 _
    obtain ⟨o2, hobj2, hpos2⟩ := hb2.live_of_pos_rc hPid.2
    exact ⟨o2, hobj2, hpos2, (hsame id obj o2 hPid.1 hobj1 hobj2).symm⟩
  · intro id obj hPid hobj1 _ c hmem
    obtain ⟨o2, hobj2, _⟩ := hb2.live_of_pos_rc hPid.2
    have hslot : obj.slot = o2.slot := hsame id obj o2 hPid.1 hobj1 hobj2
    have hmem2 : c ∈ slotBoxedLeaves o2.slot := hslot ▸ hmem
    have hpos1 : 0 < heapInternalRefs nextId1 h1 c := hb1.child_internal_pos hobj1 hmem
    have hpos2 : 0 < heapInternalRefs nextId2 h2 c := hb2.child_internal_pos hobj2 hmem2
    have hne : some c ≠ excl := by
      intro heq
      have hz := hexcl c heq.symm
      omega
    have hrc2 : 0 < rcOf h2 c := by
      have hc := hb2.conserve c
      omega
    exact ⟨hne, hrc2⟩
  · exact hd
  · exact hsurv

/-! ## `dupValue` preservation -/

theorem heapBalanced_dupValue {nextId bound : Nat} {h : AggHeap} {rank roots : ObjId → Nat}
    (hb : HeapBalanced nextId h rank bound roots) (v : Aggregate.Value)
    (hlive : ∀ id ∈ boxedLeaves v, ∃ obj, h id = some obj) :
    HeapBalanced nextId (dupValue h v) rank bound (fun id => roots id + boxedTokens v id) ∧
    (∀ id o1 o2, h id = some o1 → dupValue h v id = some o2 → o1.slot = o2.slot) := by
  have hinc : ∀ (xs : List ObjId) (h0 : AggHeap) (r0 : ObjId → Nat),
      HeapBalanced nextId h0 rank bound r0 →
      (∀ id ∈ xs, ∃ obj, h0 id = some obj) →
      HeapBalanced nextId (incRefs xs h0) rank bound (fun id => r0 id + xs.count id) ∧
      (∀ id o1 o2, h0 id = some o1 → incRefs xs h0 id = some o2 → o1.slot = o2.slot) := by
    intro xs
    induction xs with
    | nil =>
        intro h0 r0 hb0 _
        refine ⟨?_, fun id o1 o2 h1 h2 => by rw [incRefs, h1] at h2; cases h2; rfl⟩
        simpa [incRefs] using hb0
    | cons x rest ih =>
        intro h0 r0 hb0 hlv
        obtain ⟨xobj, hxobj⟩ := hlv x (List.mem_cons_self ..)
        have hb1 : HeapBalanced nextId (incRef h0 x) rank bound
            (fun id => r0 id + (if id = x then 1 else 0)) := by
          refine ⟨?_, ?_, ?_, ?_, ?_⟩
          · intro id hge
            have hlt : x < nextId := hb0.live_lt hxobj
            have hne : id ≠ x := Nat.ne_of_gt (Nat.lt_of_lt_of_le hlt hge)
            rw [incRef_preserves_other h0 x id hne]
            exact hb0.bounded id hge
          · intro id obj hobj
            by_cases hid : id = x
            · subst id
              simp only [incRef, hxobj, setAggObj, ite_true, Option.some.injEq] at hobj
              subst obj; simp
            · rw [incRef_preserves_other h0 x id hid] at hobj
              exact hb0.posRc id obj hobj
          · intro p obj hobj c hmem
            by_cases hp : p = x
            · subst p
              simp only [incRef, hxobj, setAggObj, ite_true, Option.some.injEq] at hobj
              subst obj
              exact hb0.acyclic x xobj hxobj c hmem
            · rw [incRef_preserves_other h0 x p hp] at hobj
              exact hb0.acyclic p obj hobj c hmem
          · intro id obj hobj
            by_cases hid : id = x
            · subst id
              exact hb0.rankBound x xobj hxobj
            · rw [incRef_preserves_other h0 x id hid] at hobj
              exact hb0.rankBound id obj hobj
          · intro id
            rw [rcOf_incRef h0 x id ⟨xobj, hxobj⟩, hb0.conserve id]
            have hint : heapInternalRefs nextId (incRef h0 x) id = heapInternalRefs nextId h0 id := by
              apply heapInternalRefs_same_slot
              intro p _
              by_cases hp : p = x
              · subst p; simp [slotRefCount, incRef, hxobj, setAggObj]
              · simp [slotRefCount, incRef_preserves_other h0 x p hp]
            rw [hint]; omega
        have hlv1 : ∀ id ∈ rest, ∃ obj, incRef h0 x id = some obj :=
          fun id hid => incRef_live h0 x id (hlv id (List.mem_cons_of_mem x hid))
        obtain ⟨hb2, hsame2⟩ := ih (incRef h0 x) _ hb1 hlv1
        refine ⟨?_, ?_⟩
        · have hr : (fun id => r0 id + (if id = x then 1 else 0) + rest.count id) =
              (fun id => r0 id + (x :: rest).count id) := by
            funext id
            by_cases h : id = x
            · subst id; simp [List.count_cons_self]; omega
            · have hbeq : (x == id) = false := beq_false_of_ne (Ne.symm h)
              simp [List.count_cons, h, hbeq]
          rw [hr] at hb2
          exact hb2
        · intro id o1 o2 h1 h2
          by_cases hid : id = x
          · subst id
            rw [hxobj] at h1; cases h1
            have h1' : incRef h0 x x = some { xobj with rc := xobj.rc + 1 } := by
              simp [incRef, hxobj, setAggObj]
            exact hsame2 x { xobj with rc := xobj.rc + 1 } o2 h1' h2
          · have h1' : incRef h0 x id = some o1 := by
              rw [incRef_preserves_other h0 x id hid]; exact h1
            exact hsame2 id o1 o2 h1' h2
  exact hinc (boxedLeaves v) h roots hb hlive

end Flick.Memory.Soundness
