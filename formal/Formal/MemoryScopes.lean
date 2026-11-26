import Formal.MemoryArena

/-!
# Selected-owner, multi-slot arena ledger and M2 owner close lifecycle

This models `ArenaMake`, `ArenaAlloc`, `ArenaGet`, and the M2 owner lifecycle
(`Open` -> `Closing` (children/pending work) -> `Finalizing` (LIFO finalizers,
with late arena reads/allocations allowed) -> `Closed` (detach-all retirement))
matching `flick-vm/src/tier0/program/owner_close.rs` and `scoped_values.rs`.

Each `Data::Arena { contents: Vec<Value> }` has ONE private Object owner, not
one cell object per slot. Owner/Arena/View capabilities all map to the nonowning
`Value.handle` abstraction. Object aliases retain multiplicity.

Closing runs in strict phase order across all exit outcomes (`success`,
`failure`, `cancelled`):
1. Children finish before finalizers begin (`children = 0`).
2. Registered finalizers run in exact reverse registration order (LIFO) while
   arena storage remains live (`storageLive = true`), and finalizers may read
   or allocate additional arenas/slots into the finalizing owner.
3. Only after all finalizers finish does `retire` detach ALL arena slot vectors
   before ordinary drops of private roots and contents, leaving zero live
   arena-owned roots (`ownerRoots = []`) and transitioning to `closed`.
4. Closing an already-closed owner is idempotent (runs zero finalizers and
   releases zero tokens), and closed owners/arenas reject further admission
   (`make`, `alloc`, `get` return `none`).
5. Unrelated owners and external roots (including the root Ref-cell ledger) are
   preserved with exact per-object token multiplicity.

**Scope of Lean guarantees vs compiler/runtime obligations:**
This proves the abstract multi-phase owner/arena token ledger and LIFO close
order. It does NOT prove Rust compiler/checker soundness, source-to-memory
lowering equivalence, handle freshness/provenance checks, counted heap-graph
acyclicity, host-bridge wire marshalling, or physical memory reclamation.
-/
namespace Flick.MemoryScopes

open MemoryArena (Value token inventory inventory_append)

structure Storage where
  root : Nat
  slots : List Value
  deriving DecidableEq, Repr

abbrev Arenas := List Storage

def owned (arenas : Arenas) : List Value :=
  arenas.flatMap fun a => Value.object a.root :: a.slots

def balance (id : Nat) (arenas : Arenas) : Nat := inventory id (owned arenas)

@[simp] theorem balance_append (id : Nat) (a b : Arenas) :
    balance id (a ++ b) = balance id a + balance id b := by
  simp [balance, owned, List.flatMap_append]

/-- ArenaMake: one empty storage object, one private owner, zero-token handle. -/
def make (arenas : Arenas) (fresh : Nat) : Arenas × Value :=
  (arenas ++ [⟨fresh, []⟩], .handle fresh)

theorem make_balance (id fresh : Nat) (arenas : Arenas) :
    balance id (make arenas fresh).1 = balance id arenas + token id (.object fresh) ∧
    token id (make arenas fresh).2 = 0 := by
  simp [make, balance, owned, inventory, token]

/-- ArenaAlloc transfers an incoming owned value into the vector; the view
is nonowning. A valid runtime view also records this slot's index. -/
def alloc (storage : Storage) (incoming : Value) : Storage × Value :=
  (⟨storage.root, storage.slots ++ [incoming]⟩, .handle storage.root)

theorem alloc_balance (id : Nat) (before after : Arenas) (s : Storage)
    (incoming : Value) :
    balance id (before ++ (alloc s incoming).1 :: after) =
      balance id (before ++ s :: after) + token id incoming ∧
    token id (alloc s incoming).2 = 0 := by
  simp [alloc, balance, owned, inventory, token, Nat.add_assoc]
  omega

/-- ArenaGet selects a slot and `heap.dup` retains its representation. -/
def get (s : Storage) (index : Nat) : Option Value := s.slots[index]?

theorem get_selected (root : Nat) (before after : List Value) (v : Value) :
    get ⟨root, before ++ v :: after⟩ before.length = some v := by
  simp [get]

