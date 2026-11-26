import Std

/-!
# Abstract unboxed aggregate values and counted-heap token semantics (#744)

Models target-independent unboxed aggregates (trees of immediates, boxed
`ObjId` leaves, and nested aggregates), target-independent `CellLayout`
compatibility descriptors, and the counted-heap semantics of:

- aggregate `dup` / `drop` (adding/subtracting `boxedTokens`; no-op on immediate aggregates),
- `project` from a borrowed aggregate and survival of a projected nested aggregate
  when its parent aggregate is dropped,
- `boxAggregate` fallback-cell boxing (conserving descendant tokens),
- `unboxBorrow` (retaining descendant tokens without releasing the fallback cell),
- `unboxConsume` (unique zero-extra-retain transfer vs shared retain-then-decrement
  preserving all external aliases), and
- typed-cell `reuse` with full `CellLayout` compatibility (dropping an incompatible
  same-capacity candidate and allocating a fresh replacement while preserving all
  other heap objects).

**Scope limits:** Abstract model theorems; they do NOT prove verified Rust
extraction, target ABI byte layouts, compiler/lowering equivalence, fuel metering,
or physical machine-memory reclamation.
-/

namespace Flick.Memory.Aggregate

abbrev ObjId := Nat

/-- Target-independent value representation mirroring `flick_ir::memory::ValueRep`. -/
inductive ValueRep where
  | cell
  | aggregate
  deriving DecidableEq, Repr

/-- Target-independent heap-cell layout descriptor mirroring `flick_ir::memory::CellLayout`. -/
structure CellLayout where
  capacity : Nat
  align : Nat
  ownership : Nat
  deriving DecidableEq, Repr

/-- Abstract aggregate value tree: immediates, boxed `ObjId` leaves, and nested aggregates. -/
inductive Value where
  | imm
  | boxed (id : ObjId)
  | agg (tag : Nat) (fields : List Value)
  deriving Repr

mutual
  /-- Ordered multiset list of boxed `ObjId` leaves inside a value. -/
  def boxedLeaves : Value → List ObjId
    | .imm => []
    | .boxed id => [id]
    | .agg _ fields => boxedLeavesList fields

  def boxedLeavesList : List Value → List ObjId
    | [] => []
    | v :: vs => boxedLeaves v ++ boxedLeavesList vs
end

/-- Multiset of boxed heap-object tokens owned by a value (`0` for immediate aggregates). -/
def boxedTokens (v : Value) (id : ObjId) : Nat :=
  (boxedLeaves v).count id

mutual
  /-- True iff `v` contains no boxed heap references (immediate scalar or immediate aggregate). -/
  def isImmediate : Value → Bool
    | .imm => true
    | .boxed _ => false
    | .agg _ fields => isImmediateList fields

  def isImmediateList : List Value → Bool
    | [] => true
    | v :: vs => isImmediate v && isImmediateList vs
end

mutual
  theorem immediate_boxedLeaves_nil (v : Value) :
      isImmediate v = true → boxedLeaves v = [] := by
    cases v with
    | imm => intro _; rfl
    | boxed _ => intro h; cases h
    | agg _ fields =>
        intro h
        exact immediateList_boxedLeaves_nil fields h

  theorem immediateList_boxedLeaves_nil (vs : List Value) :
      isImmediateList vs = true → boxedLeavesList vs = [] := by
    cases vs with
    | nil => intro _; rfl
    | cons v rest =>
        intro h
        simp only [isImmediateList, Bool.and_eq_true] at h
        simp [boxedLeavesList, immediate_boxedLeaves_nil v h.1,
          immediateList_boxedLeaves_nil rest h.2]
end

theorem immediate_boxedTokens_zero (v : Value) (h : isImmediate v = true) (id : ObjId) :
    boxedTokens v id = 0 := by
  simp [boxedTokens, immediate_boxedLeaves_nil v h]

theorem boxedLeavesList_getElem_count_le (fields : List Value) (i : Nat) (field : Value)
    (h : fields[i]? = some field) (k : ObjId) :
    (boxedLeaves field).count k ≤ (boxedLeavesList fields).count k := by
  induction fields generalizing i with
  | nil => simp at h
  | cons v rest ih =>
      cases i with
      | zero =>
          simp only [List.getElem?_cons_zero, Option.some.injEq] at h
          subst h
          simp [boxedLeavesList, List.count_append]
      | succ j =>
          simp only [List.getElem?_cons_succ] at h
          have hle := ih j h
          simp only [boxedLeavesList, List.count_append]
          omega

theorem boxedTokens_field_le (tag : Nat) (fields : List Value) (i : Nat) (field : Value)
    (h : fields[i]? = some field) (k : ObjId) :
    boxedTokens field k ≤ boxedTokens (.agg tag fields) k :=
  boxedLeavesList_getElem_count_le fields i field h k

