import Formal.Held

/-!
# Checks of the live typing contract

These are checks of the rules, not the preservation or region-safety theorem.
-/

namespace Flick.LiveChecks

def region : Caps := fun a => a = .r 0
def emptyStage : Stage := .rt Row.none Row.none
def qualified : Ty := .arrow (.hnd (.rg 0)) emptyStage (.base .unit) region
def unqualified : Ty := .arrow (.hnd (.rg 0)) emptyStage (.base .unit) Caps.none

theorem use_none_le (r ε : Row) :
    (TypingMode.live.use (.rt r ε) Caps.none).le (.rt r ε) := by
  refine ⟨?_, fun _ h => h⟩
  intro x h
  rcases h with h | h
  · exact h
  · cases x <;> exact h.elim

/-- In captures mode the parameter shows the region, so the qualifier may disappear. -/
theorem captures_erases : Le qualified unqualified := by
  refine .arrow .refl ⟨fun _ h => h, fun _ h => h⟩ .refl ?_
  intro a h
  rcases h with h | h | h
  · exact .inl h
  · exact h.elim
  · exact .inl h

/-- Under live, showing the region in a parameter must not erase its use obligation. -/
theorem live_keeps_use : ¬ TypingMode.live.subtype qualified unqualified := by
  intro h
  have hstage := h.arrow_use
  exact (hstage.1 (.atm (.r 0)) (Or.inr rfl)).elim id id

/-- An effectful task cannot lose its qualifier just because its result shows the region. -/
example : ¬ LeLive (.eff Row.none Row.none (.hnd (.rg 0)) region)
    (.eff Row.none Row.none (.hnd (.rg 0)) Caps.none) := by
  intro h
  exact (h.eff_use.1 (.atm (.r 0)) (Or.inr rfl)).elim id id

/-- A qualifier may instead move into the runtime row; this is not a ban on widening. -/
example : LeLive qualified
    (.arrow (.hnd (.rg 0)) (.rt (Row.ofCaps region) Row.none) (.base .unit) Caps.none) := by
  refine .arrow .refl ⟨?_, fun _ h => h⟩ .refl ?_
  · intro x hx
    exact hx.elim (fun h => h.elim) Or.inl
  · intro a ha
    exact ha.elim Or.inl (fun h => h.elim (fun h => h.elim) Or.inl)

/-- Pure use stays pure, even for a qualified value. -/
example (C : Caps) : TypingMode.live.use .pure C = .pure := rfl

def holdsOnly : Tm := .lam (.let_ (.cap 0) (.lit .unit))
def thunk : Ty := .arrow (.base .unit) .pure (.base .unit) Caps.none

/-- The value really holds the capability; the test is not a closure that ignores it syntactically. -/
theorem holdsOnly_holds : held [] [] holdsOnly (.r 0) := by
  exact .inr (.inl ⟨0, by simp [holdsOnly, Tm.Has], rfl⟩)

/-- Holding without use is accepted without a qualifier. -/
theorem holdsOnly_live (S : Sig) : TypLive S [] Facts.none [] holdsOnly (.ex thunk .pure) := by
  exact .lam (.let_ (a := .hnd (.rg 0)) .cap .lit)
    (by simp [Tm.NoIns, Tm.Has]) trivial .refl

/-- The same unqualified closure remains rejected by the capture discipline. -/
theorem holdsOnly_not_captures (S : Sig) : ¬ Typ S [] Facts.none [] holdsOnly (.ex thunk .pure) := by
  intro h
  have hr := (h.holds .lam rfl).1 _ holdsOnly_holds
  simp [thunk, Ty.reach, Caps.none] at hr

def scopeHold : Tm :=
  .scope (.let_ (.app (.const .open_) (.var 0)) (.lam (.let_ (.var 1) (.lit .unit))))