/-- The original storage stays owned and the returned value adds exactly
one counted token iff it is Object (MemoryArena.get_counted_iff). -/
theorem get_retains (id : Nat) (s : Storage) (index : Nat) (v : Value)
    (selected : get s index = some v) :
    get s index = some v ∧
    inventory id (owned [s] ++ (get s index).toList) =
      balance id [s] + token id v := by
  exact ⟨selected, by simp [selected, balance, inventory]⟩

/-- First phase of close_owner: detach vectors, preserving every root. -/
def detach : Arenas → Arenas × List Value
  | [] => ([], [])
  | a :: arenas =>
    let rest := detach arenas
    (⟨a.root, []⟩ :: rest.1, a.slots ++ rest.2)

theorem detach_empty (arenas : Arenas) :
    ∀ a ∈ (detach arenas).1, a.slots = [] := by
  induction arenas with
  | nil => simp [detach]
  | cons a arenas ih =>
    simp only [detach, List.mem_cons]
    intro s hs
    rcases hs with rfl | hs
    · rfl
    · exact ih s hs

theorem detach_roots (arenas : Arenas) :
    (detach arenas).1.map Storage.root = arenas.map Storage.root := by
  induction arenas with
  | nil => rfl
  | cons a arenas ih => simp [detach, ih]

theorem detach_balance (id : Nat) (arenas : Arenas) :
    balance id arenas = balance id (detach arenas).1 + inventory id (detach arenas).2 := by
  induction arenas with
  | nil => rfl
  | cons a arenas ih =>
    simp only [detach, balance, owned, List.flatMap_cons, List.cons_append,
      inventory, List.nil_append, inventory_append] at *
    omega

/-- After detaching, ordinary drop roots are private owners then contents,
exactly the `roots.extend(detached)` stream, without heap traversal. -/
def releases (arenas : Arenas) : List Value :=
  let detached := detach arenas
  detached.1.map (fun a => Value.object a.root) ++ detached.2

theorem empty_owned (id : Nat) (arenas : Arenas)
    (h : ∀ a ∈ arenas, a.slots = []) :
    balance id arenas = inventory id (arenas.map fun a => Value.object a.root) := by
  induction arenas with
  | nil => rfl
  | cons a arenas ih =>
    have ha := h a (by simp)
    have ht : ∀ s ∈ arenas, s.slots = [] := by
      intro s hs
      exact h s (by simp [hs])
    simp [balance, owned, ha, inventory] at *
    exact ih ht

theorem releases_balance (id : Nat) (arenas : Arenas) :
    inventory id (releases arenas) = balance id arenas := by
  simp only [releases, inventory_append]
  rw [← empty_owned id (detach arenas).1 (detach_empty arenas)]
  exact (detach_balance id arenas).symm

inductive Owner where
  | active (arenas : Arenas)
  | closed
  deriving DecidableEq, Repr

def ownerRoots : Owner → List Value
  | .active arenas => owned arenas
  | .closed => []

def close : Owner → Owner × List Value
  | .active arenas => (.closed, releases arenas)
  | .closed => (.closed, [])

theorem close_balance (id : Nat) (owner : Owner) :
    (close owner).1 = .closed ∧
    inventory id (close owner).2 = inventory id (ownerRoots owner) := by
  cases owner with
  | active arenas => exact ⟨rfl, releases_balance id arenas⟩
  | closed => exact ⟨rfl, rfl⟩

def roots (owners : List Owner) : List Value := owners.flatMap ownerRoots

/-- Other owner contexts and ordinary/root-Ref roots are untouched. This
holds per arbitrary heap object, including aliases across all contexts. -/
theorem close_selected (id : Nat) (before after : List Owner) (owner : Owner)
    (external : List Value) :
    inventory id (external ++ roots (before ++ owner :: after)) =
      inventory id (external ++ roots (before ++ (close owner).1 :: after)) +
      inventory id (close owner).2 := by
  have h := close_balance id owner
  simp [roots, List.flatMap_append, inventory_append, h.1, ownerRoots] at *
  omega

/-!
## M2 multi-phase owner close lifecycle (C0–C6)

