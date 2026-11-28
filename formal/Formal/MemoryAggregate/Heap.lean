import Formal.MemoryAggregate.HeapBase

/-!
# Fallback-cell boxing, unboxing, and typed-cell reuse (#744)
-/

namespace Flick.Memory.Aggregate

/-- Direct boxed child leaves owned by a heap cell slot. -/
def slotBoxedLeaves : AggSlot → List ObjId
  | .tagged _ _ fields => boxedLeavesList fields
  | .boxedAgg inner _ => boxedLeaves inner
  | .reuseToken _ => []
  | .opaque => []

/-- Single-step cell drop: frees a unique cell (`rc ≤ 1`) and decrements its
direct `slotBoxedLeaves`, or decrements `rc` on a shared cell (`rc ≥ 2`). -/
def dropCellOnce (h : AggHeap) (id : ObjId) : AggHeap :=
  match h id with
  | none => h
  | some ⟨rc, slot⟩ =>
      if rc ≤ 1 then decRefs (slotBoxedLeaves slot) (setAggObj h id none)
      else setAggObj h id (some ⟨rc - 1, slot⟩)

theorem setAggObj_cancel (h : AggHeap) (id : ObjId) (obj : Option AggObj)
    (hnone : h id = none) :
    setAggObj (setAggObj h id obj) id none = h := by
  funext k
  by_cases hk : k = id
  · subst hk; simp [setAggObj, hnone]
  · simp [setAggObj, hk]

/-! ### 4. `boxAggregate` fallback-cell boxing -/

/-- Consumes an owned aggregate and moves its references directly into a fresh
boxed fallback cell (`rc = 1`) with zero extra refcount changes on descendants. -/
def boxAggregateHeap (h : AggHeap) (fresh : ObjId) (layout : CellLayout) (v : Value) :
    AggHeap × ObjId :=
  (setAggObj h fresh (some ⟨1, .boxedAgg v layout⟩), fresh)

theorem boxAggregate_conserves_tokens (h : AggHeap) (fresh : ObjId)
    (layout : CellLayout) (v : Value) :
    let res := boxAggregateHeap h fresh layout v
    res.2 = fresh ∧
    res.1 fresh = some ⟨1, .boxedAgg v layout⟩ ∧
    rcOf res.1 fresh = 1 ∧
    ∀ other, other ≠ fresh → res.1 other = h other ∧ rcOf res.1 other = rcOf h other := by
  refine ⟨rfl, by simp [boxAggregateHeap, setAggObj],
    by simp [boxAggregateHeap, rcOf, setAggObj], ?_⟩
  intro other hne
  simp [boxAggregateHeap, rcOf, setAggObj, hne]

/-! ### 5. `unboxBorrow` from a boxed fallback cell -/

/-- Borrows a boxed fallback cell and returns an owned aggregate copy by
retaining all boxed leaves inside it, without releasing the cell itself. -/
def unboxBorrowHeap (h : AggHeap) (id : ObjId) : Option (AggHeap × Value) :=
  match h id with
  | some ⟨rc, .boxedAgg inner _⟩ =>
      if 0 < rc then some (dupValue h inner, inner) else none
  | _ => none

theorem unboxBorrow_retains_without_releasing (h : AggHeap) (id : ObjId)
    (rc : Nat) (inner : Value) (layout : CellLayout)
    (hobj : h id = some ⟨rc, .boxedAgg inner layout⟩)
    (hpos : 0 < rc)
    (hacyc : id ∉ boxedLeaves inner)
    (hlive : ∀ x ∈ boxedLeaves inner, ∃ obj, h x = some obj) :
    ∃ h', unboxBorrowHeap h id = some (h', inner) ∧
      h' id = some ⟨rc, .boxedAgg inner layout⟩ ∧
      ∀ k, rcOf h' k = rcOf h k + boxedTokens inner k := by
  refine ⟨dupValue h inner, by simp [unboxBorrowHeap, hobj, hpos], ?_, ?_⟩
  · rw [dupValue, incRefs_preserves_not_mem (boxedLeaves inner) h id hacyc, hobj]
  · intro k
    exact dupValue_adds_boxedTokens h inner hlive k

