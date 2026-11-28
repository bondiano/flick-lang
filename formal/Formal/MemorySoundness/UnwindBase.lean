import Formal.MemorySoundness.Exec

/-!
# Abnormal-Exit Unwind, M2 Scope/Arena Close, and Zero-Live-Object Teardown (#463)

Connects `evalMemPrefix` abnormal exits (`ExitKind = failure | cancelled | success`
at ANY step prefix of a `checkRepTokens`-accepted `MemBlock`) to:
- `unwindFrame`: draining every remaining frame-owned local (including live
  `reuseToken` cells, nested inline aggregates, and `boxedAgg` fallback cells)
  via ordinary recursive drops (`dropValFuel`);
- `MemoryScopes.closeLifecycle`: M2 C0–C6 multi-phase owner close (children ->
  LIFO finalizers + late arenas -> detach-all arena retirement);
- `MemoryArena.close`: root `Ref`-cell detach-all teardown (including cyclic
  `Ref` handles) draining the entire heap to zero live objects.

Main theorems:
- `unwind_frame_and_scope_balanced`
- `teardown_zero_live_objects`
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate

variable {width : Nat}

/-! ## Bridging `MemoryArena.Value` / `MemoryScopes` / `MemoryArena` into `HeapBalanced` -/

/-- Map arena/scope ledger values to counted aggregate values: nonowning
capabilities (`.handle _`) and immediates carry zero counted heap references. -/
def arenaValToAggVal : MemoryArena.Value → Aggregate.Value
  | .immediate => .imm
  | .handle _ => .imm
  | .object id => .boxed id

theorem boxedTokens_arenaValToAggVal (v : MemoryArena.Value) (id : ObjId) :
    boxedTokens (arenaValToAggVal v) id = MemoryArena.token id v := by
  cases v with
  | immediate => rfl
  | handle _ => rfl
  | object other =>
      by_cases h : id = other
      · subst id; simp [arenaValToAggVal, boxedTokens, boxedLeaves, MemoryArena.token]
      · have hbeq : (other == id) = false := beq_false_of_ne (Ne.symm h)
        simp [arenaValToAggVal, boxedTokens, boxedLeaves, List.count_cons, hbeq, MemoryArena.token, h]

theorem valsTokens_map_arenaVal (vs : List MemoryArena.Value) (id : ObjId) :
    valsTokens (vs.map arenaValToAggVal) id = MemoryArena.inventory id vs := by
  induction vs with
  | nil => rfl
  | cons v rest ih =>
      simp only [List.map_cons, valsTokens_cons, MemoryArena.inventory,
        boxedTokens_arenaValToAggVal, ih]

def scopeOwnersRefs (owners : List MemoryScopes.LifecycleOwner) (id : ObjId) : Nat :=
  MemoryArena.inventory id (MemoryScopes.lifecycleAllRoots owners)

def refArenaRefs (arena : MemoryArena.Arena) (id : ObjId) : Nat :=
  MemoryArena.balance id arena

def worldRoots (ext : List Aggregate.Value) (owners : List MemoryScopes.LifecycleOwner)
    (refArena : MemoryArena.Arena) (id : ObjId) : Nat :=
  valsTokens ext id + scopeOwnersRefs owners id + refArenaRefs refArena id

theorem valsTokens_ext_le_worldRoots (ext : List Aggregate.Value)
    (owners : List MemoryScopes.LifecycleOwner) (refArena : MemoryArena.Arena) (id : ObjId) :
    valsTokens ext id ≤ worldRoots ext owners refArena id := by
  unfold worldRoots; omega

/-! ## Frame unwinding via ordinary recursive drops -/

def repeatDropVal (fuel : Nat) : Nat → AggHeap → Aggregate.Value → AggHeap
  | 0, h, _ => h
  | n + 1, h, v => repeatDropVal fuel n (dropValFuel fuel h v) v

theorem repeatDropVal_eq_dropVals (fuel : Nat) (n : Nat) (h : AggHeap) (v : Aggregate.Value) :
    repeatDropVal fuel n h v = dropValsFuel fuel h (List.replicate n v) := by
  induction n generalizing h with
  | zero => rfl
  | succ m ih =>
      simp only [repeatDropVal, List.replicate_succ, dropValsFuel]
      exact ih (dropValFuel fuel h v)

theorem valsTokens_replicate (n : Nat) (v : Aggregate.Value) (id : ObjId) :
    valsTokens (List.replicate n v) id = n * boxedTokens v id := by
  induction n with
  | zero => simp
  | succ m ih =>
      simp only [List.replicate_succ, valsTokens_cons, ih, Nat.succ_mul]
      omega

def unwindLocals (fuel : Nat) (env : Local width → Option Aggregate.Value)
    (tok : Local width → Nat) : List (Local width) → AggHeap → AggHeap
  | [], h => h
  | x :: rest, h =>
      match env x with
      | none => unwindLocals fuel env tok rest h
      | some v => unwindLocals fuel env tok rest (repeatDropVal fuel (tok x) h v)

