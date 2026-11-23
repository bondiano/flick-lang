import Std

/-! # Ownership token checking and selected-path safety -/

namespace Flick.Memory.Pass

abbrev Local (width : Nat) := Fin width

structure State (width : Nat) where
  tokens : Local width → Nat
  defined : Local width → Bool

variable {width : Nat} {α : Type}

def initial : State width := ⟨fun _ => 0, fun _ => false⟩

def update (values : Local width → α) (name : Local width) (value : α) : Local width → α :=
  fun other => if other == name then value else values other

/-- Micro-events checked by the ownership validator, including borrowed reads. -/
inductive Event (width : Nat) where
  | take (name : Local width)
  | dup (name : Local width)
  | borrow (name : Local width)
  | bind (name : Local width)
  deriving DecidableEq, Repr

/-- A consumed operand list may contain the same local more than once. -/
inductive Operation (width : Nat) where
  | consume (operands : List (Local width))
  | borrow (receiver : Local width)
  | construct (fields evidence : List (Local width))
  | record (fields : List (Local width))
  | dropReuse (object : Local width)
  | reuse (receiver : Local width) (fields evidence : List (Local width))
  | reuseRecord (receiver : Local width) (fields : List (Local width))
  deriving DecidableEq, Repr

inductive Instruction (width : Nat) where
  | let_ (binding : Local width) (value : Operation width)
  | dup (name : Local width)
  | drop (name : Local width)
  deriving DecidableEq, Repr

/-- Nested sequential nodes represent a block's instruction vector. -/
inductive Block (width : Nat) where
  | ret (result : Local width)
  | seq (instruction : Instruction width) (next : Block width)
  | branch (condition : Local width) (yes no : Block width)
  deriving DecidableEq, Repr

def operationEvents : Operation width → List (Event width)
  | .consume operands => operands.map .take
  | .borrow receiver => [.borrow receiver]
  | .construct fields evidence => (fields ++ evidence).map .take
  | .record fields => fields.map .take
  | .dropReuse object => [.take object]
  | .reuse receiver fields evidence => (receiver :: (fields ++ evidence)).map .take
  | .reuseRecord receiver fields => (receiver :: fields).map .take

def instructionEvents : Instruction width → List (Event width)
  | .let_ binding value => operationEvents value ++ [.bind binding]
  | .dup name => [.dup name]
  | .drop name => [.take name]

def required : Event width → Option (Local width)
  | .take name | .dup name | .borrow name => some name
  | .bind _ => none

/-- All reads, copies and consuming uses require a token. Binding freshness
is separate from current ownership: dropping a local does not undefine it. -/
def step (event : Event width) (state : State width) : Option (State width) :=
  match event with
  | .take name =>
      if state.tokens name == 0 then none
      else some { state with tokens := update state.tokens name (state.tokens name - 1) }
  | .dup name =>
      if state.tokens name == 0 then none
      else some { state with tokens := update state.tokens name (state.tokens name + 1) }
  | .borrow name => if state.tokens name == 0 then none else some state
  | .bind name =>
      if state.defined name then none
      else some {
        tokens := update state.tokens name 1
        defined := update state.defined name true
      }

/-- Failure cannot be repaired by later events. -/
def runEvents : List (Event width) → State width → Option (State width)
  | [], state => some state
  | event :: rest, state => (step event state).bind (runEvents rest)

/-- Successful reads, copies and consumption cannot rely on truncated
subtraction from zero: their required token count is strictly positive. -/
theorem step_requires_token (event : Event width) (state next : State width)
    (success : step event state = some next) :
    ∀ name, required event = some name → 0 < state.tokens name := by
  cases event with
  | take name =>
      have nonzero : state.tokens name ≠ 0 := by
        intro zero
        simp [step, zero] at success
      intro other same
      simp only [required, Option.some.injEq] at same
      subst other
      omega
  | dup name =>
      have nonzero : state.tokens name ≠ 0 := by
        intro zero
        simp [step, zero] at success
      intro other same
      simp only [required, Option.some.injEq] at same
      subst other
      omega
  | borrow name =>
      have nonzero : state.tokens name ≠ 0 := by
        intro zero
        simp [step, zero] at success
      intro other same
      simp only [required, Option.some.injEq] at same
      subst other
      omega
  | bind name => simp [required]

theorem step_bind_fresh (name : Local width) (state next : State width)
    (success : step (.bind name) state = some next) : state.defined name = false := by
  cases fresh : state.defined name with
  | false => rfl
  | true => simp [step, fresh] at success

def emptyTokens (state : State width) : Bool :=
  (List.finRange width).all (fun name => state.tokens name == 0)

/-- Independent token-only validation. Every selected or unselected
successor must validate from the same post-condition-consumption state. -/
def checkTokens : Block width → State width → Bool
  | .ret result, state =>
      match runEvents [.take result] state with
      | none => false
      | some next => emptyTokens next
  | .seq instruction next, state =>
      match runEvents (instructionEvents instruction) state with
      | none => false
      | some after => checkTokens next after
  | .branch condition yes no, state =>
      match runEvents [.take condition] state with
      | none => false
      | some after => checkTokens yes after && checkTokens no after