/-! ### 6. `unboxConsume` (unique transfer vs shared retain-then-decrement) -/

/-- Consumes a boxed fallback cell:
- unique (`rc = 1`): frees the cell slot and transfers `inner` with zero extra descendant retains;
- shared (`rc ≥ 2`): retains `inner`'s boxed leaves before decrementing the cell refcount,
  preserving every external alias. -/
def unboxConsumeHeap (h : AggHeap) (id : ObjId) : Option (AggHeap × Value) :=
  match h id with
  | some ⟨rc, .boxedAgg inner layout⟩ =>
      if rc = 1 then
        some (setAggObj h id none, inner)
      else if 2 ≤ rc then
        let h1 := dupValue h inner
        some (setAggObj h1 id (some ⟨rc - 1, .boxedAgg inner layout⟩), inner)
      else
        none
  | _ => none

theorem unboxConsume_unique_transfers (h : AggHeap) (id : ObjId)
    (inner : Value) (layout : CellLayout)
    (hobj : h id = some ⟨1, .boxedAgg inner layout⟩) :
    ∃ h', unboxConsumeHeap h id = some (h', inner) ∧
      h' id = none ∧
      ∀ other, other ≠ id → h' other = h other ∧ rcOf h' other = rcOf h other := by
  refine ⟨setAggObj h id none, by simp [unboxConsumeHeap, hobj], by simp [setAggObj], ?_⟩
  intro other hne
  simp [setAggObj, rcOf, hne]

theorem unboxConsume_shared_retains_preserves_aliases (h : AggHeap) (id : ObjId)
    (rc : Nat) (inner : Value) (layout : CellLayout)
    (hobj : h id = some ⟨rc, .boxedAgg inner layout⟩)
    (hshared : 2 ≤ rc)
    (hlive : ∀ x ∈ boxedLeaves inner, ∃ obj, h x = some obj) :
    ∃ h', unboxConsumeHeap h id = some (h', inner) ∧
      h' id = some ⟨rc - 1, .boxedAgg inner layout⟩ ∧
      ∀ other, other ≠ id → rcOf h' other = rcOf h other + boxedTokens inner other := by
  have hne1 : rc ≠ 1 := by omega
  refine ⟨setAggObj (dupValue h inner) id (some ⟨rc - 1, .boxedAgg inner layout⟩),
    by simp [unboxConsumeHeap, hobj, hne1, hshared],
    by simp [setAggObj], ?_⟩
  intro other hne
  simp only [rcOf, setAggObj, hne, ite_false]
  exact dupValue_adds_boxedTokens h inner hlive other

/-! ### 7. Typed-cell `reuse` with `CellLayout` compatibility -/

/-- Typed-cell reuse checking uniqueness AND full `CellLayout` compatibility
(not merely logical field count). -/
def reuseTypedCellHeap (h : AggHeap) (cand fresh : ObjId)
    (tag : Nat) (targetLayout : CellLayout) (fields : List Value) :
    AggHeap × ObjId × Bool :=
  match h cand with
  | some ⟨1, .reuseToken candLayout⟩ =>
      if candLayout = targetLayout then
        (setAggObj h cand (some ⟨1, .tagged tag targetLayout fields⟩), cand, true)
      else
        let h1 := decRef h cand
        (setAggObj h1 fresh (some ⟨1, .tagged tag targetLayout fields⟩), fresh, false)
  | _ =>
      let h1 := decRef h cand
      (setAggObj h1 fresh (some ⟨1, .tagged tag targetLayout fields⟩), fresh, false)

