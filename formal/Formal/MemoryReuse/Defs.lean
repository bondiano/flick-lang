import Formal.MemoryPass

/-!
# Frame-limited FBIP reuse: syntax, pairing, and event-state lemmas (phase M3)

Defines `TargetKind`, `pairReuse`, `localUnused`, and the state-extension
lemmas used by `Formal.MemoryReuse.Pass` to prove whole-block token preservation.
-/

namespace Flick.Memory.Reuse

open Flick.Memory.Pass

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

inductive TargetKind where
  | tagged
  | record
  deriving DecidableEq, Repr

def matchesAlloc (kind : TargetKind) : Operation width → Bool
  | .construct .. => kind == .tagged
  | .record .. => kind == .record
  | _ => false

def rewriteAlloc (r : Local width) : Operation width → Operation width
  | .construct fields evidence => .reuse r fields evidence
  | .record fields => .reuseRecord r fields
  | op => op

/-- Checks whether a block contains any compatible allocation reachable on some branch. -/
def hasCompatibleAlloc (kind : TargetKind) : Block width → Bool
  | .ret _ => false
  | .seq (.let_ _ op) next => matchesAlloc kind op || hasCompatibleAlloc kind next
  | .seq _ next => hasCompatibleAlloc kind next
  | .branch _ yes no => hasCompatibleAlloc kind yes || hasCompatibleAlloc kind no

/-- Frame-limited reuse pairing along every control-flow path of a block.
Matches `flick-core/src/opt/perceus/reuse.rs::pair_reuse_block`. -/
def pairReuse (r : Local width) (kind : TargetKind) : Block width → Block width
  | .ret result => .seq (.drop r) (.ret result)
  | .seq (.let_ binding op) next =>
      if matchesAlloc kind op then
        .seq (.let_ binding (rewriteAlloc r op)) next
      else
        .seq (.let_ binding op) (pairReuse r kind next)
  | .seq inst next => .seq inst (pairReuse r kind next)
  | .branch cond yes no =>
      let yesHas := hasCompatibleAlloc kind yes
      let noHas := hasCompatibleAlloc kind no
      if yesHas || noHas then
        .branch cond
          (if yesHas then pairReuse r kind yes else .seq (.drop r) yes)
          (if noHas then pairReuse r kind no else .seq (.drop r) no)
      else
        .seq (.drop r) (.branch cond yes no)

def eventLocal : Event width → Local width
  | .take n | .dup n | .borrow n | .bind n => n

def localUnusedInOp (r : Local width) (op : Operation width) : Bool :=
  !(operands op).contains r

def localUnusedInInst (r : Local width) : Instruction width → Bool
  | .let_ b op => (b != r) && localUnusedInOp r op
  | .dup n => n != r
  | .drop n => n != r

def localUnused (r : Local width) : Block width → Bool
  | .ret v => v != r
  | .seq inst next => localUnusedInInst r inst && localUnused r next
  | .branch c yes no => (c != r) && localUnused r yes && localUnused r no

def withDefined (state : State width) (r : Local width) : State width :=
  { state with defined := update state.defined r true }

def withFreshToken (state : State width) (r : Local width) : State width :=
  { tokens := update state.tokens r 1, defined := update state.defined r true }

theorem state_ext (a b : State width)
    (ht : ∀ n, a.tokens n = b.tokens n) (hd : ∀ n, a.defined n = b.defined n) : a = b := by
  cases a; cases b
  have h1 := funext ht
  have h2 := funext hd
  subst h1; subst h2; rfl

theorem step_withDefined (e : Event width) (r : Local width)
    (hne : eventLocal e ≠ r) (state next : State width)
    (h : Pass.step e state = some next) :
    Pass.step e (withDefined state r) = some (withDefined next r) := by
  cases e with
  | take n =>
      simp only [Pass.step, withDefined] at h ⊢
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h
        subst next
        simp_all
  | dup n =>
      simp only [Pass.step, withDefined] at h ⊢
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h
        subst next
        simp_all
  | borrow n =>
      simp only [Pass.step, withDefined] at h ⊢
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h
        subst next
        simp_all
  | bind n =>
      have hn : n ≠ r := hne
      simp only [Pass.step, withDefined, update, beq_iff_eq, hn, ite_false] at h ⊢
      split at h
      · contradiction
      · rename_i hnd
        simp only [hnd, Bool.false_eq_true, ite_false, Option.some.injEq] at h ⊢
        subst next
        apply state_ext
        · intro k; rfl
        · intro k
          by_cases hk1 : k = n <;> by_cases hk2 : k = r <;> simp_all [update]

