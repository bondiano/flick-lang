import Formal.MemoryArena

/-!
# Selected-owner, multi-slot arena ledger

This models successful `ArenaMake`, `ArenaAlloc`, `ArenaGet` and the
`close_owner` release boundary in `flick-model/src/memory/program.rs`.
Each `Data::Arena { contents: Vec<Value> }` has ONE private Object owner,
not one cell object per slot. Owner/Arena/View capabilities all map to the
nonowning `Value.handle` abstraction. Object aliases retain multiplicity.

Closing detaches ALL vectors before ordinary drops of roots and contents.
The owner sum type records the implementation invariant that closed owners
have no arena roots. Contexts include arbitrary other owners and external
roots (including the independent root Ref-cell ledger). No reachability
scan is modeled. Handles and object IDs are unrestricted.

This is a root/slot token ledger, not source/checker soundness, freshness,
provenance validation, heap acyclicity, physical reclamation, or all M4.
Closure is conditional on reaching the post-finalizer release boundary;
there is no finalizer/handler execution theorem. Such constructs remain
unsupported on the counted path; JS/Go release after all finalizers.
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

-- Multiple slots share ONE root object; aliased contents count twice.
example : inventory 7 (releases [⟨0, [.object 7, .object 7, .handle 0]⟩]) = 2 := by
  decide
example : inventory 0 (releases [⟨0, [.handle 0, .handle 1, .handle 2]⟩]) = 1 := by
  decide
-- Retained Owner/Arena/View capabilities contribute no releases or retains.
example (id capability : Nat) : token id (.handle capability) = 0 := rfl
example : close (close (.active [⟨0, [.object 7]⟩])).1 = (.closed, []) := rfl

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
