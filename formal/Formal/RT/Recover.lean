import Formal.RT.Cause

/-!
# λRT: recovery and the typed head (runtime.md §2.3–§2.4, R5)

A row `ε` and the judgment `· ⊢ v : Errors ε` of the core are abstracted to a predicate
`Has : V → Prop` per row. The premise of (Try) of the core, "the remainder `ε'` holds every
error of the body that no arm covers", is `Covers`.
-/

namespace Flick.RT

variable {V T : Type}

/-- Some reason of `c` is a defect. -/
def hasDie (c : Cause V) : Bool := c.any fun | .die _ => true | _ => false

/-- `dies c`: every `Fail v` becomes `Die v` (`failsToDies`, Р81). -/
def dies (c : Cause V) : Cause V := c.map fun | .fail v => .die v | r => r

/-- The result of `recover`: an arm's handler or a failure. -/
inductive Recovered (T V : Type) where
  | handled (t : T)
  | fail (c : Cause V)

/-- `recover(c, ā)` of §2.3; `arms v` is the handler of the arm that `v` matches. -/
def recover (arms : V → Option T) (c : Cause V) : Recovered T V :=
  if hasDie c then .fail (if (head c).isSome then dies c else c)
  else match head c with
    | some v => match arms v with
      | some t => .handled t
      | none => .fail c
    | none => .fail c

theorem head_dies (c : Cause V) : head (dies c) = none := by
  induction c with
  | nil => rfl
  | cons r c ih => cases r <;> simpa [dies, head] using ih

/-- **I_ε** (§2.4): if the cause has a `Fail`, its head has type `Errors ε`. -/
def Iε (Has : V → Prop) (c : Cause V) : Prop := ∀ v, head c = some v → Has v

/-- The premise of (Try): an error of `ε` that no arm covers is in the remainder `ε'`. -/
def Covers (Has Has' : V → Prop) (arms : V → Option T) : Prop :=
  ∀ v, Has v → arms v = none → Has' v

/-- **R5** (runtime.md §4). A `try` typed with remainder `ε'` that fails with `c'` gives
`c'` a head of type `Errors ε'`; the head is never a value some arm covers. -/
theorem R5 {Has Has' : V → Prop} {arms : V → Option T} {c c' : Cause V}
    (hc : Iε Has c) (hcov : Covers Has Has' arms) (hr : recover arms c = .fail c') :
    Iε Has' c' ∧ ∀ v, head c' = some v → arms v = none := by
  have none_ok : head c' = none → Iε Has' c' ∧ ∀ v, head c' = some v → arms v = none :=
    fun h => ⟨fun _ hv => (by rw [h] at hv; cases hv), fun _ hv => (by rw [h] at hv; cases hv)⟩
  cases hd : hasDie c
  · cases hh : head c with
    | none => simp [recover, hd, hh] at hr; subst hr; exact none_ok hh
    | some v =>
      cases ha : arms v with
      | some t => simp [recover, hd, hh, ha] at hr
      | none =>
        simp [recover, hd, hh, ha] at hr; subst hr
        refine ⟨fun w hw => ?_, fun w hw => ?_⟩
        · rw [hh] at hw; cases hw; exact hcov v (hc v hh) ha
        · rw [hh] at hw; cases hw; exact ha
  · cases hh : head c with
    | none => simp [recover, hd, hh] at hr; subst hr; exact none_ok hh
    | some v => simp [recover, hd, hh] at hr; subst hr; exact none_ok (head_dies c)

variable [DecidableEq V]

/-- `⊕` at type `Failure (ε + ε')` keeps I_ε (§2.4). -/
theorem Iε_comb {H₁ H₂ H : V → Prop} (h₁ : ∀ v, H₁ v → H v) (h₂ : ∀ v, H₂ v → H v)
    {a b : Cause V} (ha : Iε H₁ a) (hb : Iε H₂ b) : Iε H (comb a b) := by
  intro v hv
  rw [head_comb] at hv
  cases e : head a with
  | some w => rw [e] at hv; cases hv; exact h₁ _ (ha _ e)
  | none => rw [e] at hv; exact h₂ _ (hb _ hv)

omit [DecidableEq V] in
/-- `⧺` likewise. -/
theorem Iε_append {H₁ H₂ H : V → Prop} (h₁ : ∀ v, H₁ v → H v) (h₂ : ∀ v, H₂ v → H v)
    {a b : Cause V} (ha : Iε H₁ a) (hb : Iε H₂ b) : Iε H (a ++ b) := by
  intro v hv
  rw [head_append] at hv
  cases e : head a with
  | some w => rw [e] at hv; cases hv; exact h₁ _ (ha _ e)
  | none => rw [e] at hv; exact h₂ _ (hb _ hv)

omit [DecidableEq V] in
/-- A cause without `Fail` (hooks and finalizers, Р16) satisfies every I_ε. -/
theorem Iε_none {H : V → Prop} {c : Cause V} (h : head c = none) : Iε H c :=
  fun _ hv => by rw [h] at hv; cases hv

/-- The result of a closing, `x ⊕ c_H ⊕ c_F` (C6), keeps the head of the body. -/
theorem Iε_close {H : V → Prop} {x cH cF : Cause V} (hx : Iε H x) (hH : head cH = none)
    (hF : head cF = none) : Iε H (comb (comb x cH) cF) :=
  Iε_comb (fun _ h => h) (fun _ h => h) (Iε_comb (fun _ h => h) (fun _ h => h) hx (Iε_none hH))
    (Iε_none hF)

omit [DecidableEq V] in
/-- An operation that keeps the head or drops it keeps I_ε; Г7(ж) is one (`Hyp.g7zh`). -/
theorem Iε_keep {H : V → Prop} {c c' : Cause V} (h : head c' = none ∨ head c' = head c)
    (hc : Iε H c) : Iε H c' := by
  rcases h with h | h
  · exact Iε_none h
  · intro v hv; rw [h] at hv; exact hc v hv

end Flick.RT
