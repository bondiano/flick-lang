import Formal.MemorySoundness.Exec.EvalDef

/-!
# Shared helpers for single-operation operational soundness (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMemOp_defSame {state afterPass : Pass.State width} (op : MemOp width)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp op))) state = some afterPass) :
    afterPass.defined = state.defined := by
  have hdefEq : ∀ (evs : List (Event width)) (s1 s2 : Pass.State width),
      (∀ e ∈ evs, ∀ n, e ≠ .bind n) →
      runEvents evs s1 = some s2 → s2.defined = s1.defined := by
    intro evs
    induction evs with
    | nil => intro s1 s2 _ hr; simp [runEvents] at hr; subst s2; rfl
    | cons e rest ih =>
        intro s1 s2 hnb hr
        simp only [runEvents] at hr
        cases hs : Pass.step e s1 with
        | none => simp [hs] at hr
        | some mid =>
            simp only [hs, Option.bind_some] at hr
            have heq : mid.defined = s1.defined := by
              cases e with
              | bind n => exact (hnb (.bind n) (List.mem_cons_self ..) n rfl).elim
              | take n | dup n | borrow n =>
                  simp only [Pass.step] at hs
                  split at hs
                  · contradiction
                  · simp only [Option.some.injEq] at hs; subst mid; rfl
            rw [ih mid s2 (fun e' he' => hnb e' (List.mem_cons_of_mem e he')) hr, heq]
  have hnb : ∀ e ∈ operationEvents (eraseOp (toRepOp op)), ∀ n, e ≠ .bind n := by
    intro e he n
    rw [operationEvents_projection] at he
    simp only [List.mem_append, List.mem_map] at he
    rcases he with ⟨_, _, rfl⟩ | ⟨_, _, rfl⟩
    · intro h; cases h
    · intro h; cases h
  exact hdefEq _ state afterPass hnb hrunOp

theorem evalMemOp_tokMono {state afterPass : Pass.State width} (op : MemOp width)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp op))) state = some afterPass) :
    ∀ x, afterPass.tokens x ≤ state.tokens x := by
  intro x
  have hle : ∀ (evs : List (Event width)) (s1 s2 : Pass.State width),
      (∀ e ∈ evs, (∃ n, e = .take n) ∨ (∃ n, e = .borrow n)) →
      runEvents evs s1 = some s2 → s2.tokens x ≤ s1.tokens x := by
    intro evs
    induction evs with
    | nil => intro s1 s2 _ hr; simp [runEvents] at hr; subst s2; exact Nat.le_refl _
    | cons e rest ih =>
        intro s1 s2 htb hr
        simp only [runEvents] at hr
        cases hs : Pass.step e s1 with
        | none => simp [hs] at hr
        | some mid =>
            simp only [hs, Option.bind_some] at hr
            have hstepLe : mid.tokens x ≤ s1.tokens x := by
              rcases htb e (List.mem_cons_self ..) with ⟨n, rfl⟩ | ⟨n, rfl⟩
              · simp only [Pass.step] at hs
                split at hs
                · contradiction
                · simp only [Option.some.injEq] at hs; subst mid
                  by_cases hxn : x = n
                  · subst x; simp [update]
                  · have hbeq : (x == n) = false := beq_false_of_ne hxn
                    simp [update, hbeq]
              · simp only [Pass.step] at hs
                split at hs
                · contradiction
                · simp only [Option.some.injEq] at hs; subst mid; exact Nat.le_refl _
            exact Nat.le_trans (ih mid s2 (fun e' he' => htb e' (List.mem_cons_of_mem e he')) hr) hstepLe
  have htb : ∀ e ∈ operationEvents (eraseOp (toRepOp op)),
      (∃ n, e = .take n) ∨ (∃ n, e = .borrow n) := by
    intro e he
    rw [operationEvents_projection] at he
    simp only [List.mem_append, List.mem_map] at he
    rcases he with ⟨n, _, rfl⟩ | ⟨n, _, rfl⟩
    · exact Or.inr ⟨n, rfl⟩
    · exact Or.inl ⟨n, rfl⟩
  exact hle _ state afterPass htb hrunOp