/-- Even when `candLayout.capacity = targetLayout.capacity`, an incompatible
`CellLayout` drops the candidate and allocates a fresh replacement while
preserving every other heap object. -/
theorem reuseTypedCell_incompatible_layout_fallback (h : AggHeap) (cand fresh : ObjId)
    (tag : Nat) (candLayout targetLayout : CellLayout) (fields : List Value)
    (htok : h cand = some ⟨1, .reuseToken candLayout⟩)
    (_hcap : candLayout.capacity = targetLayout.capacity)
    (hincompat : candLayout ≠ targetLayout)
    (hne : fresh ≠ cand) :
    let res := reuseTypedCellHeap h cand fresh tag targetLayout fields
    res.2.1 = fresh ∧ res.2.2 = false ∧
    res.1 cand = none ∧
    res.1 fresh = some ⟨1, .tagged tag targetLayout fields⟩ ∧
    ∀ other, other ≠ cand → other ≠ fresh → res.1 other = h other := by
  refine ⟨by simp [reuseTypedCellHeap, htok, hincompat],
    by simp [reuseTypedCellHeap, htok, hincompat],
    by simp [reuseTypedCellHeap, htok, hincompat, decRef, setAggObj, Ne.symm hne],
    by simp [reuseTypedCellHeap, htok, hincompat, setAggObj], ?_⟩
  intro other hnc hnf
  simp [reuseTypedCellHeap, htok, hincompat, decRef, setAggObj, hnc, hnf]

/-- Compatible unique typed-cell reuse reuses `cand` in place and preserves all
other heap objects. -/
theorem reuseTypedCell_compatible_in_place (h : AggHeap) (cand fresh : ObjId)
    (tag : Nat) (layout : CellLayout) (fields : List Value)
    (htok : h cand = some ⟨1, .reuseToken layout⟩) :
    let res := reuseTypedCellHeap h cand fresh tag layout fields
    res.2.1 = cand ∧ res.2.2 = true ∧
    res.1 cand = some ⟨1, .tagged tag layout fields⟩ ∧
    ∀ other, other ≠ cand → res.1 other = h other := by
  refine ⟨by simp [reuseTypedCellHeap, htok], by simp [reuseTypedCellHeap, htok],
    by simp [reuseTypedCellHeap, htok, setAggObj], ?_⟩
  intro other hne
  simp [reuseTypedCellHeap, htok, setAggObj, hne]

/-! ### 8. Fallback-cell round-trips and value-level aggregate-rejecting reuse (#463/#744) -/

/-- Boxing an aggregate into a fresh fallback cell and then dropping that unique
cell without unboxing frees the cell and releases the exact same descendant
tokens as dropping the aggregate directly. -/
theorem boxAggregate_then_drop_releases_descendants (h : AggHeap) (fresh : ObjId)
    (layout : CellLayout) (v : Value) (hfresh : h fresh = none) :
    let res := boxAggregateHeap h fresh layout v
    dropCellOnce res.1 res.2 = dropValue h v ∧
    ∀ k, rcOf (dropCellOnce res.1 res.2) k = rcOf h k - boxedTokens v k := by
  have hdrop : dropCellOnce (boxAggregateHeap h fresh layout v).1
      (boxAggregateHeap h fresh layout v).2 = dropValue h v := by
    simp only [boxAggregateHeap, dropCellOnce, setAggObj, ite_true, Nat.le_refl, slotBoxedLeaves]
    change decRefs (boxedLeaves v) (setAggObj (setAggObj h fresh (some ⟨1, .boxedAgg v layout⟩)) fresh none) =
      dropValue h v
    rw [setAggObj_cancel h fresh _ hfresh]
    rfl
  refine ⟨hdrop, ?_⟩
  intro k
  rw [hdrop]
  exact dropValue_subtracts_boxedTokens h v k

/-- Boxing an aggregate into a fresh fallback cell and immediately consuming it
uniquely with `unboxConsume` returns the exact original heap and aggregate with
zero extra descendant `dup`/`drop` operations. -/
theorem boxAggregate_then_unboxConsume_unique_identity (h : AggHeap) (fresh : ObjId)
    (layout : CellLayout) (v : Value) (hfresh : h fresh = none) :
    let res := boxAggregateHeap h fresh layout v
    unboxConsumeHeap res.1 res.2 = some (h, v) := by
  simp only [boxAggregateHeap, unboxConsumeHeap, setAggObj, ite_true]
  change some (setAggObj (setAggObj h fresh (some ⟨1, .boxedAgg v layout⟩)) fresh none, v) = some (h, v)
  rw [setAggObj_cancel h fresh _ hfresh]

