import Formal.MemorySoundness.HeapInv.Base

/-!
# Rank-bounded recursive drop helpers (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Aggregate

/-! ## Rank-bounded recursive drop -/

mutual
  def dropIdFuel : Nat → AggHeap → ObjId → AggHeap
    | 0, h, _ => h
    | fuel + 1, h, id =>
        match h id with
        | none => h
        | some ⟨rc, slot⟩ =>
            if rc ≤ 1 then
              dropIdsFuel fuel (setAggObj h id none) (slotBoxedLeaves slot)
            else
              setAggObj h id (some ⟨rc - 1, slot⟩)

  def dropIdsFuel : Nat → AggHeap → List ObjId → AggHeap
    | _, h, [] => h
    | fuel, h, id :: rest => dropIdsFuel fuel (dropIdFuel fuel h id) rest
end

def dropValFuel (fuel : Nat) (h : AggHeap) (v : Aggregate.Value) : AggHeap :=
  dropIdsFuel fuel h (boxedLeaves v)

def dropValsFuel (fuel : Nat) (h : AggHeap) : List Aggregate.Value → AggHeap
  | [] => h
  | v :: vs => dropValsFuel fuel (dropValFuel fuel h v) vs

theorem dropValFuel_immediate_noop (fuel : Nat) (h : AggHeap) (v : Aggregate.Value)
    (himm : isImmediate v = true) :
    dropValFuel fuel h v = h := by
  simp [dropValFuel, immediate_boxedLeaves_nil v himm, dropIdsFuel]

theorem dropFuel_acyclic (fuel : Nat) (rank : ObjId → Nat) :
    (∀ h id,
      (∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) →
      ∀ p obj, dropIdFuel fuel h id p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) ∧
    (∀ h xs,
      (∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) →
      ∀ p obj, dropIdsFuel fuel h xs p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) := by
  induction fuel with
  | zero =>
      constructor
      · intro h id ha; simp only [dropIdFuel]; exact ha
      · intro h xs ha
        induction xs generalizing h with
        | nil => simp only [dropIdsFuel]; exact ha
        | cons x rest ih => simp only [dropIdsFuel, dropIdFuel]; exact ih h ha
  | succ f ih =>
      have h1 : ∀ h id,
          (∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) →
          ∀ p obj, dropIdFuel (f + 1) h id p = some obj →
            ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p := by
        intro h id ha p obj hp c hc
        unfold dropIdFuel at hp
        cases hobj : h id with
        | none => rw [hobj] at hp; exact ha p obj hp c hc
        | some iobj =>
            rw [hobj] at hp
            by_cases hle : iobj.rc ≤ 1
            · simp only [hle, ite_true] at hp
              apply ih.2 (setAggObj h id none) (slotBoxedLeaves iobj.slot) _ p obj hp c hc
              intro q qo hq
              by_cases hqid : q = id
              · subst q; simp [setAggObj] at hq
              · simp only [setAggObj, hqid, ite_false] at hq
                exact ha q qo hq
            · simp only [hle, ite_false] at hp
              by_cases hpid : p = id
              · subst p
                simp only [setAggObj, ite_true, Option.some.injEq] at hp
                subst obj; exact ha id iobj hobj c hc
              · simp only [setAggObj, hpid, ite_false] at hp
                exact ha p obj hp c hc
      refine ⟨h1, ?_⟩
      intro h xs ha
      induction xs generalizing h with
      | nil => simp only [dropIdsFuel]; exact ha
      | cons x rest ihList =>
          simp only [dropIdsFuel]
          exact ihList (dropIdFuel (f + 1) h x) (h1 h x ha)

theorem dropFuel_preserves_gt_rank (fuel : Nat) (rank : ObjId → Nat) :
    (∀ h id k,
      (∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) →
      rank id < rank k →
      dropIdFuel fuel h id k = h k) ∧
    (∀ h xs k,
      (∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) →
      (∀ x ∈ xs, rank x < rank k) →
      dropIdsFuel fuel h xs k = h k) := by
  induction fuel with
  | zero =>
      constructor
      · intro h id k _ _; simp only [dropIdFuel]
      · intro h xs k ha hlt
        induction xs generalizing h with
        | nil => simp only [dropIdsFuel]
        | cons x rest ih =>
            simp only [dropIdsFuel, dropIdFuel]
            exact ih h ha (fun y hy => hlt y (List.mem_cons_of_mem x hy))
  | succ f ih =>
      have h1 : ∀ h id k,
          (∀ p obj, h p = some obj → ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank p) →
          rank id < rank k →
          dropIdFuel (f + 1) h id k = h k := by
        intro h id k ha hlt
        have hne : k ≠ id := by intro eq; subst k; omega
        unfold dropIdFuel
        cases hobj : h id with
        | none => rfl
        | some obj =>
            by_cases hle : obj.rc ≤ 1
            · simp only [hle, ite_true]
              have ha1 : ∀ p o, setAggObj h id none p = some o →
                  ∀ c ∈ slotBoxedLeaves o.slot, rank c < rank p := by
                intro p o hp
                by_cases hpid : p = id
                · subst p; simp [setAggObj] at hp
                · simp only [setAggObj, hpid, ite_false] at hp
                  exact ha p o hp
              have hch : ∀ c ∈ slotBoxedLeaves obj.slot, rank c < rank k := fun c hc =>
                Nat.lt_trans (ha id obj hobj c hc) hlt
              rw [ih.2 (setAggObj h id none) (slotBoxedLeaves obj.slot) k ha1 hch]
              simp [setAggObj, hne]
            · simp only [hle, ite_false, setAggObj, hne]
      refine ⟨h1, ?_⟩
      intro h xs k ha hlt
      induction xs generalizing h with
      | nil => simp only [dropIdsFuel]
      | cons x rest ihList =>
          simp only [dropIdsFuel]
          rw [ihList (dropIdFuel (f + 1) h x)
            ((dropFuel_acyclic (f + 1) rank).1 h x ha)
            (fun y hy => hlt y (List.mem_cons_of_mem x hy)),
            h1 h x k ha (hlt x (List.mem_cons_self ..))]

theorem dropFuel_none_mono (fuel : Nat) :
    (∀ h id k, h k = none → dropIdFuel fuel h id k = none) ∧
    (∀ h xs k, h k = none → dropIdsFuel fuel h xs k = none) := by
  induction fuel with
  | zero =>
      constructor
      · intro h id k hk; simpa [dropIdFuel] using hk
      · intro h xs k hk
        induction xs generalizing h with
        | nil => simpa [dropIdsFuel] using hk
        | cons x rest ih => simp only [dropIdsFuel, dropIdFuel]; exact ih h hk
  | succ f ih =>
      have h1 : ∀ h id k, h k = none → dropIdFuel (f + 1) h id k = none := by
        intro h id k hk
        unfold dropIdFuel
        cases hobj : h id with
        | none => exact hk
        | some obj =>
            by_cases hle : obj.rc ≤ 1
            · simp only [hle, ite_true]
              apply ih.2
              by_cases hkid : k = id
              · subst k; simp [setAggObj]
              · simp [setAggObj, hkid, hk]
            · simp only [hle, ite_false]
              by_cases hkid : k = id
              · subst k; rw [hk] at hobj; cases hobj
              · simp [setAggObj, hkid, hk]
      refine ⟨h1, ?_⟩
      intro h xs k hk
      induction xs generalizing h with
      | nil => simp only [dropIdsFuel]; exact hk
      | cons x rest ihList =>
          simp only [dropIdsFuel]
          exact ihList (dropIdFuel (f + 1) h x) (h1 h x k hk)

end Flick.Memory.Soundness
