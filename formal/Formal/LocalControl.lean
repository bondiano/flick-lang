/-! E8's local completion algebra, independent of compiler lowering.
Every exit carries the current store. Fuel exhaustion is not termination. -/
namespace Flick.LocalControl

inductive Completion (σ α β ε : Type) where
  | normal (value : α) (store : σ)
  | break_ (value : β) (store : σ)
  | continue_ (store : σ)
  | raised (error : ε) (store : σ)

def bind (c : Completion σ α β ε) (k : σ → α → Completion σ γ β ε) : Completion σ γ β ε :=
  match c with
  | .normal v s => k s v
  | .break_ v s => .break_ v s
  | .continue_ s => .continue_ s
  | .raised e s => .raised e s

def catchError (c : Completion σ α β ε) (h : σ → ε → Completion σ α β ε) : Completion σ α β ε :=
  match c with
  | .raised e s => h s e
  | other => other

def assign (rhs : Completion σ α β ε) (write : σ → α → σ) : Completion σ Unit β ε :=
  bind rhs (fun s v => .normal () (write s v))

@[simp] theorem assign_error (e : ε) (s : σ) (write : σ → α → σ) :
    assign (.raised e s : Completion σ α β ε) write = .raised e s := rfl
@[simp] theorem assign_value (v : α) (s : σ) (write : σ → α → σ) :
    assign (.normal v s : Completion σ α β ε) write = .normal () (write s v) := rfl
@[simp] theorem handler_sees_current_store (e : ε) (s : σ) (h : σ → ε → Completion σ α β ε) :
    catchError (.raised e s) h = h s e := rfl
@[simp] theorem break_skips_suffix (v : β) (s : σ) (k : σ → α → Completion σ γ β ε) :
    bind (.break_ v s) k = .break_ v s := rfl
@[simp] theorem continue_skips_suffix (s : σ) (k : σ → α → Completion σ γ β ε) :
    bind (.continue_ s) k = .continue_ s := rfl

def store : Completion σ α β ε → σ
  | .normal _ s | .break_ _ s | .continue_ s | .raised _ s => s

def iterate : Nat → (σ → Completion σ Unit α ε) → σ → Option (Completion σ α α ε)
  | 0, _, _ => none
  | n + 1, body, s => match body s with
    | .normal _ s' | .continue_ s' => iterate n body s'
    | .break_ v s' => some (.normal v s')
    | .raised e s' => some (.raised e s')

/-- A store invariant preserved by each body is preserved on every finite exit,
including the state accompanying an error. -/
theorem iterate_preserves (P : σ → Prop) (body : σ → Completion σ Unit α ε)
    (step : ∀ s, P s → P (store (body s))) (n : Nat) (s : σ) (initial : P s)
    (c : Completion σ α α ε) (run : iterate n body s = some c) : P (store c) := by
  induction n generalizing s with
  | zero => simp [iterate] at run
  | succ n ih =>
    have safe := step s initial
    cases hb : body s with
    | normal v s' =>
      simp [hb, store] at safe
      exact ih s' safe (by simpa [iterate, hb] using run)
    | continue_ s' =>
      simp [hb, store] at safe
      exact ih s' safe (by simpa [iterate, hb] using run)
    | break_ v s' =>
      simp [iterate, hb] at run
      cases run
      simpa [hb, store] using safe
    | raised e s' =>
      simp [iterate, hb] at run
      cases run
      simpa [hb, store] using safe

/-! ## Labeled Control Flow and Scope Finalizers (#432) -/

/-- Labeled completion carrying the store on every exit branch.
Supports targeted multi-level break and continue exits. -/
inductive LabeledCompletion (ι : Type) (σ α β ε : Type) where
  | normal (val : α) (store : σ)
  | break_ (target : ι) (value : β) (store : σ)
  | continue_ (target : ι) (store : σ)
  | raised (error : ε) (store : σ)

