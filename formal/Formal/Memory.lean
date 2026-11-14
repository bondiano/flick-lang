import Std

/-!
# Local ownership-token preparation (phase M)

These functions mirror the local `dup`/`drop` arithmetic of
`flick-core/src/opt/perceus.rs`. A local starts with one owned token. Its
current operation consumes `consumed` tokens and its continuation needs
one token exactly when `keep` is true. Copies are emitted before uses;
an entirely unused local is dropped.

The theorems cover every natural use count and both continuation choices.
`run` is partial: duplicating or dropping an absent token fails. Thus the
execution theorem also establishes that no prefix of the local plan
underflows. It is not a proof of source-to-memory lowering, path liveness,
the compiler pass as a whole, heap reference counts, reuse, or arenas.
Those remain separate obligations of M1--M4.
-/

namespace Flick.Memory

/-- A continuation owns either one token or none. -/
def kept (keep : Bool) : Nat := if keep then 1 else 0

/-- Number of `dup`s needed from one existing token. -/
def copies (consumed : Nat) (keep : Bool) : Nat :=
  consumed - (if keep then 0 else 1)

/-- Release an existing token when neither this operation nor the continuation uses it. -/
def discarded (consumed : Nat) (keep : Bool) : Nat :=
  if consumed = 0 ∧ keep = false then 1 else 0

-- Boundary regressions: dead, moved, shared, and retained locals.
example : copies 0 false = 0 ∧ discarded 0 false = 1 := by decide
example : copies 1 false = 0 ∧ discarded 1 false = 0 := by decide
example : copies 2 false = 1 ∧ discarded 2 false = 0 := by decide
example : copies 0 true = 0 ∧ discarded 0 true = 0 := by decide
example : copies 2 true = 2 ∧ discarded 2 true = 0 := by decide

/-- The initial token and all copies account exactly for consumption,
continuation ownership, and the one possible unused-token drop. -/
theorem balanced (consumed : Nat) (keep : Bool) :
    1 + copies consumed keep = consumed + kept keep + discarded consumed keep := by
  cases keep <;> cases consumed <;>
    simp [copies, kept, discarded, Nat.add_comm]

/-- Uses and unused-token disposal never exceed the tokens prepared. -/
theorem no_underflow (consumed : Nat) (keep : Bool) :
    consumed + discarded consumed keep ≤ 1 + copies consumed keep := by
  rw [balanced]
  omega

inductive Action where
  | dup
  | drop
  deriving DecidableEq, Repr

/-- Both operations require an existing token. A final drop may reach zero. -/
def step : Action → Nat → Option Nat
  | .dup, 0 => none
  | .dup, n + 1 => some (n + 2)
  | .drop, 0 => none
  | .drop, n + 1 => some n

/-- Failure propagates, rather than being repaired by later actions. -/
def run : List Action → Nat → Option Nat
  | [], n => some n
  | action :: rest, n => (step action n).bind (run rest)

/-- Preparing one local: copies, then unused-token disposal and consuming uses.
Uses and disposal have the same token-count effect, not the same value semantics. -/
def plan (consumed : Nat) (keep : Bool) : List Action :=
  List.replicate (copies consumed keep) .dup ++
    List.replicate (discarded consumed keep + consumed) .drop

theorem run_append (a b : List Action) (n : Nat) :
    run (a ++ b) n = (run a n).bind (run b) := by
  induction a generalizing n with
  | nil => simp [run]
  | cons action rest ih =>
    simp only [List.cons_append, run]
    cases step action n <;> simp [ih]

theorem run_dups (count n : Nat) :
    run (List.replicate count .dup) (n + 1) = some (n + 1 + count) := by
  induction count generalizing n with
  | zero => simp [run]
  | succ count ih =>
    simp [List.replicate_succ, run, step, ih, Nat.add_comm,
      Nat.add_left_comm]

theorem run_drops (count n : Nat) :
    run (List.replicate count .drop) (n + count) = some n := by
  induction count with
  | zero => simp [run]
  | succ count ih =>
    simp [List.replicate_succ, run, step, ih]

/-- Every generated local plan executes without failure and leaves exactly
its continuation token. Since `run` cannot recover from failure, this also
covers every execution prefix, including the zero-use boundary. -/
theorem plan_runs (consumed : Nat) (keep : Bool) :
    run (plan consumed keep) 1 = some (kept keep) := by
  simp only [plan, run_append]
  rw [show (1 : Nat) = 0 + 1 from rfl, run_dups]
  simp only [Nat.zero_add, Option.bind_some]
  have h : 1 + copies consumed keep = kept keep + (discarded consumed keep + consumed) := by
    rw [balanced]
    omega
  rw [h, run_drops]

/-- Every syntactic prefix of a generated plan also succeeds. -/
theorem plan_prefix (consumed : Nat) (keep : Bool) (before after : List Action)
    (h : plan consumed keep = before ++ after) :
    ∃ n, run before 1 = some n := by
  have success := plan_runs consumed keep
  rw [h, run_append] at success
  cases hp : run before 1 with
  | none => simp [hp] at success
  | some n => exact ⟨n, rfl⟩

/-- A last borrow needs no copy; its post-borrow release cannot underflow. -/
theorem last_borrow_drop : run [.drop] 1 = some 0 := rfl

end Flick.Memory

/-- info: 'Flick.Memory.balanced' depends on axioms: [propext] -/
#guard_msgs in
#print axioms Flick.Memory.balanced

/-- info: 'Flick.Memory.no_underflow' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.no_underflow

/-- info: 'Flick.Memory.plan_runs' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.plan_runs

/-- info: 'Flick.Memory.plan_prefix' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.plan_prefix
