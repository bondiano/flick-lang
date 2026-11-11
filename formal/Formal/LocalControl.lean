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
end Flick.LocalControl
