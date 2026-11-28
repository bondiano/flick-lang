import Formal.MemorySoundness.UnwindBase

/-!
# Abnormal-Exit Unwind, M2 Scope/Arena Close, and Zero-Live-Object Teardown (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate

variable {width : Nat}

/-! ## Connected abnormal-exit + M2 Scope/Arena unwind theorem -/

/-- End-to-end abnormal-exit unwind theorem connecting operational `MemBlock`
prefix execution (`evalMemPrefix`), frame-local draining (`unwindFrame`), and
M2 multi-phase owner close (`MemoryScopes.closeLifecycle`):

At ANY execution prefix `steps` where a `checkRepTokens`-accepted `MemBlock`
exits abnormally with `exit : ExitKind` (`failure` / `cancelled` / `success`):
1. `unwindFrame` drains every remaining local in the abandoned frame (including
   live `reuseToken` cells, nested inline aggregates, and `boxedAgg` cells)
   while preserving `HeapBalanced` and every external alias;
2. `MemoryScopes.closeLifecycle exit owner` runs `owner`'s registered finalizers
   in strict LIFO order strictly after `childrenDone exit` and strictly before
   `arenasRetired`, retiring `owner` to `.closed` with empty roots;
3. Dropping `closeRes.released` via ordinary recursive drops (accounting for
   any initial plus late-C5 finalizer arena slots) preserves `HeapBalanced` for
   all remaining external roots (`ext`), unrelated active owners
   (`before ++ after`), and `refArena`, and preserves every external alias's
   denotation `DenotesList` unchanged. -/