Models `OwnerPhase` (`open_` -> `closing` -> `finalizing` -> `closed`),
child-completion before finalizers, LIFO finalizer execution (including late
arena allocation during finalization), post-finalizer exact-once retirement,
closed-owner admission refusal, and idempotent re-close across all exit kinds.
-/

inductive Phase where
  | open_
  | closing
  | finalizing
  | closed
  deriving DecidableEq, Repr

inductive ExitKind where
  | success
  | failure
  | cancelled
  deriving DecidableEq, Repr

/-- A registered finalizer action with a trace label and optional late arena
storage created while finalizing (which must also retire at post-C5 close). -/
structure Finalizer where
  label : Nat
  lateArenas : Arenas
  deriving DecidableEq, Repr

/-- Owner state across its full lifecycle. `finalizers` stores registrations in
registration order (oldest first, newest at the end of the list), matching
`Vec::push` / `Vec::pop` LIFO execution. -/
structure LifecycleOwner where
  phase : Phase
  children : Nat
  finalizers : List Finalizer
  arenas : Arenas
  deriving DecidableEq, Repr

/-- Storage remains live through `open_`, `closing` (C1–C4), and `finalizing` (C5). -/
def storageLive (o : LifecycleOwner) : Bool :=
  o.phase != .closed

/-- Arena admission (`ArenaMake`) is allowed until post-C5 retirement (`closed`). -/
def admitsArena (o : LifecycleOwner) : Bool :=
  o.phase != .closed

/-- Child admission (`Fiber.spawn`) is allowed only while `open_` (C0). -/
def admitsChild (o : LifecycleOwner) : Bool :=
  o.phase == .open_

/-- Arena roots currently owned by `o`. -/
def lifecycleRoots (o : LifecycleOwner) : List Value :=
  match o.phase with
  | .closed => []
  | _ => owned o.arenas

/-- Guarded `ArenaMake`: succeeds iff the owner is not closed. -/
def makeGuarded (o : LifecycleOwner) (fresh : Nat) : Option (LifecycleOwner × Value) :=
  if admitsArena o then
    let res := make o.arenas fresh
    some ({ o with arenas := res.1 }, res.2)
  else
    none

/-- Guarded `ArenaAlloc` into arena `idx`: rejected once the owner is closed. -/
def allocGuarded (o : LifecycleOwner) (idx : Nat) (incoming : Value) :
    Option (LifecycleOwner × Value) :=
  if storageLive o then
    match o.arenas[idx]? with
    | none => none
    | some s =>
      let res := alloc s incoming
      some ({ o with arenas := o.arenas.set idx res.1 }, res.2)
  else
    none

/-- Guarded `ArenaGet` from arena `idx`, slot `slotIdx`: rejected once closed. -/
def getGuarded (o : LifecycleOwner) (idx slotIdx : Nat) : Option Value :=
  if storageLive o then
    match o.arenas[idx]? with
    | none => none
    | some s => get s slotIdx
  else
    none

theorem closed_rejects_make (o : LifecycleOwner) (fresh : Nat) (h : o.phase = .closed) :
    makeGuarded o fresh = none := by
  simp [makeGuarded, admitsArena, h]

theorem closed_rejects_alloc (o : LifecycleOwner) (idx : Nat) (v : Value) (h : o.phase = .closed) :
    allocGuarded o idx v = none := by
  simp [allocGuarded, storageLive, h]

theorem closed_rejects_get (o : LifecycleOwner) (idx slotIdx : Nat) (h : o.phase = .closed) :
    getGuarded o idx slotIdx = none := by
  simp [getGuarded, storageLive, h]

/-- Trace events emitted during owner close. -/
inductive CloseEvent where
  | childrenDone (exit : ExitKind)
  | runFinalizer (label : Nat)
  | arenasRetired
  deriving DecidableEq, Repr

/-- Execute a stack of finalizers in LIFO order (taking from the end of the
registration list, i.e., folding over `finalizers.reverse`), keeping arenas live
and collecting any late arenas allocated during finalization. -/
def runFinalizersLifo (arenas : Arenas) : List Finalizer → Arenas × List CloseEvent
  | [] => (arenas, [])
  | f :: rest =>
    let next := runFinalizersLifo (arenas ++ f.lateArenas) rest
    (next.1, .runFinalizer f.label :: next.2)