/-! ## Idealized counted heap with typed cell layouts and fallback cells -/

inductive AggSlot where
  | tagged (tag : Nat) (layout : CellLayout) (fields : List Value)
  | boxedAgg (inner : Value) (layout : CellLayout)
  | reuseToken (layout : CellLayout)
  | opaque
  deriving Repr

structure AggObj where
  rc : Nat
  slot : AggSlot
  deriving Repr

abbrev AggHeap := ObjId → Option AggObj

def setAggObj (h : AggHeap) (id : ObjId) (obj : Option AggObj) : AggHeap :=
  fun other => if other = id then obj else h other

def rcOf (h : AggHeap) (id : ObjId) : Nat :=
  match h id with
  | some obj => obj.rc
  | none => 0

def incRef (h : AggHeap) (id : ObjId) : AggHeap :=
  match h id with
  | none => h
  | some obj => setAggObj h id (some { obj with rc := obj.rc + 1 })

def incRefs : List ObjId → AggHeap → AggHeap
  | [], h => h
  | id :: rest, h => incRefs rest (incRef h id)

def decRef (h : AggHeap) (id : ObjId) : AggHeap :=
  match h id with
  | none => h
  | some obj =>
      if obj.rc ≤ 1 then setAggObj h id none
      else setAggObj h id (some { obj with rc := obj.rc - 1 })

def decRefs : List ObjId → AggHeap → AggHeap
  | [], h => h
  | id :: rest, h => decRefs rest (decRef h id)

/-- Duplicate an aggregate (or any value) by retaining all of its boxed leaves. -/
def dupValue (h : AggHeap) (v : Value) : AggHeap :=
  incRefs (boxedLeaves v) h

/-- Drop an aggregate (or any value) by releasing all of its boxed leaves. -/
def dropValue (h : AggHeap) (v : Value) : AggHeap :=
  decRefs (boxedLeaves v) h

theorem incRef_live (h : AggHeap) (x y : ObjId) (hy : ∃ obj, h y = some obj) :
    ∃ obj', incRef h x y = some obj' := by
  unfold incRef
  cases hx : h x with
  | none => exact hy
  | some o =>
      by_cases hxy : y = x
      · subst hxy
        exact ⟨{ o with rc := o.rc + 1 }, by simp [setAggObj]⟩
      · simpa [setAggObj, hxy] using hy

theorem incRef_preserves_other (h : AggHeap) (x y : ObjId) (hne : y ≠ x) :
    incRef h x y = h y := by
  unfold incRef
  cases h x with
  | none => rfl
  | some o => simp [setAggObj, hne]