theorem unwind_frame_and_scope_balanced (dropFuel : Nat) (exit : MemoryScopes.ExitKind)
    (steps : Nat) (block : MemBlock width) (choices : List Bool)
    (nextId bound : Nat) (h : AggHeap) (rank : ObjId → Nat)
    (env : Local width → Option Aggregate.Value)
    (state : RepState width) (senv : Local width → Option SemVal)
    (ext : List Aggregate.Value) (sext : List SemVal)
    (before after : List MemoryScopes.LifecycleOwner)
    (owner : MemoryScopes.LifecycleOwner) (refArena : MemoryArena.Arena)
    (nextId' : Nat) (h' : AggHeap)
    (env' : Local width → Option Aggregate.Value) (tok' : Local width → Nat)
    (hopen : owner.phase ≠ .closed)
    (hnoLate : MemoryScopes.finalizersLateArenas owner.finalizers.reverse = [])
    (hchk : checkRepTokens (toRepBlock block) state = true)
    (heval : evalMemPrefix dropFuel exit steps block choices nextId h env state.tokens =
      some (.aborted exit nextId' h' env' tok'))
    (hb : HeapBalanced nextId h rank bound
      (fun id => frameRefs env state.tokens id + worldRoots ext (before ++ owner :: after) refArena id))
    (henv : EnvTokensOk env state.toPassState)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hfuel : bound + blockSteps block ≤ dropFuel) :
    let closeRes := MemoryScopes.closeLifecycle exit owner
    let hUnwound := unwindFrame dropFuel env' tok' h'
    let hFinal := dropValsFuel dropFuel hUnwound (closeRes.released.map arenaValToAggVal)
    closeRes.owner.phase = .closed ∧
    MemoryScopes.lifecycleRoots closeRes.owner = [] ∧
    closeRes.events =
      MemoryScopes.CloseEvent.childrenDone exit ::
        (owner.finalizers.reverse.map (fun f => MemoryScopes.CloseEvent.runFinalizer f.label)) ++
        [MemoryScopes.CloseEvent.arenasRetired] ∧
    ∃ rank',
      HeapBalanced nextId' hFinal rank' (bound + blockSteps block)
        (worldRoots ext (before ++ closeRes.owner :: after) refArena) ∧
      DenotesList hFinal ext sext := by
  obtain ⟨_, rank', hbAbort, hextAbort⟩ :=
    evalMemPrefix_sound dropFuel exit steps block choices nextId bound h rank
      (worldRoots ext (before ++ owner :: after) refArena) env state senv ext sext
      (.aborted exit nextId' h' env' tok') hchk heval hb henv hden hext
      (valsTokens_ext_le_worldRoots ext (before ++ owner :: after) refArena) hfuel
  obtain ⟨hbFrame, hsameFrame⟩ := heapBalanced_unwindFrame env' tok' hbAbort hfuel
  have hextFrame : DenotesList (unwindFrame dropFuel env' tok' h') ext sext := by
    apply denotesList_preserve_ext hbAbort hbFrame none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameFrame id o1 o2 h1 h2) hextAbort
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbFrame.conserve id
    have h1 : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
    have h2 := valsTokens_ext_le_worldRoots ext (before ++ owner :: after) refArena id
    omega
  let closeRes := MemoryScopes.closeLifecycle exit owner
  have hclosed := MemoryScopes.closeLifecycle_reaches_closed exit owner
  have hlifo := MemoryScopes.closeLifecycle_lifo_order exit owner hopen
  have hrelEq : ∀ id,
      worldRoots ext (before ++ owner :: after) refArena id =
      worldRoots ext (before ++ closeRes.owner :: after) refArena id +
        valsTokens (closeRes.released.map arenaValToAggVal) id := by
    intro id
    rw [valsTokens_map_arenaVal]
    have hsel := MemoryScopes.closeLifecycle_selected id exit before after owner [] hopen
    simp only [List.nil_append, hnoLate, MemoryScopes.balance, MemoryScopes.owned,
      List.flatMap_nil, MemoryArena.inventory, Nat.add_zero] at hsel
    unfold worldRoots scopeOwnersRefs
    dsimp only [closeRes]
    omega
  have hrelCnt : ∀ id, valsTokens (closeRes.released.map arenaValToAggVal) id ≤
      worldRoots ext (before ++ owner :: after) refArena id := by
    intro id; have := hrelEq id; omega
  obtain ⟨hbFinal, hsameFinal⟩ :=
    heapBalanced_dropValsFuel (closeRes.released.map arenaValToAggVal) hbFrame hrelCnt hfuel
  have hbFinal' : HeapBalanced nextId'
      (dropValsFuel dropFuel (unwindFrame dropFuel env' tok' h') (closeRes.released.map arenaValToAggVal))
      rank' (bound + blockSteps block)
      (worldRoots ext (before ++ closeRes.owner :: after) refArena) := by
    apply hbFinal.congrRoots
    intro id
    have := hrelEq id
    omega
  have hextFinal : DenotesList
      (dropValsFuel dropFuel (unwindFrame dropFuel env' tok' h') (closeRes.released.map arenaValToAggVal))
      ext sext := by
    apply denotesList_preserve_ext hbFrame hbFinal' none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameFinal id o1 o2 h1 h2) hextFrame
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbFinal'.conserve id
    have h1 : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
    have h2 := valsTokens_ext_le_worldRoots ext (before ++ closeRes.owner :: after) refArena id
    omega
  exact ⟨hclosed.1, hclosed.2.2.2.2, hlifo, rank', hbFinal', hextFinal⟩

/-- Late-finalizer-allocation companion lemma: once an active owner's finalizers
allocate late arena slots (accounted in `HeapBalanced`), closing the owner and
dropping `closeRes.released` retires BOTH initial and late-C5 arenas with exact
global RC balance and preserves all remaining external aliases. -/
theorem close_owner_with_late_arenas_balanced {nextId bound fuel : Nat} {h : AggHeap}
    {rank : ObjId → Nat} (exit : MemoryScopes.ExitKind)
    (ext : List Aggregate.Value) (sext : List SemVal)
    (before after : List MemoryScopes.LifecycleOwner)
    (owner : MemoryScopes.LifecycleOwner) (refArena : MemoryArena.Arena)
    (hopen : owner.phase ≠ .closed)
    (hb : HeapBalanced nextId h rank bound
      (fun id => worldRoots ext (before ++ owner :: after) refArena id +
        MemoryScopes.balance id (MemoryScopes.finalizersLateArenas owner.finalizers.reverse)))
    (hext : DenotesList h ext sext)
    (hfuel : bound ≤ fuel) :
    let closeRes := MemoryScopes.closeLifecycle exit owner
    let hFinal := dropValsFuel fuel h (closeRes.released.map arenaValToAggVal)
    closeRes.owner.phase = .closed ∧
    HeapBalanced nextId hFinal rank bound
      (worldRoots ext (before ++ closeRes.owner :: after) refArena) ∧
    DenotesList hFinal ext sext := by
  let closeRes := MemoryScopes.closeLifecycle exit owner
  have hclosed := MemoryScopes.closeLifecycle_reaches_closed exit owner
  have hrelEq : ∀ id,
      worldRoots ext (before ++ owner :: after) refArena id +
        MemoryScopes.balance id (MemoryScopes.finalizersLateArenas owner.finalizers.reverse) =
      worldRoots ext (before ++ closeRes.owner :: after) refArena id +
        valsTokens (closeRes.released.map arenaValToAggVal) id := by
    intro id
    rw [valsTokens_map_arenaVal]
    have hsel := MemoryScopes.closeLifecycle_selected id exit before after owner [] hopen
    simp only [List.nil_append] at hsel
    unfold worldRoots scopeOwnersRefs
    dsimp only [closeRes]
    omega
  have hrelCnt : ∀ id, valsTokens (closeRes.released.map arenaValToAggVal) id ≤
      worldRoots ext (before ++ owner :: after) refArena id +
        MemoryScopes.balance id (MemoryScopes.finalizersLateArenas owner.finalizers.reverse) := by
    intro id; have := hrelEq id; omega
  obtain ⟨hbFinal, hsameFinal⟩ :=
    heapBalanced_dropValsFuel (closeRes.released.map arenaValToAggVal) hb hrelCnt hfuel
  have hbFinal' : HeapBalanced nextId
      (dropValsFuel fuel h (closeRes.released.map arenaValToAggVal)) rank bound
      (worldRoots ext (before ++ closeRes.owner :: after) refArena) := by
    apply hbFinal.congrRoots
    intro id; have := hrelEq id; omega
  have hextFinal : DenotesList (dropValsFuel fuel h (closeRes.released.map arenaValToAggVal))
      ext sext := by
    apply denotesList_preserve_ext hb hbFinal' none (fun _ hc => by cases hc)
      (fun id o1 o2 _ h1 h2 => hsameFinal id o1 o2 h1 h2) hext
    intro id hid
    refine ⟨by simp, ?_⟩
    have hrc := hbFinal'.conserve id
    have h1 : 0 < valsTokens ext id := List.count_pos_iff.mpr hid
    have h2 := valsTokens_ext_le_worldRoots ext (before ++ closeRes.owner :: after) refArena id
    omega
  exact ⟨hclosed.1, hbFinal', hextFinal⟩

/-! ## Root `Ref` arena teardown reaching zero live heap objects -/

private theorem closed_owners_zero_refs (owners : List MemoryScopes.LifecycleOwner)
    (hclosed : ∀ o ∈ owners, o.phase = .closed) (id : ObjId) :
    scopeOwnersRefs owners id = 0 := by
  unfold scopeOwnersRefs MemoryScopes.lifecycleAllRoots
  induction owners with
  | nil => rfl
  | cons o rest ih =>
      have ho := hclosed o (List.mem_cons_self ..)
      have hrest := ih (fun x hx => hclosed x (List.mem_cons_of_mem o hx))
      simp [List.flatMap_cons, MemoryScopes.lifecycleRoots, ho, hrest]

/-- Root teardown theorem: once external roots are released (`ext = []`) and
every scoped owner is closed, detaching and dropping `MemoryArena.close refArena`
(including arbitrary self/mutual `Ref` handle cycles) leaves ZERO live objects
in the entire heap (`∀ id, hEnd id = none`). -/
theorem teardown_zero_live_objects {nextId bound fuel : Nat} {h : AggHeap}
    {rank : ObjId → Nat}
    (owners : List MemoryScopes.LifecycleOwner) (refArena : MemoryArena.Arena)
    (hclosed : ∀ o ∈ owners, o.phase = .closed)
    (hb : HeapBalanced nextId h rank bound (worldRoots [] owners refArena))
    (hfuel : bound ≤ fuel) :
    let released := (MemoryArena.close refArena).2.map arenaValToAggVal
    let hEnd := dropValsFuel fuel h released
    (MemoryArena.close refArena).1 = [] ∧
    ∀ id, hEnd id = none := by
  have hbal : ∀ id, worldRoots [] owners refArena id =
      valsTokens ((MemoryArena.close refArena).2.map arenaValToAggVal) id := by
    intro id
    rw [valsTokens_map_arenaVal, (MemoryArena.close_balance id refArena).2]
    unfold worldRoots refArenaRefs
    rw [closed_owners_zero_refs owners hclosed id]
    simp
  have hcnt : ∀ id, valsTokens ((MemoryArena.close refArena).2.map arenaValToAggVal) id ≤
      worldRoots [] owners refArena id := by
    intro id; rw [hbal id]; exact Nat.le_refl _
  obtain ⟨hbEnd, _⟩ :=
    heapBalanced_dropValsFuel ((MemoryArena.close refArena).2.map arenaValToAggVal) hb hcnt hfuel
  have hbZero : HeapBalanced nextId
      (dropValsFuel fuel h ((MemoryArena.close refArena).2.map arenaValToAggVal))
      rank bound (fun _ => 0) := by
    apply hbEnd.congrRoots
    intro id; rw [hbal id]; omega
  exact ⟨(MemoryArena.close_balance 0 refArena).1, zero_roots_empty_heap hbZero⟩

end Flick.Memory.Soundness