def finalizersLateArenas (fins : List Finalizer) : Arenas :=
  fins.flatMap Finalizer.lateArenas

theorem runFinalizersLifo_balance (id : Nat) (arenas : Arenas) (fins : List Finalizer) :
    balance id (runFinalizersLifo arenas fins).1 =
      balance id arenas + balance id (finalizersLateArenas fins) := by
  induction fins generalizing arenas with
  | nil => simp [runFinalizersLifo, finalizersLateArenas, balance, owned, inventory]
  | cons f rest ih =>
    simp only [runFinalizersLifo, finalizersLateArenas, List.flatMap_cons] at *
    rw [ih (arenas ++ f.lateArenas), balance_append, balance_append]
    omega

theorem runFinalizersLifo_events (arenas : Arenas) (fins : List Finalizer) :
    (runFinalizersLifo arenas fins).2 = fins.map (fun f => CloseEvent.runFinalizer f.label) := by
  induction fins generalizing arenas with
  | nil => rfl
  | cons f rest ih => simp [runFinalizersLifo, ih]

/-- Result of running full C0–C6 owner close on a `LifecycleOwner`. -/
structure CloseResult where
  owner : LifecycleOwner
  released : List Value
  events : List CloseEvent
  deriving DecidableEq, Repr

/-- Full M2 C0–C6 close:
- Already `closed`: no-op (zero finalizers, zero released tokens, no events).
- Active (`open_`, `closing`, or `finalizing`):
  1. C0–C4: stop admission and wait until all children finish (`children := 0`).
  2. C5: run registered finalizers in reverse registration order (LIFO),
     keeping storage live and collecting any late arenas allocated in C5.
  3. Post-C5/C6: detach all arena vectors and emit all private roots + slot
     values exactly once, transitioning to `closed` with empty arenas/finalizers. -/
def closeLifecycle (exit : ExitKind) (o : LifecycleOwner) : CloseResult :=
  match o.phase with
  | .closed =>
    { owner := { phase := .closed, children := 0, finalizers := [], arenas := [] }
      released := []
      events := [] }
  | .open_ | .closing | .finalizing =>
    let (afterFins, finEvents) := runFinalizersLifo o.arenas o.finalizers.reverse
    { owner := { phase := .closed, children := 0, finalizers := [], arenas := [] }
      released := releases afterFins
      events := CloseEvent.childrenDone exit :: finEvents ++ [CloseEvent.arenasRetired] }

theorem closeLifecycle_reaches_closed (exit : ExitKind) (o : LifecycleOwner) :
    (closeLifecycle exit o).owner.phase = .closed ∧
    (closeLifecycle exit o).owner.children = 0 ∧
    (closeLifecycle exit o).owner.finalizers = [] ∧
    (closeLifecycle exit o).owner.arenas = [] ∧
    lifecycleRoots (closeLifecycle exit o).owner = [] := by
  cases h : o.phase <;> simp [closeLifecycle, lifecycleRoots, h]

/-- Finalizers run in exact LIFO order strictly after `childrenDone` and
strictly before `arenasRetired`, regardless of exit kind. -/
theorem closeLifecycle_lifo_order (exit : ExitKind) (o : LifecycleOwner)
    (hopen : o.phase ≠ .closed) :
    (closeLifecycle exit o).events =
      CloseEvent.childrenDone exit ::
        (o.finalizers.reverse.map (fun f => CloseEvent.runFinalizer f.label)) ++
        [CloseEvent.arenasRetired] := by
  cases h : o.phase with
  | closed => exact (hopen h).elim
  | open_ | closing | finalizing =>
    simp [closeLifecycle, h, runFinalizersLifo_events]