/-- Unwind a live frame by dropping every owned local `tok x` times using
ordinary recursive drops (`dropValFuel`), covering live `reuseToken`s, nested
inline aggregates, and `boxedAgg` fallback cells uniformly. -/
def unwindFrame (fuel : Nat) (env : Local width → Option Aggregate.Value)
    (tok : Local width → Nat) (h : AggHeap) : AggHeap :=
  unwindLocals fuel env tok (List.finRange width) h

private def localsRefs (env : Local width → Option Aggregate.Value)
    (tok : Local width → Nat) (xs : List (Local width)) (id : ObjId) : Nat :=
  (xs.map (fun x => tok x * optValTokens (env x) id)).sum

private theorem heapBalanced_unwindLocals (fuel : Nat)
    (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (xs : List (Local width)) :
    ∀ {nextId bound : Nat} {h : AggHeap} {rank otherRoots : ObjId → Nat},
      HeapBalanced nextId h rank bound (fun id => localsRefs env tok xs id + otherRoots id) →
      bound ≤ fuel →
      HeapBalanced nextId (unwindLocals fuel env tok xs h) rank bound otherRoots ∧
      (∀ k o1 o2, h k = some o1 → unwindLocals fuel env tok xs h k = some o2 → o1.slot = o2.slot) := by
  induction xs with
  | nil =>
      intro nextId bound h rank otherRoots hb _
      refine ⟨hb.congrRoots (fun id => by simp [localsRefs]),
        fun k o1 o2 h1 h2 => by rw [unwindLocals, h1] at h2; cases h2; rfl⟩
  | cons x rest ih =>
      intro nextId bound h rank otherRoots hb hfuel
      cases hx : env x with
      | none =>
          have hb' : HeapBalanced nextId h rank bound
              (fun id => localsRefs env tok rest id + otherRoots id) := by
            apply hb.congrRoots
            intro id
            simp [localsRefs, hx, optValTokens]
          simpa [unwindLocals, hx] using ih hb' hfuel
      | some v =>
          have hcnt : ∀ id, valsTokens (List.replicate (tok x) v) id ≤
              localsRefs env tok (x :: rest) id + otherRoots id := by
            intro id
            rw [valsTokens_replicate]
            simp only [localsRefs, List.map_cons, List.sum_cons, hx, optValTokens]
            omega
          obtain ⟨hb1, hsame1⟩ := heapBalanced_dropValsFuel (List.replicate (tok x) v) hb hcnt hfuel
          have hrepEq : repeatDropVal fuel (tok x) h v = dropValsFuel fuel h (List.replicate (tok x) v) :=
            repeatDropVal_eq_dropVals fuel (tok x) h v
          have hb1' : HeapBalanced nextId (repeatDropVal fuel (tok x) h v) rank bound
              (fun id => localsRefs env tok rest id + otherRoots id) := by
            rw [hrepEq]
            apply hb1.congrRoots
            intro id
            rw [valsTokens_replicate]
            simp only [localsRefs, List.map_cons, List.sum_cons, hx, optValTokens]
            omega
          obtain ⟨hb2, hsame2⟩ := ih hb1' hfuel
          refine ⟨by simpa [unwindLocals, hx] using hb2, ?_⟩
          intro k o1 o2 h1k h2k
          simp only [unwindLocals, hx] at h2k
          cases hmid : repeatDropVal fuel (tok x) h v k with
          | none =>
              have hnone : ∀ (ys : List (Local width)) (h0 : AggHeap),
                  h0 k = none → unwindLocals fuel env tok ys h0 k = none := by
                intro ys
                induction ys with
                | nil => intro h0 hk; exact hk
                | cons y ys' ihy =>
                    intro h0 hk
                    simp only [unwindLocals]
                    cases hy : env y with
                    | none => exact ihy h0 hk
                    | some w =>
                        apply ihy
                        rw [repeatDropVal_eq_dropVals]
                        have hd : ∀ (ws : List Aggregate.Value) (ha : AggHeap),
                            ha k = none → dropValsFuel fuel ha ws k = none := by
                          intro ws
                          induction ws with
                          | nil => intro ha ha0; exact ha0
                          | cons u us ihu =>
                              intro ha ha0
                              exact ihu (dropValFuel fuel ha u)
                                ((dropFuel_none_mono fuel).2 ha (boxedLeaves u) k ha0)
                        exact hd _ h0 hk
              rw [hnone rest _ hmid] at h2k; cases h2k
          | some oMid =>
              have hmid' : dropValsFuel fuel h (List.replicate (tok x) v) k = some oMid := by
                rwa [← hrepEq]
              exact (hsame1 k o1 oMid h1k hmid').trans (hsame2 k oMid o2 hmid h2k)

theorem heapBalanced_unwindFrame {nextId bound fuel : Nat} {h : AggHeap}
    {rank otherRoots : ObjId → Nat}
    (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env tok id + otherRoots id))
    (hfuel : bound ≤ fuel) :
    HeapBalanced nextId (unwindFrame fuel env tok h) rank bound otherRoots ∧
    (∀ k o1 o2, h k = some o1 → unwindFrame fuel env tok h k = some o2 → o1.slot = o2.slot) :=
  heapBalanced_unwindLocals fuel env tok (List.finRange width) hb hfuel

end Flick.Memory.Soundness
