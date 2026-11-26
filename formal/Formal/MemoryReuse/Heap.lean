import Formal.MemoryReuse.Defs
import Formal.MemoryArena

/-!
# Dynamic uniqueness, alias preservation, and Ref cycle boundary (phase M3)

Models an idealized counted heap of tagged constructors, records, and empty
`reuseToken` slots with explicit per-object reference counts corresponding to
`flick-vm/src/tier0/reuse.rs` and `flick-vm/src/tier0/patterns.rs`.

**Scope limits:** Abstract model theorems; they do NOT prove verified Rust
extraction, compiler/lowering equivalence, fuel metering, host-bridge FFI, or
physical machine-memory reclamation.
-/

namespace Flick.Memory.Reuse

abbrev ObjId := Nat

inductive HeapSlot where
  | tagged (tag : Nat) (fields : List ObjId)
  | record (fields : List (Nat × ObjId))
  | reuseToken (kind : TargetKind) (capacity : Nat)
  deriving DecidableEq, Repr

structure HeapObj where
  rc : Nat
  slot : HeapSlot
  deriving DecidableEq, Repr

abbrev Heap := ObjId → Option HeapObj

def setObj (h : Heap) (id : ObjId) (obj : Option HeapObj) : Heap :=
  fun other => if other = id then obj else h other

def decRef (h : Heap) (id : ObjId) : Heap :=
  match h id with
  | none => h
  | some obj =>
      if obj.rc ≤ 1 then setObj h id none
      else setObj h id (some { obj with rc := obj.rc - 1 })

def decRefs : List ObjId → Heap → Heap
  | [], h => h
  | id :: rest, h => decRefs rest (decRef h id)

inductive TokenResult where
  | token (id : ObjId)
  | null
  deriving DecidableEq, Repr

/-- Dynamic `DropReuse` step on a counted heap:
- unique (`rc = 1`) tagged/record -> drops old children immediately, leaves 0-child `reuseToken` with `rc = 1`
- shared (`rc ≥ 2`) -> decrements `rc` by 1, preserves slot untouched, returns `null`. -/
def dropReuseHeap (h : Heap) (id : ObjId) : Heap × TokenResult :=
  match h id with
  | none => (h, .null)
  | some obj =>
      if obj.rc = 1 then
        match obj.slot with
        | .tagged _ fields =>
            let h1 := setObj h id (some ⟨1, .reuseToken .tagged fields.length⟩)
            (decRefs fields h1, .token id)
        | .record fields =>
            let h1 := setObj h id (some ⟨1, .reuseToken .record fields.length⟩)
            (decRefs (fields.map Prod.snd) h1, .token id)
        | .reuseToken .. => (decRef h id, .null)
      else
        (decRef h id, .null)