/-- Value-level `DropReuse`: rejects unboxed aggregates (`none`), converts a
unique `tagged` cell into a `reuseToken` after releasing its old children, and
returns the null sentinel (`.imm`) while decrementing shared or non-reusable cells. -/
def dropReuseVal (h : AggHeap) : Value → Option (AggHeap × Value)
  | .agg _ _ => none
  | .imm => some (h, .imm)
  | .boxed id =>
      match h id with
      | none => none
      | some ⟨rc, .tagged tag layout oldFields⟩ =>
          if rc = 1 then
            some (decRefs (boxedLeavesList oldFields) (setAggObj h id (some ⟨1, .reuseToken layout⟩)),
              .boxed id)
          else if 2 ≤ rc then
            some (setAggObj h id (some ⟨rc - 1, .tagged tag layout oldFields⟩), .imm)
          else none
      | some _ => some (dropCellOnce h id, .imm)

theorem dropReuseVal_rejects_aggregate (h : AggHeap) (tag : Nat) (fields : List Value) :
    dropReuseVal h (.agg tag fields) = none := rfl

theorem dropReuseVal_unique_tagged (h : AggHeap) (id tag : Nat) (layout : CellLayout)
    (oldFields : List Value)
    (hobj : h id = some ⟨1, .tagged tag layout oldFields⟩)
    (hacyc : id ∉ boxedLeavesList oldFields) :
    ∃ h', dropReuseVal h (.boxed id) = some (h', .boxed id) ∧
      h' id = some ⟨1, .reuseToken layout⟩ ∧
      ∀ k, k ≠ id → rcOf h' k = rcOf h k - (boxedLeavesList oldFields).count k := by
  have hpres : ∀ (xs : List ObjId) (h0 : AggHeap) (y : ObjId), y ∉ xs → decRefs xs h0 y = h0 y := by
    intro xs
    induction xs with
    | nil => intros; rfl
    | cons x rest ih =>
        intro h0 y hnot
        have hne : y ≠ x := fun eq => hnot (eq ▸ List.mem_cons_self ..)
        have hnot' : y ∉ rest := fun mem => hnot (List.mem_cons_of_mem x mem)
        rw [decRefs, ih (decRef h0 x) y hnot']
        unfold decRef
        cases h0 x with
        | none => rfl
        | some o => by_cases hle : o.rc ≤ 1 <;> simp [hle, setAggObj, hne]
  refine ⟨decRefs (boxedLeavesList oldFields) (setAggObj h id (some ⟨1, .reuseToken layout⟩)),
    by simp [dropReuseVal, hobj], ?_, ?_⟩
  · rw [hpres _ _ id hacyc]
    simp [setAggObj]
  · intro k hne
    rw [rcOf_decRefs]
    simp [rcOf, setAggObj, hne]

theorem dropReuseVal_shared_preserves_aliases (h : AggHeap) (id rc tag : Nat)
    (layout : CellLayout) (oldFields : List Value)
    (hobj : h id = some ⟨rc, .tagged tag layout oldFields⟩) (hshared : 2 ≤ rc) :
    ∃ h', dropReuseVal h (.boxed id) = some (h', .imm) ∧
      h' id = some ⟨rc - 1, .tagged tag layout oldFields⟩ ∧
      ∀ other, other ≠ id → h' other = h other := by
  have hne1 : rc ≠ 1 := by omega
  refine ⟨setAggObj h id (some ⟨rc - 1, .tagged tag layout oldFields⟩),
    by simp [dropReuseVal, hobj, hne1, hshared], by simp [setAggObj], ?_⟩
  intro other hne
  simp [setAggObj, hne]