theorem runEvents_withDefined (events : List (Event width)) (r : Local width)
    (hne : ∀ e ∈ events, eventLocal e ≠ r) (state next : State width)
    (h : runEvents events state = some next) :
    runEvents events (withDefined state r) = some (withDefined next r) := by
  induction events generalizing state with
  | nil =>
      simp only [runEvents, Option.some.injEq] at h ⊢
      subst h; rfl
  | cons e rest ih =>
      simp only [runEvents] at h ⊢
      cases hs : Pass.step e state with
      | none => simp [hs] at h
      | some mid =>
          simp only [hs, Option.bind_some] at h
          have he := step_withDefined e r (hne e (List.mem_cons_self ..)) state mid hs
          rw [he, Option.bind_some]
          exact ih (fun x hx => hne x (List.mem_cons_of_mem e hx)) mid h

theorem step_withFreshToken (e : Event width) (r : Local width)
    (hne : eventLocal e ≠ r) (state next : State width)
    (h : Pass.step e state = some next) :
    Pass.step e (withFreshToken state r) = some (withFreshToken next r) := by
  cases e with
  | take n =>
      have hn : n ≠ r := hne
      simp only [Pass.step, withFreshToken, update, beq_iff_eq, hn, ite_false] at h ⊢
      split at h
      · contradiction
      · rename_i hnz
        simp only [hnz, ite_false, Option.some.injEq] at h ⊢
        subst next
        apply state_ext
        · intro k
          by_cases hk1 : k = n <;> by_cases hk2 : k = r <;> simp_all [update]
        · intro k; rfl
  | dup n =>
      have hn : n ≠ r := hne
      simp only [Pass.step, withFreshToken, update, beq_iff_eq, hn, ite_false] at h ⊢
      split at h
      · contradiction
      · rename_i hnz
        simp only [hnz, ite_false, Option.some.injEq] at h ⊢
        subst next
        apply state_ext
        · intro k
          by_cases hk1 : k = n <;> by_cases hk2 : k = r <;> simp_all [update]
        · intro k; rfl
  | borrow n =>
      have hn : n ≠ r := hne
      simp only [Pass.step, withFreshToken, update, beq_iff_eq, hn, ite_false] at h ⊢
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h
        subst next
        simp_all
  | bind n =>
      have hn : n ≠ r := hne
      simp only [Pass.step, withFreshToken, update, beq_iff_eq, hn, ite_false] at h ⊢
      split at h
      · contradiction
      · rename_i hnd
        simp only [hnd, Bool.false_eq_true, ite_false, Option.some.injEq] at h ⊢
        subst next
        apply state_ext
        · intro k
          by_cases hk1 : k = n <;> by_cases hk2 : k = r <;> simp_all [update]
        · intro k
          by_cases hk1 : k = n <;> by_cases hk2 : k = r <;> simp_all [update]

theorem runEvents_withFreshToken (events : List (Event width)) (r : Local width)
    (hne : ∀ e ∈ events, eventLocal e ≠ r) (state next : State width)
    (h : runEvents events state = some next) :
    runEvents events (withFreshToken state r) = some (withFreshToken next r) := by
  induction events generalizing state with
  | nil =>
      simp only [runEvents, Option.some.injEq] at h ⊢
      subst h; rfl
  | cons e rest ih =>
      simp only [runEvents] at h ⊢
      cases hs : Pass.step e state with
      | none => simp [hs] at h
      | some mid =>
          simp only [hs, Option.bind_some] at h
          have he := step_withFreshToken e r (hne e (List.mem_cons_self ..)) state mid hs
          rw [he, Option.bind_some]
          exact ih (fun x hx => hne x (List.mem_cons_of_mem e hx)) mid h