/-- The annotated micro-events along one runtime path. Missing decisions
choose false; surplus decisions are irrelevant after Return. All choices
are quantified by the theorem, including choices differing from literals. -/
def path : Block width → List Bool → List (Event width)
  | .ret result, _ => [.take result]
  | .seq instruction next, choices => instructionEvents instruction ++ path next choices
  | .branch condition _ no, [] => .take condition :: path no []
  | .branch condition yes no, choice :: choices =>
      if choice then .take condition :: path yes choices
      else .take condition :: path no choices

theorem runEvents_append (before after : List (Event width)) (state : State width) :
    runEvents (before ++ after) state = (runEvents before state).bind (runEvents after) := by
  induction before generalizing state with
  | nil => simp [runEvents]
  | cons event rest ih =>
      simp only [List.cons_append, runEvents]
      cases step event state <;> simp [ih]

theorem emptyTokens_iff (state : State width) :
    emptyTokens state = true ↔ ∀ name, state.tokens name = 0 := by
  simp [emptyTokens, List.all_eq_true]

/-- Token-checker acceptance entails successful execution of any selected
path, with no local tokens left after transferring the return token. -/
theorem checkTokens_sound (block : Block width) :
    ∀ state, checkTokens block state = true → ∀ choices,
      ∃ final, runEvents (path block choices) state = some final ∧
        ∀ name, final.tokens name = 0 := by
  induction block with
  | ret result =>
      intro state accepted choices
      cases h : runEvents [.take result] state with
      | none => simp [checkTokens, h] at accepted
      | some final =>
          refine ⟨final, h, ?_⟩
          exact (emptyTokens_iff final).mp (by simpa [checkTokens, h] using accepted)
  | seq instruction next ih =>
      intro state accepted choices
      cases h : runEvents (instructionEvents instruction) state with
      | none => simp [checkTokens, h] at accepted
      | some after =>
          have acceptedNext : checkTokens next after = true := by
            simpa [checkTokens, h] using accepted
          obtain ⟨final, executed, empty⟩ := ih after acceptedNext choices
          refine ⟨final, ?_, empty⟩
          simp only [path, runEvents_append, h, Option.bind_some]
          exact executed
  | branch condition yes no ihYes ihNo =>
      intro state accepted choices
      cases h : runEvents [.take condition] state with
      | none => simp [checkTokens, h] at accepted
      | some after =>
          have acceptedBoth : checkTokens yes after = true ∧ checkTokens no after = true := by
            simpa [checkTokens, h] using accepted
          have extend (events : List (Event width)) (final : State width)
              (executed : runEvents events after = some final) :
              runEvents (.take condition :: events) state = some final := by
            change runEvents ([.take condition] ++ events) state = some final
            simpa only [runEvents_append, h, Option.bind_some] using executed
          cases choices with
          | nil =>
              obtain ⟨final, executed, empty⟩ := ihNo after acceptedBoth.2 []
              exact ⟨final, extend _ _ executed, empty⟩
          | cons choice rest =>
              cases choice with
              | false =>
                  obtain ⟨final, executed, empty⟩ := ihNo after acceptedBoth.2 rest
                  exact ⟨final, extend _ _ executed, empty⟩
              | true =>
                  obtain ⟨final, executed, empty⟩ := ihYes after acceptedBoth.1 rest
                  exact ⟨final, extend _ _ executed, empty⟩

/-- Every event prefix succeeds; an absent token or a duplicate binding
cannot be concealed by a later event or an unselected runtime branch. -/
theorem checkTokens_prefix_safe (block : Block width) (state : State width)
    (accepted : checkTokens block state = true) (choices : List Bool)
    (before after : List (Event width)) (split : path block choices = before ++ after) :
    ∃ next, runEvents before state = some next := by
  obtain ⟨final, executed, _⟩ := checkTokens_sound block state accepted choices
  rw [split, runEvents_append] at executed
  cases h : runEvents before state with
  | none => simp [h] at executed
  | some next => exact ⟨next, rfl⟩

/-- Every individual event on a selected accepted path has an actual
successful transition. `step_requires_token` and `step_bind_fresh` apply
to these states, not just to a count after the whole path has finished. -/
theorem checkTokens_event_safe (block : Block width) (state : State width)
    (accepted : checkTokens block state = true) (choices : List Bool)
    (before after : List (Event width)) (event : Event width)
    (split : path block choices = before ++ event :: after) :
    ∃ enter leave,
      runEvents before state = some enter ∧ step event enter = some leave := by
  obtain ⟨final, executed, _⟩ := checkTokens_sound block state accepted choices
  rw [split, runEvents_append] at executed
  cases first : runEvents before state with
  | none => simp [first] at executed
  | some enter =>
      have tailRun : runEvents (event :: after) enter = some final := by
        simpa [first] using executed
      cases transition : step event enter with
      | none => simp [runEvents, transition] at tailRun
      | some leave => exact ⟨enter, leave, rfl, transition⟩