theorem incRefs_preserves_not_mem (xs : List ObjId) (h : AggHeap) (y : ObjId) (hnot : y ∉ xs) :
    incRefs xs h y = h y := by
  induction xs generalizing h with
  | nil => rfl
  | cons x rest ih =>
      have hne : y ≠ x := fun eq => hnot (eq ▸ List.mem_cons_self ..)
      have hnot' : y ∉ rest := fun mem => hnot (List.mem_cons_of_mem x mem)
      rw [incRefs, ih (incRef h x) hnot', incRef_preserves_other h x y hne]

theorem rcOf_incRef (h : AggHeap) (x k : ObjId) (hlive : ∃ obj, h x = some obj) :
    rcOf (incRef h x) k = rcOf h k + (if k = x then 1 else 0) := by
  obtain ⟨obj, hobj⟩ := hlive
  by_cases hkx : k = x
  · subst hkx
    simp [incRef, hobj, rcOf, setAggObj]
  · simp [incRef, hobj, rcOf, setAggObj, hkx]

theorem rcOf_incRefs (xs : List ObjId) (h : AggHeap)
    (hlive : ∀ id ∈ xs, ∃ obj, h id = some obj) (k : ObjId) :
    rcOf (incRefs xs h) k = rcOf h k + xs.count k := by
  induction xs generalizing h with
  | nil => simp [incRefs]
  | cons x rest ih =>
      have hx : ∃ obj, h x = some obj := hlive x (List.mem_cons_self ..)
      have hrest : ∀ id ∈ rest, ∃ obj, incRef h x id = some obj :=
        fun id hid => incRef_live h x id (hlive id (List.mem_cons_of_mem x hid))
      rw [incRefs, ih (incRef h x) hrest, rcOf_incRef h x k hx]
      by_cases hkx : k = x
      · subst hkx
        simp [List.count_cons_self]
        omega
      · have hbeq : (x == k) = false := beq_false_of_ne (Ne.symm hkx)
        simp [List.count_cons, hkx, hbeq]

theorem rcOf_decRef (h : AggHeap) (x k : ObjId) :
    rcOf (decRef h x) k = rcOf h k - (if k = x then 1 else 0) := by
  by_cases hkx : k = x
  · subst hkx
    unfold decRef rcOf
    cases hk : h k with
    | none => simp [hk]
    | some obj =>
        by_cases hle : obj.rc ≤ 1
        · simp [hle, setAggObj]
        · simp [hle, setAggObj]
  · simp only [hkx, ite_false, Nat.sub_zero]
    unfold decRef
    cases hx : h x with
    | none => rfl
    | some obj =>
        by_cases hle : obj.rc ≤ 1 <;> simp [hle, rcOf, setAggObj, hkx]

theorem rcOf_decRefs (xs : List ObjId) (h : AggHeap) (k : ObjId) :
    rcOf (decRefs xs h) k = rcOf h k - xs.count k := by
  induction xs generalizing h with
  | nil => simp [decRefs]
  | cons x rest ih =>
      rw [decRefs, ih (decRef h x), rcOf_decRef h x k]
      by_cases hkx : k = x
      · subst hkx
        simp [List.count_cons_self]
        omega
      · have hbeq : (x == k) = false := beq_false_of_ne (Ne.symm hkx)
        simp [List.count_cons, hkx, hbeq]

/-! ### 1 & 2. Aggregate `dup` and `drop` -/

/-- `dup` on an aggregate adds `boxedTokens v` to every heap object's refcount. -/
theorem dupValue_adds_boxedTokens (h : AggHeap) (v : Value)
    (hlive : ∀ id ∈ boxedLeaves v, ∃ obj, h id = some obj) (k : ObjId) :
    rcOf (dupValue h v) k = rcOf h k + boxedTokens v k :=
  rcOf_incRefs (boxedLeaves v) h hlive k

/-- `dup` on an immediate aggregate is an exact no-op on the heap. -/
theorem dupValue_immediate_noop (h : AggHeap) (v : Value) (himm : isImmediate v = true) :
    dupValue h v = h := by
  simp [dupValue, immediate_boxedLeaves_nil v himm, incRefs]

/-- `drop` on an aggregate subtracts `boxedTokens v` from every heap object's refcount. -/
theorem dropValue_subtracts_boxedTokens (h : AggHeap) (v : Value) (k : ObjId) :
    rcOf (dropValue h v) k = rcOf h k - boxedTokens v k :=
  rcOf_decRefs (boxedLeaves v) h k

/-- `drop` on an immediate aggregate is an exact no-op on the heap. -/
theorem dropValue_immediate_noop (h : AggHeap) (v : Value) (himm : isImmediate v = true) :
    dropValue h v = h := by
  simp [dropValue, immediate_boxedLeaves_nil v himm, decRefs]

/-! ### 3. Aggregate `project` and parent-drop survival -/

/-- Projecting field `i` from a borrowed aggregate retains `boxedTokens field`
while leaving the receiver's own tokens intact. -/
def projectAgg (h : AggHeap) (v : Value) (i : Nat) : Option (AggHeap × Value) :=
  match v with
  | .agg _ fields =>
      match fields[i]? with
      | none => none
      | some field => some (dupValue h field, field)
  | _ => none

theorem projectAgg_retains_field (h : AggHeap) (tag : Nat) (fields : List Value)
    (i : Nat) (field : Value) (hget : fields[i]? = some field)
    (hlive : ∀ id ∈ boxedLeaves field, ∃ obj, h id = some obj) :
    ∃ h', projectAgg h (.agg tag fields) i = some (h', field) ∧
      ∀ k, rcOf h' k = rcOf h k + boxedTokens field k := by
  refine ⟨dupValue h field, by simp [projectAgg, hget], ?_⟩
  intro k
  exact dupValue_adds_boxedTokens h field hlive k

/-- Projecting a nested aggregate field and then dropping the parent aggregate
preserves all boxed descendant tokens owned by the projected field. -/
theorem projectAgg_survives_parent_drop (h : AggHeap) (tag : Nat) (fields : List Value)
    (i : Nat) (field : Value) (hget : fields[i]? = some field)
    (hlive : ∀ id ∈ boxedLeaves field, ∃ obj, h id = some obj)
    (hparent : ∀ k, boxedTokens (.agg tag fields) k ≤ rcOf h k) :
    ∃ h' h'',
      projectAgg h (.agg tag fields) i = some (h', field) ∧
      h'' = dropValue h' (.agg tag fields) ∧
      ∀ k, boxedTokens field k ≤ rcOf h'' k ∧
        rcOf h'' k + boxedTokens (.agg tag fields) k = rcOf h k + boxedTokens field k := by
  refine ⟨dupValue h field, dropValue (dupValue h field) (.agg tag fields),
    by simp [projectAgg, hget], rfl, ?_⟩
  intro k
  have hdup := dupValue_adds_boxedTokens h field hlive k
  have hdrop := dropValue_subtracts_boxedTokens (dupValue h field) (.agg tag fields) k
  have hpk := hparent k
  have hfk := boxedTokens_field_le tag fields i field hget k
  omega

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

end Flick.Memory.Aggregate