/-- Exact-once per-object release accounting: closing releases the exact token
inventory of initial arenas plus all late arenas allocated during finalization. -/
theorem closeLifecycle_exact_balance (id : Nat) (exit : ExitKind) (o : LifecycleOwner)
    (hopen : o.phase ≠ .closed) :
    inventory id (closeLifecycle exit o).released =
      inventory id (lifecycleRoots o) +
      balance id (finalizersLateArenas o.finalizers.reverse) := by
  cases h : o.phase with
  | closed => exact (hopen h).elim
  | open_ | closing | finalizing =>
    simp only [closeLifecycle, lifecycleRoots, h, releases_balance, runFinalizersLifo_balance]
    rfl

/-- Closing an already-closed owner (or closing twice under any combination of
exit kinds) releases zero tokens and runs zero events. -/
theorem closeLifecycle_idempotent (exit1 exit2 : ExitKind) (o : LifecycleOwner) :
    let first := closeLifecycle exit1 o
    let second := closeLifecycle exit2 first.owner
    second.owner = first.owner ∧
    second.released = [] ∧
    second.events = [] := by
  cases h : o.phase <;> simp [closeLifecycle, h]

def lifecycleAllRoots (owners : List LifecycleOwner) : List Value :=
  owners.flatMap lifecycleRoots

/-- Selected-owner close in an arbitrary multi-owner + external-root context:
closing one active owner leaves every other owner and external root untouched,
retires that owner's roots to zero, and emits exact initial + late-C5 tokens. -/
theorem closeLifecycle_selected (id : Nat) (exit : ExitKind)
    (before after : List LifecycleOwner) (owner : LifecycleOwner)
    (external : List Value) (hopen : owner.phase ≠ .closed) :
    inventory id (external ++ lifecycleAllRoots (before ++ owner :: after)) +
      balance id (finalizersLateArenas owner.finalizers.reverse) =
      inventory id (external ++ lifecycleAllRoots (before ++ (closeLifecycle exit owner).owner :: after)) +
      inventory id (closeLifecycle exit owner).released := by
  have hclosed := closeLifecycle_reaches_closed exit owner
  have hbal := closeLifecycle_exact_balance id exit owner hopen
  simp only [lifecycleAllRoots, List.flatMap_append, List.flatMap_cons,
    inventory_append, hclosed.2.2.2.2, List.nil_append] at *
  omega

-- Multiple slots share ONE root object; aliased contents count twice.
example : inventory 7 (releases [⟨0, [.object 7, .object 7, .handle 0]⟩]) = 2 := by
  decide
example : inventory 0 (releases [⟨0, [.handle 0, .handle 1, .handle 2]⟩]) = 1 := by
  decide
-- Retained Owner/Arena/View capabilities contribute no releases or retains.
example (id capability : Nat) : token id (.handle capability) = 0 := rfl
example : close (close (.active [⟨0, [.object 7]⟩])).1 = (.closed, []) := rfl
-- LIFO finalizer order + late arena retirement on failure/cancellation.
example :
    let o : LifecycleOwner :=
      { phase := .open_
        children := 2
        finalizers := [⟨1, []⟩, ⟨2, [⟨10, [.object 99]⟩]⟩]
        arenas := [⟨0, [.object 7]⟩] }
    (closeLifecycle .cancelled o).events =
      [.childrenDone .cancelled, .runFinalizer 2, .runFinalizer 1, .arenasRetired] ∧
    inventory 99 (closeLifecycle .cancelled o).released = 1 ∧
    inventory 7 (closeLifecycle .cancelled o).released = 1 := by
  decide

end Flick.MemoryScopes

/-- info: 'Flick.MemoryScopes.alloc_balance' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.alloc_balance
/-- info: 'Flick.MemoryScopes.get_retains' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.get_retains
/-- info: 'Flick.MemoryScopes.detach_balance' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.detach_balance
/-- info: 'Flick.MemoryScopes.close_selected' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.close_selected
/-- info: 'Flick.MemoryScopes.closeLifecycle_lifo_order' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.closeLifecycle_lifo_order
/-- info: 'Flick.MemoryScopes.closeLifecycle_selected' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.closeLifecycle_selected
/-- info: 'Flick.MemoryScopes.closeLifecycle_idempotent' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.MemoryScopes.closeLifecycle_idempotent