/-- Parameter binding is the same fresh one-token initialization used by
the Rust validator; duplicate parameter names are rejected. -/
def checkFunction (parameters : List (Local width)) (block : Block width) : Bool :=
  match runEvents (parameters.map .bind) initial with
  | none => false
  | some state => checkTokens block state

theorem checkFunction_sound (parameters : List (Local width)) (block : Block width)
    (accepted : checkFunction parameters block = true) (choices : List Bool) :
    ∃ final,
      runEvents (parameters.map .bind ++ path block choices) initial = some final ∧
      ∀ name, final.tokens name = 0 := by
  cases h : runEvents (parameters.map .bind) initial with
  | none => simp [checkFunction, h] at accepted
  | some state =>
      have acceptedBlock : checkTokens block state = true := by
        simpa [checkFunction, h] using accepted
      obtain ⟨final, executed, empty⟩ := checkTokens_sound block state acceptedBlock choices
      refine ⟨final, ?_, empty⟩
      simpa only [runEvents_append, h, Option.bind_some] using executed

theorem checkFunction_prefix_safe (parameters : List (Local width)) (block : Block width)
    (accepted : checkFunction parameters block = true) (choices : List Bool)
    (before after : List (Event width))
    (split : parameters.map .bind ++ path block choices = before ++ after) :
    ∃ next, runEvents before initial = some next := by
  obtain ⟨final, executed, _⟩ := checkFunction_sound parameters block accepted choices
  rw [split, runEvents_append] at executed
  cases h : runEvents before initial with
  | none => simp [h] at executed
  | some next => exact ⟨next, rfl⟩

-- Executable boundary cases. Numeric locals in these examples inhabit
-- `Fin 4`; the theorems above quantify over arbitrary finite width.

-- An ordinary field and an evidence field both own the repeated operand.
example : checkFunction (width := 4) [0]
    (.seq (.dup 0) (.seq (.let_ 1 (.construct [0] [0])) (.ret 1))) = true := by decide

-- A borrow keeps its receiver token until the following explicit drop.
example : checkFunction (width := 4) [0]
    (.seq (.let_ 1 (.borrow 0)) (.seq (.drop 0) (.ret 1))) = true := by decide

-- Dropping a dead match/project result does not consume the pending receiver.
example : checkFunction (width := 4) [0]
    (.seq (.let_ 1 (.borrow 0))
      (.seq (.drop 1) (.seq (.let_ 2 (.reuse 0 [] [])) (.ret 2)))) = true := by decide

-- Reuse consumes its candidate and every ordinary/evidence operand, even
-- when the same local appears as candidate and evidence.
example : checkFunction (width := 4) [0]
    (.seq (.dup 0) (.seq (.let_ 1 (.reuse 0 [] [0])) (.ret 1))) = true := by decide
example : checkFunction (width := 4) [0]
    (.seq (.let_ 1 (.reuse 0 [] [0])) (.ret 1)) = false := by decide

-- The condition remains live on only one successor. The other drops it.
example : checkFunction (width := 4) [0]
    (.seq (.dup 0) (.branch 0 (.ret 0)
      (.seq (.drop 0) (.seq (.let_ 1 (.consume [])) (.ret 1))))) = true := by decide

-- Separate successors may bind the same fresh name: freshness is per path.
example : checkFunction (width := 4) [0]
    (.branch 0 (.seq (.let_ 1 (.consume [])) (.ret 1))
      (.seq (.let_ 1 (.consume [])) (.ret 1))) = true := by decide

-- Operand multiplicity, missing tokens, and stale receiver reads are checked.
example : checkFunction (width := 4) [0]
    (.seq (.let_ 1 (.construct [0] [0])) (.ret 1)) = false := by decide
example : checkFunction (width := 4) [0]
    (.seq (.drop 0) (.seq (.dup 0) (.ret 0))) = false := by decide
example : checkFunction (width := 4) [0]
    (.seq (.drop 0) (.seq (.let_ 1 (.borrow 0)) (.ret 1))) = false := by decide

-- Rebinding stays forbidden after release, as do duplicate parameters.
example : checkFunction (width := 4) [0]
    (.seq (.drop 0) (.seq (.let_ 0 (.consume [])) (.ret 0))) = false := by decide
example : checkFunction (width := 4) [0, 0] (.ret 0) = false := by decide

-- A leaked local and an invalid unselected successor each reject the block.
example : checkFunction (width := 4) [0]
    (.seq (.let_ 1 (.consume [])) (.ret 1)) = false := by decide
example : checkFunction (width := 4) [0]
    (.branch 0 (.seq (.let_ 1 (.consume [])) (.ret 1)) (.ret 0)) = false := by decide

end Flick.Memory.Pass