theorem instEvents_unused (inst : Instruction width) (r : Local width)
    (h : localUnusedInInst r inst = true) :
    ∀ e ∈ instructionEvents inst, eventLocal e ≠ r := by
  cases inst with
  | dup n =>
      intro e he
      simp only [instructionEvents, List.mem_singleton] at he
      subst he
      simpa [localUnusedInInst, eventLocal] using h
  | drop n =>
      intro e he
      simp only [instructionEvents, List.mem_singleton] at he
      subst he
      simpa [localUnusedInInst, eventLocal] using h
  | let_ b op =>
      simp only [localUnusedInInst, localUnusedInOp, Bool.and_eq_true, bne_iff_ne,
        Bool.not_eq_true'] at h
      have hnotmem : r ∉ operands op := by
        intro hmem
        have hc : (operands op).contains r = true := List.contains_iff_mem.mpr hmem
        rw [hc] at h
        exact absurd h.2 (by decide)
      intro e he
      simp only [instructionEvents, operationEvents_projection, List.mem_append, List.mem_map,
        List.mem_singleton] at he
      rcases he with (⟨x, hx, rfl⟩ | ⟨x, hx, rfl⟩) | rfl
      · intro same; subst same
        exact hnotmem (List.mem_append_right _ hx)
      · intro same; subst same
        exact hnotmem (List.mem_append_left _ hx)
      · exact h.1

/-- Adding an unused local to `defined` preserves `checkTokens`. -/
theorem checkTokens_withDefined (b : Block width) (r : Local width)
    (unused : localUnused r b = true) :
    ∀ state, checkTokens b state = true → checkTokens b (withDefined state r) = true := by
  induction b with
  | ret v =>
      intro state h
      have hv : v ≠ r := by simpa [localUnused] using unused
      simp only [checkTokens] at h ⊢
      cases hr : runEvents [.take v] state with
      | none => simp [hr] at h
      | some after =>
          have hrun := runEvents_withDefined [.take v] r
            (by intro e he; simp at he; subst he; exact hv) state after hr
          rw [hrun]
          simpa [hr, withDefined, emptyTokens] using h
  | seq inst next ih =>
      intro state h
      simp only [localUnused, Bool.and_eq_true] at unused
      simp only [checkTokens] at h ⊢
      cases hr : runEvents (instructionEvents inst) state with
      | none => simp [hr] at h
      | some after =>
          have hrun := runEvents_withDefined (instructionEvents inst) r
            (instEvents_unused inst r unused.1) state after hr
          rw [hrun]
          exact ih unused.2 after (by simpa [hr] using h)
  | branch c yes no ihY ihN =>
      intro state h
      simp only [localUnused, Bool.and_eq_true, bne_iff_ne] at unused
      simp only [checkTokens] at h ⊢
      cases hr : runEvents [.take c] state with
      | none => simp [hr] at h
      | some after =>
          have hrun := runEvents_withDefined [.take c] r
            (by intro e he; simp at he; subst he; exact unused.1.1) state after hr
          rw [hrun]
          simp only [hr, Bool.and_eq_true] at h
          simp only [Bool.and_eq_true]
          exact ⟨ihY unused.1.2 after h.1, ihN unused.2 after h.2⟩

theorem take_freshToken (state : State width) (r : Local width)
    (hzero : state.tokens r = 0) :
    runEvents [.take r] (withFreshToken state r) = some (withDefined state r) := by
  simp only [runEvents, Pass.step, withFreshToken, update, beq_self_eq_true, ite_true]
  simp only [show ((1 : Nat) == 0) = false from rfl, Bool.false_eq_true, ite_false, Option.bind_some]
  apply congrArg some
  apply state_ext
  · intro n
    by_cases hn : n = r
    · subst n; simp [withDefined, update, hzero]
    · simp [withDefined, update, hn]
  · intro n; rfl

theorem step_preserves_zero_token (e : Event width) (r : Local width)
    (hne : eventLocal e ≠ r) (state next : State width)
    (hzero : state.tokens r = 0) (h : Pass.step e state = some next) :
    next.tokens r = 0 := by
  cases e with
  | take n =>
      have hn : r ≠ n := Ne.symm hne
      simp only [Pass.step] at h
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h; subst next; simp [update, hn, hzero]
  | dup n =>
      have hn : r ≠ n := Ne.symm hne
      simp only [Pass.step] at h
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h; subst next; simp [update, hn, hzero]
  | borrow n =>
      simp only [Pass.step] at h
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h; subst next; exact hzero
  | bind n =>
      have hn : r ≠ n := Ne.symm hne
      simp only [Pass.step] at h
      split at h
      · contradiction
      · simp only [Option.some.injEq] at h; subst next; simp [update, hn, hzero]

theorem runEvents_preserves_zero_token (events : List (Event width)) (r : Local width)
    (hne : ∀ e ∈ events, eventLocal e ≠ r) (state next : State width)
    (hzero : state.tokens r = 0) (h : runEvents events state = some next) :
    next.tokens r = 0 := by
  induction events generalizing state with
  | nil =>
      simp only [runEvents, Option.some.injEq] at h; subst h; exact hzero
  | cons e rest ih =>
      simp only [runEvents] at h
      cases hs : Pass.step e state with
      | none => simp [hs] at h
      | some mid =>
          simp only [hs, Option.bind_some] at h
          have hz := step_preserves_zero_token e r (hne e (List.mem_cons_self ..)) state mid hzero hs
          exact ih (fun x hx => hne x (List.mem_cons_of_mem e hx)) mid hz h

theorem rewriteAlloc_events (r : Local width) (kind : TargetKind) (op : Operation width)
    (hm : matchesAlloc kind op = true) :
    operationEvents (rewriteAlloc r op) = .take r :: operationEvents op := by
  cases op <;> simp [matchesAlloc, rewriteAlloc, operationEvents] at hm ⊢

end Flick.Memory.Reuse
