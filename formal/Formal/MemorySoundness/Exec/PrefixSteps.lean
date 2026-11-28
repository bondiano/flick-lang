import Formal.MemorySoundness.Exec.OpSound

/-!
# Frame transition lemmas for dup and drop steps (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMem_step_dup_local {nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state : RepState width} {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (x : Local width) (vx : Aggregate.Value)
    (hxPos : 0 < state.tokens x)
    (hvx : env x = some vx)
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state.toPassState)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id) :
    let st' : RepState width :=
      ⟨update state.tokens x (state.tokens x + 1), state.defined, state.isAgg⟩
    HeapBalanced nextId (dupValue h vx) rank (bound + 1)
      (fun id => frameRefs env st'.tokens id + otherRoots id) ∧
    EnvTokensOk env st'.toPassState ∧
    EnvDenotes (dupValue h vx) env st'.tokens senv ∧
    DenotesList (dupValue h vx) ext sext := by
  intro st'
  have hlive : ∀ id ∈ boxedLeaves vx, ∃ obj, h id = some obj := by
    intro id hid
    have h1 : 0 < boxedTokens vx id := List.count_pos_iff.mpr hid
    have h2 := frameRefs_tok_le env state.tokens x vx hvx hxPos id
    have hposR : 0 < frameRefs env state.tokens id + otherRoots id := by omega
    obtain ⟨obj, hobj, _⟩ := hb.live_of_pos_roots hposR
    exact ⟨obj, hobj⟩
  obtain ⟨hbDup, hsameDup⟩ := heapBalanced_dupValue hb vx hlive
  have hbNext : HeapBalanced nextId (dupValue h vx) rank (bound + 1)
      (fun id => frameRefs env st'.tokens id + otherRoots id) := by
    apply (hbDup.weakenBound (Nat.le_succ bound)).congrRoots
    intro id
    have hupd := frameRefs_update_tok env state.tokens x (state.tokens x + 1) id
    simp only [hvx, optValTokens] at hupd
    have hmul : (state.tokens x + 1) * boxedTokens vx id =
        state.tokens x * boxedTokens vx id + boxedTokens vx id := by
      rw [Nat.add_mul, Nat.one_mul]
    change frameRefs env state.tokens id + otherRoots id + boxedTokens vx id =
      frameRefs env (update state.tokens x (state.tokens x + 1)) id + otherRoots id
    omega
  have henvNext : EnvTokensOk env st'.toPassState := by
    refine ⟨?_, ?_⟩
    · intro y hy
      by_cases hyx : y = x
      · subst y; exact ⟨vx, hvx⟩
      · have hbeq : (y == x) = false := beq_false_of_ne hyx
        exact henv.ownedBound y (by simpa [st', RepState.toPassState, update, hbeq] using hy)
    · intro y hy
      have h0 : state.tokens y = 0 := henv.undefZero y hy
      by_cases hyx : y = x
      · subst y; omega
      · have hbeq : (y == x) = false := beq_false_of_ne hyx
        simp [st', RepState.toPassState, update, hbeq, h0]
  have hdenNext : EnvDenotes (dupValue h vx) env st'.tokens senv := by
    intro y hy
    have hyOrig : 0 < state.tokens y := by
      by_cases hyx : y = x
      · subst y; exact hxPos
      · have hbeq : (y == x) = false := beq_false_of_ne hyx
        simpa [st', update, hbeq] using hy
    obtain ⟨vy, svy, hvy, hsvy, hdy⟩ := hden y hyOrig
    refine ⟨vy, svy, hvy, hsvy, ?_⟩
    apply denotes_preserve_of_balanced hb hbDup none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameDup id o1 o2 h1 h2) hdy
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbDup.conserve id
    have hfr := frameRefs_tok_le env state.tokens y vy hvy hyOrig id
    have h1 : 0 < boxedTokens vy id := List.count_pos_iff.mpr hid
    omega
  have hextNext : DenotesList (dupValue h vx) ext sext := by
    apply denotesList_preserve_ext hb hbDup none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameDup id o1 o2 h1 h2) hext
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbDup.conserve id
    have h1 : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
    have h2 := hextRoots id
    omega
  exact ⟨hbNext, henvNext, hdenNext, hextNext⟩

theorem evalMem_step_drop_local {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state : RepState width} {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (x : Local width) (vx : Aggregate.Value)
    (hxPos : 0 < state.tokens x)
    (hvx : env x = some vx)
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state.toPassState)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    (hfuelB : bound ≤ dropFuel) :
    let st' : RepState width :=
      ⟨update state.tokens x (state.tokens x - 1), state.defined, state.isAgg⟩
    HeapBalanced nextId (dropValFuel dropFuel h vx) rank (bound + 1)
      (fun id => frameRefs env st'.tokens id + otherRoots id) ∧
    EnvTokensOk env st'.toPassState ∧
    EnvDenotes (dropValFuel dropFuel h vx) env st'.tokens senv ∧
    DenotesList (dropValFuel dropFuel h vx) ext sext := by
  intro st'
  have hcnt : ∀ id, boxedTokens vx id ≤ frameRefs env state.tokens id + otherRoots id := by
    intro id
    have := frameRefs_tok_le env state.tokens x vx hvx hxPos id
    omega
  obtain ⟨hbDrop, hsameDrop⟩ := heapBalanced_dropValFuel vx hb hcnt hfuelB
  have hbNext : HeapBalanced nextId (dropValFuel dropFuel h vx) rank (bound + 1)
      (fun id => frameRefs env st'.tokens id + otherRoots id) := by
    apply (hbDrop.weakenBound (Nat.le_succ bound)).congrRoots
    intro id
    have hupd := frameRefs_update_tok env state.tokens x (state.tokens x - 1) id
    simp only [hvx, optValTokens] at hupd
    have hmul : (state.tokens x - 1) * boxedTokens vx id + boxedTokens vx id =
        state.tokens x * boxedTokens vx id := by
      have h1 : state.tokens x - 1 + 1 = state.tokens x := by omega
      calc (state.tokens x - 1) * boxedTokens vx id + boxedTokens vx id
          = (state.tokens x - 1) * boxedTokens vx id + 1 * boxedTokens vx id := by simp
        _ = (state.tokens x - 1 + 1) * boxedTokens vx id := (Nat.add_mul _ _ _).symm
        _ = state.tokens x * boxedTokens vx id := by rw [h1]
    change frameRefs env state.tokens id + otherRoots id - boxedTokens vx id =
      frameRefs env (update state.tokens x (state.tokens x - 1)) id + otherRoots id
    omega
  have henvNext : EnvTokensOk env st'.toPassState := by
    refine ⟨?_, ?_⟩
    · intro y hy
      by_cases hyx : y = x
      · subst y; exact ⟨vx, hvx⟩
      · have hbeq : (y == x) = false := beq_false_of_ne hyx
        exact henv.ownedBound y (by simpa [st', RepState.toPassState, update, hbeq] using hy)
    · intro y hy
      have h0 : state.tokens y = 0 := henv.undefZero y hy
      by_cases hyx : y = x
      · subst y; omega
      · have hbeq : (y == x) = false := beq_false_of_ne hyx
        simp [st', RepState.toPassState, update, hbeq, h0]
  have hdenNext : EnvDenotes (dropValFuel dropFuel h vx) env st'.tokens senv := by
    intro y hy
    have hyOrig : 0 < state.tokens y := by
      by_cases hyx : y = x
      · subst y; exact hxPos
      · have hbeq : (y == x) = false := beq_false_of_ne hyx
        simpa [st', update, hbeq] using hy
    obtain ⟨vy, svy, hvy, hsvy, hdy⟩ := hden y hyOrig
    refine ⟨vy, svy, hvy, hsvy, ?_⟩
    apply denotes_preserve_of_balanced hb hbNext none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameDrop id o1 o2 h1 h2) hdy
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbNext.conserve id
    have hfr := frameRefs_tok_le env st'.tokens y vy hvy hy id
    have h1 : 0 < boxedTokens vy id := List.count_pos_iff.mpr hid
    omega
  have hextNext : DenotesList (dropValFuel dropFuel h vx) ext sext := by
    apply denotesList_preserve_ext hb hbNext none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameDrop id o1 o2 h1 h2) hext
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbNext.conserve id
    have h1 : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
    have h2 := hextRoots id
    omega
  exact ⟨hbNext, henvNext, hdenNext, hextNext⟩

end Flick.Memory.Soundness
