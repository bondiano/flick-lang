import Std

/-!
# Root cell arena ownership ledger

This abstracts `flick-model/src/memory.rs` and `memory/program.rs`: `RefMake`
allocates a `Data::Cell` object and keeps its sole private owner in
`Machine.cells`; `CellHandle` is nonowning. Slots hold strong values.
`RefGet` duplicates the current value, `RefSet` replaces before dropping,
and `cleanup` takes ALL slots before ordinary `drop_values` of cell owners
and detached contents. No reachability scan is represented or required.

Object numbers identify counted tokens, not their heap payloads. Multiplicity
is retained, including aliases. Handles may name arbitrary, self, or mutually
referencing cells. There is no finite bound on arena size or object numbers.
This proves the arena root/slot ledger, NOT counted heap-graph acyclicity,
physical deallocation, source checking, whole Perceus insertion, or all M4.
Fresh allocation, valid-handle lookup and execution lifetime checks remain
Rust obligations. All operations here are successful, validated operations;
no scope-close or Native API is specified.
-/
namespace Flick.MemoryArena

inductive Value where
  | immediate
  | handle (cell : Nat)
  | object (id : Nat)
  deriving DecidableEq, Repr

/-- Only an Object token participates in ordinary reference counting. -/
def token (id : Nat) : Value → Nat
  | .object other => if id = other then 1 else 0
  | _ => 0

def inventory (id : Nat) : List Value → Nat
  | [] => 0
  | v :: vs => token id v + inventory id vs

@[simp] theorem inventory_append (id : Nat) (xs ys : List Value) :
    inventory id (xs ++ ys) = inventory id xs + inventory id ys := by
  induction xs with
  | nil => simp [inventory]
  | cons v vs ih => simp [inventory, ih, Nat.add_assoc]

structure Cell where
  owner : Nat
  slot : Option Value
  deriving DecidableEq, Repr

abbrev Arena := List Cell

def slotValues (c : Cell) : List Value := c.slot.toList

def owned (a : Arena) : List Value :=
  a.flatMap fun c => Value.object c.owner :: slotValues c

def balance (id : Nat) (a : Arena) : Nat := inventory id (owned a)

@[simp] theorem balance_append (id : Nat) (a b : Arena) :
    balance id (a ++ b) = balance id a + balance id b := by
  simp [balance, owned, List.flatMap_append]

/-- RefMake transfers `incoming` into the new slot and keeps the cell's
allocated Object token, not the returned nonowning handle. -/
def make (a : Arena) (fresh : Nat) (incoming : Value) : Arena × Value :=
  (a ++ [⟨fresh, some incoming⟩], .handle fresh)

theorem make_balance (id fresh : Nat) (a : Arena) (incoming : Value) :
    balance id (make a fresh incoming).1 =
      balance id a + token id (.object fresh) + token id incoming ∧
    token id (make a fresh incoming).2 = 0 := by
  simp [make, balance, owned, slotValues, inventory, token, Nat.add_assoc]

/-- RefGet leaves the slot intact; `heap.dup` grants one extra owned token
exactly for Object, while immediates and CellHandle need no counted retain. -/
def get (c : Cell) : Option Value := c.slot

theorem get_retains (id owner : Nat) (v : Value) :
    get ⟨owner, some v⟩ = some v ∧
    balance id [⟨owner, some v⟩] + inventory id [v] =
      token id (.object owner) + 2 * token id v := by
  simp [get, balance, owned, slotValues, inventory]
  omega

theorem get_counted_iff (id : Nat) (v : Value) :
    token id v = 1 ↔ v = .object id := by
  cases v <;> simp [token, eq_comm]

/-- `mem::replace` returns the old slot for later drop, after installing
incoming ownership. This also describes the commit inside RefModify.
Its callback tuple is (result, new contents), NOT (new contents, result). -/
def replace (c : Cell) (incoming : Value) : Cell × List Value :=
  (⟨c.owner, some incoming⟩, slotValues c)

theorem replace_installed (c : Cell) (incoming : Value) :
    (replace c incoming).1.owner = c.owner ∧
    (replace c incoming).1.slot = some incoming ∧
    (replace c incoming).2 = slotValues c := by
  exact ⟨rfl, rfl, rfl⟩

