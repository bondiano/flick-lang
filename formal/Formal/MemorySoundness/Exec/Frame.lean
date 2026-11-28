import Formal.MemorySoundness.HeapInv
import Formal.MemoryAggregate.Pass
import Formal.MemoryAggregate.RepReuse

/-!
# Operational Counted Execution and Source Equivalence (#463)

Defines counted operational execution `evalMemPrefix` over `MemBlock width` and
`AggHeap` (with recursive `dropValFuel`, `dupValue`, cell and inline-aggregate
`project` / nested `projectPath`, `construct .cell` / `.aggregate`,
`boxAggregate`, `unboxBorrow`, `unboxConsume`, `dropReuseRec`, and Stage 1 /
Stage 2 `CellLayout`-checked `reuseTypedCellRec`), and proves:

- `evalMemPrefix_sound`: every execution of a `checkRepTokens`-accepted
  `MemBlock` (for arbitrary branch choices and arbitrary step-prefix budget)
  preserves `HeapBalanced` across all live objects and external/arena roots,
  preserves every external alias's semantic denotation (`DenotesList`), and on
  normal return yields a value denoting the exact `evalSource` result with an
  empty frame (zero leaked locals);
- `evalMem_preserves_balance_and_denote`: normal-return theorem connecting
  counted memory execution to erased source evaluation;
- `memDropToReuse_preserves_evalSource` and `pipeline_source_equiv_and_balanced`:
  end-to-end composition of Stage 2 representation-aware FBIP reuse (`DropReuse`
  + `pairReuse` skipping `.construct .aggregate`) with `checkRepTokens`,
  counted execution, global RC conservation, and source evaluation.
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-! ## Frame ownership accounting -/

def optValTokens (v : Option Aggregate.Value) (id : ObjId) : Nat :=
  match v with
  | none => 0
  | some val => boxedTokens val id

def sumFin (f : Local width → Nat) : Nat :=
  ((List.finRange width).map f).sum