/-- Value-level typed-cell `Reuse`: rejects unboxed aggregates (`none`),
supports both Stage 2 `reuseToken` cells and Stage 1 live unique `tagged` cells
under full `CellLayout` equality, and falls back to dropping the candidate and
allocating at `fresh` on mismatched layout, shared cells, or null sentinels. -/
def reuseTypedCellVal (h : AggHeap) (cand : Value) (fresh : ObjId)
    (tag : Nat) (targetLayout : CellLayout) (fields : List Value) :
    Option (AggHeap × Value × Bool) :=
  match cand with
  | .agg _ _ => none
  | .imm =>
      some (setAggObj h fresh (some ⟨1, .tagged tag targetLayout fields⟩), .boxed fresh, false)
  | .boxed id =>
      match h id with
      | none => none
      | some ⟨1, .reuseToken candLayout⟩ =>
          if candLayout = targetLayout then
            some (setAggObj h id (some ⟨1, .tagged tag targetLayout fields⟩), .boxed id, true)
          else
            some (setAggObj (setAggObj h id none) fresh (some ⟨1, .tagged tag targetLayout fields⟩),
              .boxed fresh, false)
      | some ⟨1, .tagged _ candLayout oldFields⟩ =>
          if candLayout = targetLayout then
            let h1 := decRefs (boxedLeavesList oldFields)
              (setAggObj h id (some ⟨1, .tagged tag targetLayout fields⟩))
            some (h1, .boxed id, true)
          else
            let h1 := dropCellOnce h id
            some (setAggObj h1 fresh (some ⟨1, .tagged tag targetLayout fields⟩),
              .boxed fresh, false)
      | some _ =>
          let h1 := dropCellOnce h id
          some (setAggObj h1 fresh (some ⟨1, .tagged tag targetLayout fields⟩),
            .boxed fresh, false)

theorem reuseTypedCellVal_rejects_aggregate (h : AggHeap) (candTag : Nat)
    (candFields : List Value) (fresh tag : Nat) (targetLayout : CellLayout)
    (fields : List Value) :
    reuseTypedCellVal h (.agg candTag candFields) fresh tag targetLayout fields = none := rfl

theorem reuseTypedCellVal_live_tagged_compatible_in_place (h : AggHeap) (id fresh : ObjId)
    (oldTag newTag : Nat) (layout : CellLayout) (oldFields newFields : List Value)
    (hobj : h id = some ⟨1, .tagged oldTag layout oldFields⟩)
    (hacyc : id ∉ boxedLeavesList oldFields) :
    ∃ h', reuseTypedCellVal h (.boxed id) fresh newTag layout newFields = some (h', .boxed id, true) ∧
      h' id = some ⟨1, .tagged newTag layout newFields⟩ ∧
      ∀ k, k ≠ id → rcOf h' k = rcOf h k - (boxedLeavesList oldFields).count k := by
  have hpres : ∀ (xs : List ObjId) (h0 : AggHeap) (y : ObjId), y ∉ xs → decRefs xs h0 y = h0 y := by
    intro xs
    induction xs with
    | nil => intros; rfl
    | cons x rest ih =>
        intro h0 y hnot
        have hne : y ≠ x := fun eq => hnot (eq ▸ List.mem_cons_self ..)
        have hnot' : y ∉ rest := fun mem => hnot (List.mem_cons_of_mem x mem)
        rw [decRefs, ih (decRef h0 x) y hnot']
        unfold decRef
        cases h0 x with
        | none => rfl
        | some o => by_cases hle : o.rc ≤ 1 <;> simp [hle, setAggObj, hne]
  refine ⟨decRefs (boxedLeavesList oldFields) (setAggObj h id (some ⟨1, .tagged newTag layout newFields⟩)),
    by simp [reuseTypedCellVal, hobj], ?_, ?_⟩
  · rw [hpres _ _ id hacyc]
    simp [setAggObj]
  · intro k hne
    rw [rcOf_decRefs]
    simp [rcOf, setAggObj, hne]

end Flick.Memory.Aggregate