/-- Access the store associated with any labeled completion outcome. -/
def LabeledCompletion.store {ι σ α β ε : Type} : LabeledCompletion ι σ α β ε → σ
  | .normal _ s | .break_ _ _ s | .continue_ _ s | .raised _ s => s

/-- Replace the store carried by a labeled completion without modifying its variant or payload. -/
def LabeledCompletion.withStore {ι σ α β ε : Type} (c : LabeledCompletion ι σ α β ε) (s' : σ) :
    LabeledCompletion ι σ α β ε :=
  match c with
  | .normal v _ => .normal v s'
  | .break_ target v _ => .break_ target v s'
  | .continue_ target _ => .continue_ target s'
  | .raised e _ => .raised e s'

@[simp] theorem LabeledCompletion.store_withStore {ι σ α β ε : Type}
    (c : LabeledCompletion ι σ α β ε) (s' : σ) :
    (c.withStore s').store = s' := by
  cases c <;> rfl

@[simp] theorem LabeledCompletion.withStore_withStore {ι σ α β ε : Type}
    (c : LabeledCompletion ι σ α β ε) (s₁ s₂ : σ) :
    (c.withStore s₁).withStore s₂ = c.withStore s₂ := by
  cases c <;> rfl

/-- Sequential binding for labeled completions: executes continuation `k` on `.normal`,
short-circuiting early breaks, continues, or raised errors. -/
def LabeledCompletion.bind {ι σ α β γ ε : Type} (c : LabeledCompletion ι σ α β ε)
    (k : σ → α → LabeledCompletion ι σ γ β ε) : LabeledCompletion ι σ γ β ε :=
  match c with
  | .normal v s => k s v
  | .break_ target v s => .break_ target v s
  | .continue_ target s => .continue_ target s
  | .raised e s => .raised e s

@[simp] theorem LabeledCompletion.bind_normal {ι σ α β γ ε : Type} (v : α) (s : σ)
    (k : σ → α → LabeledCompletion ι σ γ β ε) :
    (LabeledCompletion.normal v s : LabeledCompletion ι σ α β ε).bind k = k s v := rfl

@[simp] theorem LabeledCompletion.bind_break {ι σ α β γ ε : Type} (target : ι) (v : β) (s : σ)
    (k : σ → α → LabeledCompletion ι σ γ β ε) :
    (LabeledCompletion.break_ target v s : LabeledCompletion ι σ α β ε).bind k = .break_ target v s := rfl

@[simp] theorem LabeledCompletion.bind_continue {ι σ α β γ ε : Type} (target : ι) (s : σ)
    (k : σ → α → LabeledCompletion ι σ γ β ε) :
    (LabeledCompletion.continue_ target s : LabeledCompletion ι σ α β ε).bind k = .continue_ target s := rfl

@[simp] theorem LabeledCompletion.bind_raised {ι σ α β γ ε : Type} (e : ε) (s : σ)
    (k : σ → α → LabeledCompletion ι σ γ β ε) :
    (LabeledCompletion.raised e s : LabeledCompletion ι σ α β ε).bind k = .raised e s := rfl

/-- Catch error handler for labeled completions: intercepts `.raised` outcomes. -/
def LabeledCompletion.catchError {ι σ α β ε : Type} (c : LabeledCompletion ι σ α β ε)
    (h : σ → ε → LabeledCompletion ι σ α β ε) : LabeledCompletion ι σ α β ε :=
  match c with
  | .raised e s => h s e
  | other => other

@[simp] theorem LabeledCompletion.catchError_raised {ι σ α β ε : Type} (e : ε) (s : σ)
    (h : σ → ε → LabeledCompletion ι σ α β ε) :
    (LabeledCompletion.raised e s : LabeledCompletion ι σ α β ε).catchError h = h s e := rfl

/-- Run finalizers in reverse registration order (LIFO: newest registered runs first). -/
def runFinalizers {σ : Type} (fins : List (σ → σ)) (s : σ) : σ :=
  fins.foldr (fun f acc => f acc) s

/-- Apply scope finalizers to any labeled completion in reverse registration (LIFO) order. -/
def scopeFinalize {ι σ α β ε : Type} (fins : List (σ → σ))
    (c : LabeledCompletion ι σ α β ε) : LabeledCompletion ι σ α β ε :=
  c.withStore (runFinalizers fins c.store)

@[simp] theorem scopeFinalize_nil {ι σ α β ε : Type} (c : LabeledCompletion ι σ α β ε) :
    scopeFinalize [] c = c := by
  cases c <;> rfl

@[simp] theorem scopeFinalize_normal {ι σ α β ε : Type} (fins : List (σ → σ)) (v : α) (s : σ) :
    scopeFinalize fins (.normal v s : LabeledCompletion ι σ α β ε) = .normal v (runFinalizers fins s) := rfl

@[simp] theorem scopeFinalize_break {ι σ α β ε : Type} (fins : List (σ → σ)) (target : ι) (v : β) (s : σ) :
    scopeFinalize fins (.break_ target v s : LabeledCompletion ι σ α β ε) =
      .break_ target v (runFinalizers fins s) := rfl

@[simp] theorem scopeFinalize_continue {ι σ α β ε : Type} (fins : List (σ → σ)) (target : ι) (s : σ) :
    scopeFinalize fins (.continue_ target s : LabeledCompletion ι σ α β ε) =
      .continue_ target (runFinalizers fins s) := rfl

@[simp] theorem scopeFinalize_raised {ι σ α β ε : Type} (fins : List (σ → σ)) (e : ε) (s : σ) :
    scopeFinalize fins (.raised e s : LabeledCompletion ι σ α β ε) =
      .raised e (runFinalizers fins s) := rfl

@[simp] theorem scopeFinalize_store {ι σ α β ε : Type} (fins : List (σ → σ))
    (c : LabeledCompletion ι σ α β ε) :
    (scopeFinalize fins c).store = runFinalizers fins c.store := by
  cases c <;> rfl

theorem runFinalizers_append {σ : Type} (fins₁ fins₂ : List (σ → σ)) (s : σ) :
    runFinalizers (fins₁ ++ fins₂) s = runFinalizers fins₁ (runFinalizers fins₂ s) := by
  simp [runFinalizers, List.foldr_append]

/-- LIFO composition law: finalizers registered later (`fins₂`) execute strictly before
finalizers registered earlier (`fins₁`). -/
theorem scopeFinalize_order {ι σ α β ε : Type} (fins₁ fins₂ : List (σ → σ))
    (c : LabeledCompletion ι σ α β ε) :
    scopeFinalize (fins₁ ++ fins₂) c = scopeFinalize fins₁ (scopeFinalize fins₂ c) := by
  simp [scopeFinalize, runFinalizers_append]

theorem scopeFinalize_snoc {ι σ α β ε : Type} (fins : List (σ → σ)) (f : σ → σ)
    (c : LabeledCompletion ι σ α β ε) :
    scopeFinalize (fins ++ [f]) c = scopeFinalize fins (c.withStore (f c.store)) := by
  simp [scopeFinalize, runFinalizers]

theorem scopeFinalize_cons {ι σ α β ε : Type} (f : σ → σ) (fins : List (σ → σ))
    (c : LabeledCompletion ι σ α β ε) :
    scopeFinalize (f :: fins) c = (scopeFinalize fins c).withStore (f (scopeFinalize fins c).store) := by
  simp [scopeFinalize, runFinalizers]

/-- Execution of finalizers over an explicit event log proves LIFO ordering:
if finalizers each append an identifier to the store log, they appear in reverse registration order. -/
theorem scopeFinalize_event_lifo {ι α β ε : Type} (evs : List Nat)
    (c : LabeledCompletion ι (List Nat) α β ε) (h_empty : c.store = []) :
    (scopeFinalize (evs.map (fun id log => log ++ [id])) c).store = evs.reverse := by
  simp [scopeFinalize, runFinalizers, h_empty]
  induction evs with
  | nil => rfl
  | cons x xs ih =>
    simp [List.foldr_cons, ih]

/-- Scope finalization preserves store invariants preserved by every registered finalizer. -/
theorem scopeFinalize_preserves {ι σ α β ε : Type} (P : σ → Prop) (fins : List (σ → σ))
    (c : LabeledCompletion ι σ α β ε)
    (h_fins : ∀ f ∈ fins, ∀ s, P s → P (f s))
    (h_init : P c.store) :
    P (scopeFinalize fins c).store := by
  simp [scopeFinalize]
  induction fins with
  | nil =>
    simp [runFinalizers]
    exact h_init
  | cons f fs ih =>
    simp [runFinalizers]
    apply h_fins f (by simp)
    apply ih
    intro g hg s hs
    exact h_fins g (by simp [hg]) s hs

variable {ι : Type} [DecidableEq ι]

/-- Fuel-bounded execution of labeled loops with explicit target matching.
Continues or breaks matching `lbl` are resolved locally; exits targeting outer loops
or unhandled errors propagate unchanged. -/
def iterateL {σ β ε : Type} (lbl : ι) :
    Nat → (σ → LabeledCompletion ι σ Unit β ε) → σ → Option (LabeledCompletion ι σ β β ε)
  | 0, _, _ => none
  | n + 1, body, s => match body s with
    | .normal _ s' => iterateL lbl n body s'
    | .continue_ target s' =>
      if target = lbl then
        iterateL lbl n body s'
      else
        some (.continue_ target s')
    | .break_ target v s' =>
      if target = lbl then
        some (.normal v s' : LabeledCompletion ι σ β β ε)
      else
        some (.break_ target v s')
    | .raised e s' => some (.raised e s')

/-- Store invariant preservation across all finite executions and early exit points. -/
theorem iterateL_preserves {σ β ε : Type} (lbl : ι) (P : σ → Prop)
    (body : σ → LabeledCompletion ι σ Unit β ε)
    (step : ∀ s, P s → P (body s).store) (n : Nat) (s : σ) (initial : P s)
    (c : LabeledCompletion ι σ β β ε) (run : iterateL lbl n body s = some c) : P c.store := by
  induction n generalizing s with
  | zero => simp [iterateL] at run
  | succ n ih =>
    have safe := step s initial
    cases hb : body s with
    | normal v s' =>
      simp [hb, LabeledCompletion.store] at safe
      simp [iterateL, hb] at run
      exact ih s' safe run
    | continue_ target s' =>
      simp [hb, LabeledCompletion.store] at safe
      simp [iterateL, hb] at run
      split at run
      · exact ih s' safe run
      · cases run
        simpa [hb, LabeledCompletion.store] using safe
    | break_ target v s' =>
      simp [hb, LabeledCompletion.store] at safe
      simp [iterateL, hb] at run
      split at run
      · cases run
        simpa [hb, LabeledCompletion.store] using safe
      · cases run
        simpa [hb, LabeledCompletion.store] using safe
    | raised e s' =>
      simp [hb, LabeledCompletion.store] at safe
      simp [iterateL, hb] at run
      cases run
      simpa [hb, LabeledCompletion.store] using safe

@[simp] theorem iterateL_step_break_match {σ β ε : Type} (lbl : ι) (n : Nat)
    (body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (v : β) (s' : σ)
    (hb : body s = .break_ lbl v s') :
    iterateL lbl (n + 1) body s = some (.normal v s') := by
  simp [iterateL, hb]

@[simp] theorem iterateL_step_break_miss {σ β ε : Type} (lbl target : ι) (n : Nat)
    (body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (v : β) (s' : σ)
    (hb : body s = .break_ target v s') (hne : target ≠ lbl) :
    iterateL lbl (n + 1) body s = some (.break_ target v s') := by
  simp [iterateL, hb, hne]

@[simp] theorem iterateL_step_continue_match {σ β ε : Type} (lbl : ι) (n : Nat)
    (body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (s' : σ)
    (hb : body s = .continue_ lbl s') :
    iterateL lbl (n + 1) body s = iterateL lbl n body s' := by
  simp [iterateL, hb]

@[simp] theorem iterateL_step_continue_miss {σ β ε : Type} (lbl target : ι) (n : Nat)
    (body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (s' : σ)
    (hb : body s = .continue_ target s') (hne : target ≠ lbl) :
    iterateL lbl (n + 1) body s = some (.continue_ target s') := by
  simp [iterateL, hb, hne]

@[simp] theorem iterateL_step_raised {σ β ε : Type} (lbl : ι) (n : Nat)
    (body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (e : ε) (s' : σ)
    (hb : body s = .raised e s') :
    iterateL lbl (n + 1) body s = some (.raised e s') := by
  simp [iterateL, hb]

/-- Nested exit propagation: breaking an outer loop from within an inner loop
bubbles through the inner loop unchanged and terminates the outer loop. -/
theorem nested_loop_break_outer {σ β ε : Type} (lbl_out lbl_in : ι) (n m : Nat)
    (inner_body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (v : β) (s' : σ)
    (hb : inner_body s = .break_ lbl_out v s') (hne : lbl_out ≠ lbl_in) :
    iterateL lbl_in (m + 1) inner_body s = some (.break_ lbl_out v s') ∧
    iterateL lbl_out (n + 1) (fun _ => (.break_ lbl_out v s' : LabeledCompletion ι σ Unit β ε)) s =
      some (.normal v s') := by
  constructor
  · simp [iterateL, hb, hne]
  · simp [iterateL]

/-- Multi-level exit propagation across arbitrary ancestors: an exit targeted at an
outer ancestor bypasses both inner and enclosing middle loops. -/
theorem nested_loop_break_ancestor {σ β ε : Type} (lbl_anc lbl_mid lbl_in : ι) (n m : Nat)
    (inner_body : σ → LabeledCompletion ι σ Unit β ε) (s : σ) (v : β) (s' : σ)
    (hb : inner_body s = .break_ lbl_anc v s')
    (h_in : lbl_anc ≠ lbl_in) (h_mid : lbl_anc ≠ lbl_mid) :
    iterateL lbl_in (m + 1) inner_body s = some (.break_ lbl_anc v s') ∧
    iterateL lbl_mid (n + 1) (fun _ => (.break_ lbl_anc v s' : LabeledCompletion ι σ Unit β ε)) s =
      some (.break_ lbl_anc v s') := by
  constructor
  · simp [iterateL, hb, h_in]
  · simp [iterateL, h_mid]

/-- Scope finalization combined with labeled break matching at target loop. -/
theorem iterateL_scopeFinalize_break_match {σ β ε : Type} (lbl : ι) (n : Nat)
    (fins : List (σ → σ)) (v : β) (s : σ) :
    iterateL lbl (n + 1) (fun _ => scopeFinalize fins (.break_ lbl v s : LabeledCompletion ι σ Unit β ε)) s =
      some (.normal v (runFinalizers fins s)) := by
  simp [iterateL]

/-- Scope finalization combined with labeled break propagation across outer loops. -/
theorem iterateL_scopeFinalize_break_miss {σ β ε : Type} (lbl target : ι) (n : Nat)
    (fins : List (σ → σ)) (v : β) (s : σ) (hne : target ≠ lbl) :
    iterateL lbl (n + 1) (fun _ => scopeFinalize fins (.break_ target v s : LabeledCompletion ι σ Unit β ε)) s =
      some (.break_ target v (runFinalizers fins s)) := by
  simp [iterateL, hne]

end Flick.LocalControl