/-- In ANY arena context, replacement transfers the incoming token and
releases exactly the previous slot, including same-object replacement. -/
theorem replace_balance (id : Nat) (before after : Arena) (c : Cell)
    (incoming : Value) :
    balance id (before ++ c :: after) + token id incoming =
      balance id (before ++ (replace c incoming).1 :: after) +
      inventory id (replace c incoming).2 := by
  cases c with
  | mk owner slot =>
    cases slot <;>
      simp [replace, balance, owned, slotValues, inventory, Nat.add_assoc] <;> omega

/-- Taking slots is a pure first phase. No ordinary drop occurs here. -/
def detach : Arena → Arena × List Value
  | [] => ([], [])
  | c :: cs =>
    let rest := detach cs
    (⟨c.owner, none⟩ :: rest.1, slotValues c ++ rest.2)

theorem detach_empty (a : Arena) :
    ∀ c ∈ (detach a).1, c.slot = none := by
  induction a with
  | nil => simp [detach]
  | cons c cs ih =>
    simp only [detach, List.mem_cons]
    intro x hx
    rcases hx with rfl | hx
    · rfl
    · exact ih x hx

theorem detach_owners (a : Arena) :
    (detach a).1.map Cell.owner = a.map Cell.owner := by
  induction a with
  | nil => rfl
  | cons c cs ih => simp [detach, ih]

theorem detach_balance (id : Nat) (a : Arena) :
    balance id a = balance id (detach a).1 + inventory id (detach a).2 := by
  induction a with
  | nil => rfl
  | cons c cs ih =>
    simp only [detach, balance, owned, List.flatMap_cons, List.cons_append,
      inventory, slotValues, Option.toList_none, List.nil_append,
      inventory_append] at *
    omega

/-- Second phase drains private owners, then detached contents, matching
cleanup's `roots.extend(take(cells)); roots.extend(detached)`. Other machine
roots are independent and can be prepended via `close_with_roots` below. -/
def close (a : Arena) : Arena × List Value :=
  let detached := detach a
  ([], detached.1.map (fun c => Value.object c.owner) ++ detached.2)

theorem empty_slots_owned (id : Nat) (a : Arena)
    (h : ∀ c ∈ a, c.slot = none) :
    balance id a = inventory id (a.map fun c => Value.object c.owner) := by
  induction a with
  | nil => rfl
  | cons c cs ih =>
    have hc := h c (by simp)
    have ht : ∀ d ∈ cs, d.slot = none := by
      intro d hd
      exact h d (by simp [hd])
    simp [balance, owned, slotValues, hc, inventory] at *
    exact ih ht

/-- Exact per-object multiplicity, not merely a total count or set equality.
Every slot is empty before release; no private owner remains after close. -/
theorem close_balance (id : Nat) (a : Arena) :
    (close a).1 = [] ∧
    inventory id (close a).2 = balance id a := by
  constructor
  · rfl
  · simp only [close, inventory_append]
    rw [← empty_slots_owned id (detach a).1 (detach_empty a)]
    exact (detach_balance id a).symm

theorem close_with_roots (id : Nat) (a : Arena) (roots : List Value) :
    inventory id (roots ++ (close a).2) = inventory id roots + balance id a := by
  simp [close_balance id a |>.2]

-- Regression: a mutual handle cycle contributes only the two private owners.
example : (close [⟨0, some (.handle 1)⟩, ⟨1, some (.handle 0)⟩]).2 =
    [.object 0, .object 1, .handle 1, .handle 0] := rfl
-- A self-handle is nonowning; aliasing counted contents retains multiplicity.
example : inventory 7 (close [⟨0, some (.object 7)⟩,
    ⟨1, some (.object 7)⟩, ⟨2, some (.handle 2)⟩]).2 = 2 := by decide

end Flick.MemoryArena

/-- info: 'Flick.MemoryArena.replace_balance' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryArena.replace_balance

/-- info: 'Flick.MemoryArena.get_retains' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryArena.get_retains

/-- info: 'Flick.MemoryArena.close_balance' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryArena.close_balance

/-- info: 'Flick.MemoryArena.close_with_roots' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.MemoryArena.close_with_roots