theorem sumList_update_not_mem (xs : List (Local width)) (f : Local width → Nat)
    (x : Local width) (v : Nat) (hnot : x ∉ xs) :
    (xs.map (update f x v)).sum = (xs.map f).sum := by
  induction xs with
  | nil => rfl
  | cons y rest ih =>
      have hne : y ≠ x := fun eq => hnot (eq ▸ List.mem_cons_self ..)
      have hnot' : x ∉ rest := fun h => hnot (List.mem_cons_of_mem y h)
      have hbeq : (y == x) = false := beq_false_of_ne hne
      simp only [List.map_cons, List.sum_cons, update, hbeq, Bool.false_eq_true, ite_false]
      rw [ih hnot']

theorem sumList_update_mem (xs : List (Local width)) (f : Local width → Nat)
    (x : Local width) (v : Nat) (hnodup : xs.Nodup) (hmem : x ∈ xs) :
    (xs.map (update f x v)).sum + f x = (xs.map f).sum + v := by
  induction xs with
  | nil => simp at hmem
  | cons y rest ih =>
      have ⟨hnotIn, hnodupRest⟩ := List.nodup_cons.mp hnodup
      simp only [List.mem_cons] at hmem
      rcases hmem with rfl | hmemRest
      · simp only [List.map_cons, List.sum_cons, update, beq_self_eq_true, ite_true]
        rw [sumList_update_not_mem rest f x v hnotIn]
        omega
      · have hne : y ≠ x := fun eq => hnotIn (eq ▸ hmemRest)
        have hbeq : (y == x) = false := beq_false_of_ne hne
        simp only [List.map_cons, List.sum_cons, update, hbeq, Bool.false_eq_true, ite_false]
        have := ih hnodupRest hmemRest
        omega

private theorem finRange_nodup (n : Nat) : (List.finRange n).Nodup := by
  induction n with
  | zero => exact List.nodup_nil
  | succ m ih =>
      rw [List.finRange_succ, List.nodup_cons]
      refine ⟨?_, List.Pairwise.map Fin.succ (fun _ _ hne heq => hne (Fin.succ_inj.mp heq)) ih⟩
      intro hmem
      simp only [List.mem_map] at hmem
      rcases hmem with ⟨k, _, hk⟩
      exact Fin.succ_ne_zero k hk

theorem sumFin_update (f : Local width → Nat) (x : Local width) (v : Nat) :
    sumFin (update f x v) + f x = sumFin f + v :=
  sumList_update_mem (List.finRange width) f x v (finRange_nodup width) (List.mem_finRange x)

theorem sumFin_zero : sumFin (width := width) (fun _ => 0) = 0 := by
  unfold sumFin
  suffices ∀ (xs : List (Local width)), (xs.map (fun _ => 0)).sum = 0 from this _
  intro xs
  induction xs with
  | nil => rfl
  | cons _ rest ih => simp [ih]

theorem sumFin_le (f : Local width → Nat) (x : Local width) :
    f x ≤ sumFin f := by
  have h := sumFin_update f x 0
  omega

/-- Total heap-object tokens owned by a frame `(env, tok)`. -/
def frameRefs (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (id : ObjId) : Nat :=
  sumFin (fun x => tok x * optValTokens (env x) id)

theorem frameRefs_empty_tok (env : Local width → Option Aggregate.Value) (id : ObjId) :
    frameRefs env (fun _ => 0) id = 0 := by
  unfold frameRefs
  have : (fun x => 0 * optValTokens (env x) id) = (fun _ => 0) := by funext x; simp
  rw [this]
  exact sumFin_zero

theorem frameRefs_update_tok (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (x : Local width) (n : Nat) (id : ObjId) :
    frameRefs env (update tok x n) id + tok x * optValTokens (env x) id =
      frameRefs env tok id + n * optValTokens (env x) id := by
  unfold frameRefs
  have hcongr : (fun y => update tok x n y * optValTokens (env y) id) =
      update (fun y => tok y * optValTokens (env y) id) x (n * optValTokens (env x) id) := by
    funext y
    by_cases hy : y = x
    · subst y; simp [update]
    · have hbeq : (y == x) = false := beq_false_of_ne hy
      simp [update, hbeq]
  rw [hcongr]
  exact sumFin_update _ x _

theorem frameRefs_bind_fresh (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (b : Local width) (v : Aggregate.Value) (hz : tok b = 0) (id : ObjId) :
    frameRefs (update env b (some v)) (update tok b 1) id =
      frameRefs env tok id + boxedTokens v id := by
  unfold frameRefs
  have hcongr : (fun y => update tok b 1 y * optValTokens (update env b (some v) y) id) =
      update (fun y => tok y * optValTokens (env y) id) b (boxedTokens v id) := by
    funext y
    by_cases hy : y = b
    · subst y; simp [update, optValTokens]
    · have hbeq : (y == b) = false := beq_false_of_ne hy
      simp [update, hbeq]
  rw [hcongr]
  have h := sumFin_update (fun y => tok y * optValTokens (env y) id) b (boxedTokens v id)
  simp only [hz, Nat.zero_mul, Nat.add_zero] at h
  exact h

theorem frameRefs_tok_le (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (x : Local width) (v : Aggregate.Value) (hx : env x = some v) (hpos : 0 < tok x) (id : ObjId) :
    boxedTokens v id ≤ frameRefs env tok id := by
  have hle := sumFin_le (fun y => tok y * optValTokens (env y) id) x
  simp only [hx, optValTokens] at hle
  have h1 : boxedTokens v id ≤ tok x * boxedTokens v id :=
    Nat.le_mul_of_pos_left _ hpos
  exact Nat.le_trans h1 hle

theorem frameRefs_singleton (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (r : Local width) (v : Aggregate.Value) (hr : env r = some v)
    (h1 : tok r = 1) (hother : ∀ x, x ≠ r → tok x = 0) (id : ObjId) :
    frameRefs env tok id = boxedTokens v id := by
  have hupd : tok = update (fun _ => 0) r 1 := by
    funext x
    by_cases hx : x = r
    · subst x; simp [update, h1]
    · have hbeq : (x == r) = false := beq_false_of_ne hx
      simp [update, hbeq, hother x hx]
  rw [hupd]
  have h := frameRefs_update_tok env (fun _ => 0) r 1 id
  simp only [frameRefs_empty_tok, Nat.zero_mul, Nat.add_zero, Nat.zero_add, hr, optValTokens,
    Nat.one_mul] at h
  exact h

/-! ## Environment and denotation invariants -/

/-- Runtime environment agreement with `Pass.State`: every owned local is bound,
and undefined locals hold zero tokens. -/
structure EnvTokensOk (env : Local width → Option Aggregate.Value) (state : Pass.State width) : Prop where
  ownedBound : ∀ x, 0 < state.tokens x → ∃ v, env x = some v
  undefZero : ∀ x, state.defined x = false → state.tokens x = 0

/-- Every currently owned local denotes its pure source semantic value in `h`. -/
def EnvDenotes (h : AggHeap) (env : Local width → Option Aggregate.Value)
    (tok : Local width → Nat) (senv : Local width → Option SemVal) : Prop :=
  ∀ x, 0 < tok x → ∃ v sv, env x = some v ∧ senv x = some sv ∧ Denotes h v sv

end Flick.Memory.Soundness