/-- The source path acquires a capability, holds it in a closure, and leaves its scope. -/
theorem scopeHold_live (S : Sig) :
    TypLive S [] Facts.none [] scopeHold (.ex thunk emptyStage) := by
  refine .scope (F := Facts.none) (t := thunk) (r := Row.none) (ε := Row.none)
    (φ' := .rt (Row.scope (.tvar 0)) Row.none)
    (by simp [Tm.NoIns, Tm.Has]) (fun _ _ h => h.elim) ?_
    ⟨fun _ h => .inl h, fun _ h => h.elim⟩
    ⟨fun _ h => h, fun _ h => h⟩ .refl
  refine .let_ (a := .hnd (.tvar 0)) ?_ ?_
  · refine .app
      (.const (c := .open_) (ι := { (default : CArg) with s := .tvar 0 }) trivial .refl)
      (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) ?_ .refl
    exact use_none_le _ _
  · exact .lam
      (.let_ (a := .hnd (.tvar 0)) (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) .lit)
      (by simp [Tm.NoIns, Tm.Has]) trivial .refl

/-- Holding without use works for tasks and recursive functions too. -/
example (S : Sig) :
    TypLive S [] Facts.none [] (.task (.let_ (.cap 0) (.lit .unit)))
      (.ex (.eff Row.none Row.none (.base .unit) Caps.none) .pure) := by
  exact .task (ψ := .pure) (.let_ (a := .hnd (.rg 0)) .cap .lit) trivial
    (by simp [Tm.NoIns, Tm.Has]) trivial .refl

example (S : Sig) :
    TypLive S [] Facts.none [] (.fix (.let_ (.cap 0) (.lit .unit))) (.ex thunk .pure) := by
  exact .fix (.let_ (a := .hnd (.rg 0)) .cap .lit) (by simp [Tm.NoIns, Tm.Has]) trivial .refl

example (S : Sig) :
    TypLive S [] Facts.none [] (.letrec (.let_ (.cap 0) (.lit .unit)) (.lit .unit))
      (.ex (.base .unit) .pure) := by
  exact .letrec (a := .base .unit) (ψ := .pure) (r := .base .unit) (C := Caps.none)
    (.let_ (a := .hnd (.rg 0)) .cap .lit) (by simp [Tm.NoIns, Tm.Has]) trivial .lit

/-- Dictionary methods can hold without use, although a dictionary type has no reach. -/
example (S : Sig) (hc : S.dict 0 = some (0, [("f", Scheme.mono thunk)])) :
    TypLive S [] Facts.none [] (.drec 0 (.rcons "f" holdsOnly .rnil))
      (.ex (.dict 0 (Ty.spine [])) .pure) := by
  refine .drec (us := []) hc rfl ?_ (by simp [holdsOnly, Tm.NoIns, Tm.Has]) trivial
  have hf : (Scheme.mono thunk).substT (instT []) = Scheme.mono thunk := by
    simp [Scheme.substT, Scheme.mono, thunk, Ty.substT, Stage.substT]
  simp only [List.map_cons, List.map_nil, hf]
  exact .dcons (s := .mono thunk)
    (.lam (.let_ (a := .hnd (.rg 0)) .cap .lit) (by simp [Tm.NoIns, Tm.Has]) trivial .refl) .dnil

/-- A qualified body may use its region even when the bare arrow row is empty. -/
example (S : Sig) :
    TypLive S [] Facts.none [] (.lam (.app (.const .op) (.cap 0)))
      (.ex (.arrow (.base .unit) emptyStage (.base .unit) region) .pure) := by
  refine .lam ?_ (by simp [Tm.NoIns, Tm.Has]) trivial .refl
  refine .app
    (.const (c := .op) (ι := { (default : CArg) with s := .rg 0 }) trivial .refl) .cap
    ⟨?_, fun _ h => h⟩ .refl
  intro x h
  rcases h with h | h
  · exact .inr h
  · cases x <;> exact h.elim

/-- The source path acquires and uses its capability inside the scope, discharging Live. -/
example (S : Sig) :
    TypLive S [] Facts.none [] (.scope (.app (.const .op) (.app (.const .open_) (.var 0))))
      (.ex (.base .unit) emptyStage) := by
  refine .scope (F := Facts.none) (r := Row.none) (ε := Row.none)
    (φ' := .rt (Row.scope (.tvar 0)) Row.none)
    (by simp [Tm.NoIns, Tm.Has]) (fun _ _ h => h.elim) ?_
    ⟨fun _ h => .inl h, fun _ h => h.elim⟩
    ⟨fun _ h => h, fun _ h => h⟩ .refl
  refine .app
    (.const (c := .op) (ι := { (default : CArg) with s := .tvar 0 }) trivial .refl) ?_
    (use_none_le _ _) .refl
  exact .app
    (.const (c := .open_) (ι := { (default : CArg) with s := .tvar 0 }) trivial .refl)
    (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
    (use_none_le _ _) .refl

/-- `op` cannot be coerced to a function whose use has an empty row. -/
example (S : Sig) :
    ¬ LeLive (Const.op.ty S { (default : CArg) with s := .rg 0 } .live) unqualified := by
  intro h
  exact (h.arrow_use.1 (.atm (.r 0)) (Or.inl rfl)).elim id id

/-- Both function calls and awaiting tasks with a qualifier need that region in the row. -/
example : ¬ (TypingMode.live.use emptyStage region).le emptyStage := by
  intro h
  exact h.1 (.atm (.r 0)) (Or.inr rfl)

/-- Live regions in a finalizer's work row need Outlives, even if its qualifier is empty. -/
example : ¬ Const.ok Facts.none .addFin
    { (default : CArg) with s := .rg 1, r := Row.ofCaps region } .live := by
  intro h
  rcases h.2 (.r 0) (Or.inl rfl) with h | ⟨_, h, _⟩
  · cases h
  · exact h.elim

/-- The same check applies to a child. -/
example : ¬ Const.ok Facts.none .spawn
    { (default : CArg) with s := .rg 1, r := Row.ofCaps region } .live := by
  intro h
  rcases h.2 (.r 0) (Or.inl rfl) with h | ⟨_, h, _⟩
  · cases h
  · exact h.elim

/-- Work using its own region is allowed. -/
example : Const.ok Facts.none .spawn
    { (default : CArg) with s := .rg 0, r := Row.ofCaps region } .live := by
  refine ⟨?_, ?_⟩
  · intro a h
    exact h.elim (fun h => h.elim) (fun h => h.elim)
  · intro a h
    exact .inl (h.elim id False.elim)

/-- A control arm may use its own resume, without leaking the handler's region into its outer row. -/
example (S : Sig) (hop : S.op "E" "op" = some (.control, .base .unit, .base .unit)) :
    TypLive S [] Facts.none []
      (.acons (Pat.spine "op" [.var, .var]) (.app (.var 1) (.lit .unit)) .anil)
      (.harms "E" (.base .unit) emptyStage (.tvar 0)) := by
  refine .hactl hop ?_ .hanil
  exact .app (.var (T := .arrow (.base .unit) emptyStage (.base .unit) (Ty.tvar 0).reach)
      (ts := []) rfl rfl (fun _ _ h => h.elim) (by rw [Scheme.inst_mono]; exact .refl)) .lit
    ⟨fun _ h => h, fun _ h => h⟩ .refl

end Flick.LiveChecks

namespace Flick.LiveChecks

/-- A row variable may later contain Live, so it cannot bypass an owner's bound. -/
example : ¬ TypingMode.live.keep Facts.none (fun x => x = .rv (.v 0)) (.rg 1) := by
  intro h
  rcases h (.v 0) (Or.inr rfl) with h | ⟨_, h, _⟩
  · cases h
  · exact h.elim

/-- `Scope.run` of a function whose qualifier names a region not in the rows around the scope
is rejected in the live discipline; the capture discipline asks nothing of the constant. -/
example : ¬ Const.ok Facts.none .scopeRun { (default : CArg) with C := fun a => a = .r 0 } .live :=
  fun h => h (.atm (.r 0)) rfl

example : Const.ok Facts.none .scopeRun
    { (default : CArg) with C := fun a => a = .r 0, r := Row.ofCaps fun a => a = .r 0 } .live :=
  fun _ h => h

example : Const.ok Facts.none .scopeRun { (default : CArg) with C := fun a => a = .r 0 } .captures :=
  trivial

end Flick.LiveChecks