theorem evalMemOp_hbZero {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width} (b : Local width) (op : MemOp width)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp op))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (henv : EnvTokensOk env state) :
    afterPass.tokens b = 0 := by
  have hdefSame := evalMemOp_defSame op hrunOp
  have htokMono := evalMemOp_tokMono op hrunOp
  have hdef0 : state.defined b = false := by rw [← hdefSame]; exact hfreshB
  have htok0 : state.tokens b = 0 := henv.undefZero b hdef0
  have := htokMono b
  omega

theorem evalMemOp_finish {nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (op : MemOp width)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp op))) state = some afterPass)
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    {rank' : ObjId → Nat} {sv : SemVal} (excl : Option ObjId)
    (hsrc : evalSourceOp op senv = some sv)
    (hb' : HeapBalanced nextId' h' rank' (bound + 1)
      (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id))
    (hexcl : ∀ c, excl = some c → heapInternalRefs nextId h c = 0 ∧
      frameRefs env afterPass.tokens c = 0 ∧ otherRoots c = 0)
    (hsame : ∀ id o1 o2, some id ≠ excl → h id = some o1 → h' id = some o2 → o1.slot = o2.slot)
    (hdRes : Denotes h' res sv) :
    ∃ rank' sv,
      evalSourceOp op senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  have hdefSame := evalMemOp_defSame op hrunOp
  have htokMono := evalMemOp_tokMono op hrunOp
  refine ⟨rank', sv, hsrc, hb', ?_, ?_, ?_⟩
  · refine ⟨?_, ?_⟩
    · intro x hx
      by_cases hxb : x = b
      · subst x; exact ⟨res, by simp [update]⟩
      · have hbeq : (x == b) = false := beq_false_of_ne hxb
        simp only [update, hbeq, Bool.false_eq_true, ite_false] at hx ⊢
        have := htokMono x
        exact henv.ownedBound x (by omega)
    · intro x hx
      by_cases hxb : x = b
      · subst x; simp [update] at hx
      · have hbeq : (x == b) = false := beq_false_of_ne hxb
        simp only [update, hbeq, Bool.false_eq_true, ite_false] at hx ⊢
        rw [hdefSame] at hx
        have h0 := henv.undefZero x hx
        have := htokMono x
        omega
  · intro x hx
    by_cases hxb : x = b
    · subst x
      exact ⟨res, sv, by simp [update], by simp [update], hdRes⟩
    · have hbeq : (x == b) = false := beq_false_of_ne hxb
      simp only [update, hbeq, Bool.false_eq_true, ite_false] at hx ⊢
      have hxOrig : 0 < state.tokens x := by have := htokMono x; omega
      obtain ⟨vx, svx, hvx, hsvx, hdx⟩ := hden x hxOrig
      refine ⟨vx, svx, hvx, hsvx, ?_⟩
      apply denotes_preserve_of_balanced hb hb' excl (fun c hc => (hexcl c hc).1) hsame hdx
      intro id hid
      refine ⟨?_, ?_⟩
      · intro heq
        have hz := (hexcl id heq.symm).2.1
        have hle := frameRefs_tok_le env afterPass.tokens x vx hvx hx id
        have hpos : 0 < boxedTokens vx id := List.count_pos_iff.mpr hid
        omega
      · have hrc := hb'.conserve id
        have hfr := frameRefs_tok_le (update env b (some res)) (update afterPass.tokens b 1)
          x vx (by simp [update, hbeq, hvx]) (by simp [update, hbeq, hx]) id
        have hpos : 0 < boxedTokens vx id := List.count_pos_iff.mpr hid
        omega
  · apply denotesList_preserve_ext hb hb' excl (fun c hc => (hexcl c hc).1) hsame hext
    intro id hid
    refine ⟨?_, ?_⟩
    · intro heq
      have hz := (hexcl id heq.symm).2.2
      have hle := hextRoots id
      have hpos : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
      omega
    · have hrc := hb'.conserve id
      have hpos : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
      have hle := hextRoots id
      omega

end Flick.Memory.Soundness