/-- When `id` is uniquely owned (`rc = 1`), `dropReuseHeap` releases all old
fields immediately and returns `token id` pointing to a 0-child `reuseToken`
(assuming `id` is not in its own fields, i.e., acyclic counted data). -/
theorem dropReuseHeap_unique_tagged (h : Heap) (id tag : Nat) (fields : List ObjId)
    (hobj : h id = some ⟨1, .tagged tag fields⟩)
    (hacyc : id ∉ fields) :
    (dropReuseHeap h id).2 = .token id ∧
    (dropReuseHeap h id).1 id = some ⟨1, .reuseToken .tagged fields.length⟩ := by
  have preserve : ∀ (xs : List ObjId) (h' : Heap), id ∉ xs →
      h' id = some ⟨1, .reuseToken .tagged fields.length⟩ →
      (decRefs xs h') id = some ⟨1, .reuseToken .tagged fields.length⟩ := by
    intro xs
    induction xs with
    | nil => intro h' _ hs; exact hs
    | cons y ys ih =>
        intro h' hnot hs
        have hne : id ≠ y := fun eq => hnot (eq ▸ List.mem_cons_self ..)
        have hnot' : id ∉ ys := fun mem => hnot (List.mem_cons_of_mem y mem)
        apply ih (decRef h' y) hnot'
        simp only [decRef]
        cases h' y with
        | none => exact hs
        | some o =>
            by_cases hrc : o.rc ≤ 1 <;> simp [hrc, setObj, hne, hs]
  constructor
  · simp [dropReuseHeap, hobj]
  · simp only [dropReuseHeap, hobj, ↓reduceIte]
    apply preserve fields _ hacyc
    simp [setObj]

/-- When `id` is shared (`rc ≥ 2`), `dropReuseHeap` returns `null`, leaves `id`'s
slot payload completely unchanged for all external aliases, and leaves every
other heap object untouched. -/
theorem dropReuseHeap_shared_preserves_aliases (h : Heap) (id : ObjId) (obj : HeapObj)
    (hobj : h id = some obj) (hshared : 2 ≤ obj.rc) :
    (dropReuseHeap h id).2 = .null ∧
    (dropReuseHeap h id).1 id = some { obj with rc := obj.rc - 1 } ∧
    ∀ other, other ≠ id → (dropReuseHeap h id).1 other = h other := by
  have hne1 : obj.rc ≠ 1 := by omega
  have hgt1 : ¬ (obj.rc ≤ 1) := by omega
  refine ⟨by simp [dropReuseHeap, hobj, hne1],
    by simp [dropReuseHeap, hobj, hne1, decRef, hgt1, setObj], ?_⟩
  intro other hne
  simp [dropReuseHeap, hobj, hne1, decRef, hgt1, setObj, hne]

/-- Dynamic `Reuse` (for tagged constructors) on a counted heap with fresh fallback ID. -/
def reuseTaggedHeap (h : Heap) (cand : TokenResult) (fresh : ObjId)
    (tag : Nat) (fields : List ObjId) : Heap × ObjId × Bool :=
  match cand with
  | .token id =>
      match h id with
      | some ⟨1, .reuseToken .tagged cap⟩ =>
          if cap = fields.length then
            (setObj h id (some ⟨1, .tagged tag fields⟩), id, true)
          else
            let h1 := decRef h id
            (setObj h1 fresh (some ⟨1, .tagged tag fields⟩), fresh, false)
      | _ =>
          let h1 := decRef h id
          (setObj h1 fresh (some ⟨1, .tagged tag fields⟩), fresh, false)
  | .null =>
      (setObj h fresh (some ⟨1, .tagged tag fields⟩), fresh, false)

/-- Unique compatible reuse mutates the exact slot in place with zero fresh
allocation and leaves every other heap object `other ≠ id` untouched. -/
theorem reuseTaggedHeap_unique_in_place (h : Heap) (id fresh tag : Nat) (fields : List ObjId)
    (htok : h id = some ⟨1, .reuseToken .tagged fields.length⟩) :
    let res := reuseTaggedHeap h (.token id) fresh tag fields
    res.2.1 = id ∧ res.2.2 = true ∧
    res.1 id = some ⟨1, .tagged tag fields⟩ ∧
    ∀ other, other ≠ id → res.1 other = h other := by
  refine ⟨by simp [reuseTaggedHeap, htok], by simp [reuseTaggedHeap, htok],
    by simp [reuseTaggedHeap, htok, setObj],
    fun other hne => by simp [reuseTaggedHeap, htok, setObj, hne]⟩

/-- Shared/null fallback allocates at `fresh` (`h fresh = none`) and preserves
every pre-existing object in `h` (including all external aliases) unchanged. -/
theorem reuseTaggedHeap_null_preserves_aliases (h : Heap) (fresh tag : Nat) (fields : List ObjId) :
    let res := reuseTaggedHeap h .null fresh tag fields
    res.2.1 = fresh ∧ res.2.2 = false ∧
    res.1 fresh = some ⟨1, .tagged tag fields⟩ ∧
    ∀ other, other ≠ fresh → res.1 other = h other := by
  refine ⟨by simp [reuseTaggedHeap], by simp [reuseTaggedHeap],
    by simp [reuseTaggedHeap, setObj],
    fun other hne => by simp [reuseTaggedHeap, setObj, hne]⟩

/-- Dynamic `Update` on a record object: in-place when `rc = 1`, fresh allocation
when `rc ≥ 2`, preserving every external alias bit-for-bit. -/
def updateRecordHeap (h : Heap) (id fresh : ObjId) (updated : List (Nat × ObjId)) :
    Heap × ObjId × Bool :=
  match h id with
  | some ⟨1, .record _⟩ =>
      (setObj h id (some ⟨1, .record updated⟩), id, true)
  | _ =>
      let h1 := decRef h id
      (setObj h1 fresh (some ⟨1, .record updated⟩), fresh, false)

theorem updateRecordHeap_shared_preserves_aliases (h : Heap) (id fresh : ObjId)
    (oldFields updated : List (Nat × ObjId)) (rc : Nat)
    (hobj : h id = some ⟨rc, .record oldFields⟩) (hshared : 2 ≤ rc) (hne : fresh ≠ id) :
    let res := updateRecordHeap h id fresh updated
    res.2.1 = fresh ∧ res.2.2 = false ∧
    res.1 id = some ⟨rc - 1, .record oldFields⟩ ∧
    res.1 fresh = some ⟨1, .record updated⟩ := by
  have hne1 : rc ≠ 1 := by omega
  have hgt1 : ¬ (rc ≤ 1) := by omega
  cases rc with
  | zero => omega
  | succ k =>
      cases k with
      | zero => omega
      | succ m =>
          simp [updateRecordHeap, hobj, decRef, setObj, Ne.symm hne]

/-- Dropping an unused 0-child `reuseToken` (early failure, abort, fuel cut, or
non-allocating branch) frees the exact slot with zero effect on other objects. -/
theorem drop_unused_reuseToken_clean (h : Heap) (id : ObjId) (kind : TargetKind) (cap : Nat)
    (htok : h id = some ⟨1, .reuseToken kind cap⟩) :
    decRef h id id = none ∧ ∀ other, other ≠ id → decRef h id other = h other := by
  simp only [decRef, htok, Nat.le_refl, ite_true, setObj]
  exact ⟨by simp, fun other hne => by simp [hne]⟩

/-- Ref cycle boundary corollary (from `Flick.MemoryArena`): overwriting a cyclic
cell immediately emits the old cyclic payload for prompt RC drop while leaving
the cell owner in the root arena; an unbroken cycle's payload is retained until
`MemoryArena.close` detaches all slots. -/
theorem cycle_break_vs_teardown (id owner : Nat) (cyclicPayload replacement : MemoryArena.Value)
    (before after : MemoryArena.Arena) :
    (MemoryArena.replace ⟨owner, some cyclicPayload⟩ replacement).2 = [cyclicPayload] ∧
    MemoryArena.balance id (before ++ ⟨owner, some cyclicPayload⟩ :: after) +
        MemoryArena.token id replacement =
      MemoryArena.balance id (before ++ (MemoryArena.replace ⟨owner, some cyclicPayload⟩ replacement).1 :: after) +
        MemoryArena.inventory id [cyclicPayload] ∧
    MemoryArena.inventory id (MemoryArena.close (before ++ ⟨owner, some cyclicPayload⟩ :: after)).2 =
      MemoryArena.balance id (before ++ ⟨owner, some cyclicPayload⟩ :: after) := by
  refine ⟨rfl, ?_, (MemoryArena.close_balance id _).2⟩
  exact MemoryArena.replace_balance id before after ⟨owner, some cyclicPayload⟩ replacement

end Flick.Memory.Reuse
