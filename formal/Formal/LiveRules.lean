import Formal.Held
import Formal.TySubstBase

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
    (Stage.use (.rt r ε) Caps.none).le (.rt r ε) := by
  refine ⟨?_, fun _ h => h⟩
  intro x h
  rcases h with h | h
  · exact h
  · cases x <;> exact h.elim

/-- Under live, showing the region in a parameter must not erase its use obligation. -/
theorem live_keeps_use : ¬ LeLive S qualified unqualified := by
  intro h
  have hstage := h.arrow_use
  exact (hstage.1 (.atm (.r 0)) (Or.inr rfl)).elim id id

/-- An effectful task cannot lose its qualifier just because its result shows the region. -/
example : ¬ LeLive S (.eff Row.none Row.none (.hnd (.rg 0)) region)
    (.eff Row.none Row.none (.hnd (.rg 0)) Caps.none) := by
  intro h
  exact (h.eff_use.1 (.atm (.r 0)) (Or.inr rfl)).elim id id

/-- A qualifier may instead move into the runtime row; this is not a ban on widening. -/
example : LeLive S qualified
    (.arrow (.hnd (.rg 0)) (.rt (Row.ofCaps region) Row.none) (.base .unit) Caps.none) := by
  refine .arrow .refl ⟨?_, fun _ h => h⟩ .refl ?_
  · intro x hx
    exact hx.elim (fun h => h.elim) Or.inl
  · intro a ha
    exact ha.elim Or.inl (fun h => h.elim (fun h => h.elim) Or.inl)

/-- Pure use stays pure, even for a qualified value. -/
example (C : Caps) : Stage.use .pure C = .pure := rfl

def holdsOnly : Tm := .lam (.let_ (.cap 0) (.lit .unit))
def thunk : Ty := .arrow (.base .unit) .pure (.base .unit) Caps.none

/-- The value really holds the capability; the test is not a closure that ignores it syntactically. -/
theorem holdsOnly_holds : held [] [] holdsOnly (.r 0) := by
  exact .inr (.inl ⟨0, by simp [holdsOnly, Tm.Has], rfl⟩)

/-- Holding without use is accepted without a qualifier. -/
theorem holdsOnly_live (S : Sig) : Typ S [] Facts.none [] holdsOnly (.ex thunk .pure) := by
  exact .lam (.let_ (a := .hnd (.rg 0)) .cap .lit)
    (by simp [Tm.NoIns, Tm.Has]) .refl

def scopeHold : Tm :=
  .scope (.let_ (.app (.const .open_) (.var 0)) (.lam (.let_ (.var 1) (.lit .unit))))

/-- The source path acquires a capability, holds it in a closure, and leaves its scope. -/
theorem scopeHold_live (S : Sig) :
    Typ S [] Facts.none [] scopeHold (.ex thunk emptyStage) := by
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
      (by simp [Tm.NoIns, Tm.Has]) .refl

/-- Holding without use works for tasks and recursive functions too. -/
example (S : Sig) :
    Typ S [] Facts.none [] (.task (.let_ (.cap 0) (.lit .unit)))
      (.ex (.eff Row.none Row.none (.base .unit) Caps.none) .pure) := by
  exact .task (ψ := .pure) (.let_ (a := .hnd (.rg 0)) .cap .lit) trivial
    (by simp [Tm.NoIns, Tm.Has]) .refl

example (S : Sig) :
    Typ S [] Facts.none [] (.fix (.let_ (.cap 0) (.lit .unit))) (.ex thunk .pure) := by
  exact .fix (.let_ (a := .hnd (.rg 0)) .cap .lit) (by simp [Tm.NoIns, Tm.Has]) .refl

example (S : Sig) :
    Typ S [] Facts.none [] (.letrec (.let_ (.cap 0) (.lit .unit)) (.lit .unit))
      (.ex (.base .unit) .pure) := by
  exact .letrec (a := .base .unit) (ψ := .pure) (r := .base .unit) (C := Caps.none)
    (.let_ (a := .hnd (.rg 0)) .cap .lit) (by simp [Tm.NoIns, Tm.Has]) .lit

/-- Dictionary methods can hold without use, although a dictionary type has no reach. -/
example (S : Sig) (hc : S.dict 0 = some (0, [("f", Scheme.mono thunk)])) :
    Typ S [] Facts.none [] (.drec 0 (.rcons "f" holdsOnly .rnil))
      (.ex (.dict 0 (Ty.spine [])) .pure) := by
  refine .drec (us := []) hc rfl ?_ (by simp [holdsOnly, Tm.NoIns, Tm.Has])
  have hf : (Scheme.mono thunk).substT (instT []) = Scheme.mono thunk := by
    simp [Scheme.substT, Scheme.mono, thunk, Ty.substT, Stage.substT]
  simp only [List.map_cons, List.map_nil, hf]
  exact .dcons (s := .mono thunk)
    (.lam (.let_ (a := .hnd (.rg 0)) .cap .lit) (by simp [Tm.NoIns, Tm.Has]) .refl) .dnil

/-- A qualified body may use its region even when the bare arrow row is empty. -/
example (S : Sig) :
    Typ S [] Facts.none [] (.lam (.app (.const .op) (.cap 0)))
      (.ex (.arrow (.base .unit) emptyStage (.base .unit) region) .pure) := by
  refine .lam ?_ (by simp [Tm.NoIns, Tm.Has]) .refl
  refine .app
    (.const (c := .op) (ι := { (default : CArg) with s := .rg 0 }) trivial .refl) .cap
    ⟨?_, fun _ h => h⟩ .refl
  intro x h
  rcases h with h | h
  · exact .inr h
  · cases x <;> exact h.elim

/-- The source path acquires and uses its capability inside the scope, discharging Live. -/
example (S : Sig) :
    Typ S [] Facts.none [] (.scope (.app (.const .op) (.app (.const .open_) (.var 0))))
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
    ¬ LeLive S (Const.op.ty S { (default : CArg) with s := .rg 0 }) unqualified := by
  intro h
  exact (h.arrow_use.1 (.atm (.r 0)) (Or.inl rfl)).elim id id

/-- Both function calls and awaiting tasks with a qualifier need that region in the row. -/
example : ¬ (Stage.use emptyStage region).le emptyStage := by
  intro h
  exact h.1 (.atm (.r 0)) (Or.inr rfl)

/-- Live regions in a finalizer's work row need Outlives, even if its qualifier is empty. -/
example : ¬ Const.ok Facts.none .addFin
    { (default : CArg) with s := .rg 1, r := Row.ofCaps region } := by
  intro h
  rcases h.2.1 (.r 0) (Or.inl rfl) with h | ⟨_, h, _⟩
  · cases h
  · exact h.elim

/-- The same check applies to a child. -/
example : ¬ Const.ok Facts.none .spawn
    { (default : CArg) with s := .rg 1, r := Row.ofCaps region } := by
  intro h
  rcases h.2.1 (.r 0) (Or.inl rfl) with h | ⟨_, h, _⟩
  · cases h
  · exact h.elim

/-- Work using its own region is allowed. -/
example : Const.ok Facts.none .spawn
    { (default : CArg) with s := .rg 0, r := Row.ofCaps region } := by
  refine ⟨?_, ?_, ?_⟩
  · intro a h
    exact h.elim (fun h => h.elim) (fun h => h.elim)
  · intro a h
    exact .inl (h.elim id fun ⟨_, h⟩ => h.elim)
  · intro y hy
    cases y <;> first | exact (hy : False).elim | simp [Row.ofCaps] at hy

/-- A child's qualifier is part of what it uses: a task that uses its owner's region but does
not show it in its row is rejected, though its owner outlives it. -/
example : ¬ Const.ok Facts.none .spawn { (default : CArg) with s := .rg 0, C := region } :=
  fun h => h.2.2 (.atm (.r 0)) rfl

example : Const.ok Facts.none .spawn
    { (default : CArg) with s := .rg 0, C := region, r := Row.ofCaps region } := by
  refine ⟨?_, ?_, fun _ h => h⟩
  · intro a h
    exact .inl (h.elim id fun h => h.elim)
  · intro a h
    exact .inl (h.elim id fun ⟨_, h⟩ => h.elim)

/-- The same holds of a finalizer. -/
example : ¬ Const.ok Facts.none .addFin { (default : CArg) with s := .rg 0, C := region } :=
  fun h => h.2.2 (.atm (.r 0)) rfl
/-- A control arm may use its own resume, without leaking the handler's region into its outer row. -/
example (S : Sig) (hop : S.op "E" "op" = some (.control, .base .unit, .base .unit)) :
    Typ S [] Facts.none []
      (.acons (Pat.spine "op" [.var, .var]) (.app (.var 1) (.lit .unit)) .anil)
      (.harms "E" (.base .unit) emptyStage (.tvar 0)) := by
  refine .hactl hop ?_ .hanil
  exact .app (.var (T := .arrow (.base .unit) emptyStage (.base .unit) (Ty.tvar 0).reach)
      (ts := []) rfl rfl (fun _ _ h => h.elim) (by rw [Scheme.inst_mono]; exact .refl)) .lit
    ⟨fun _ h => h, fun _ h => h⟩ .refl

end Flick.LiveChecks

namespace Flick.LiveChecks

/-- A row variable may later contain Live, so it cannot bypass an owner's bound. -/
example : ¬ Row.Outlives Facts.none (fun x => x = .rv (.v 0) fun _ => False) (.rg 1) := by
  intro h
  rcases h (.v 0) (Or.inr ⟨_, rfl⟩) with h | ⟨_, h, _⟩
  · cases h
  · exact h.elim

/-- `Scope.run` of a function whose qualifier names a region not in the rows around the scope
is rejected: the scope's rows must keep what its body uses live. -/
example : ¬ Const.ok Facts.none .scopeRun { (default : CArg) with C := fun a => a = .r 0 } :=
  fun h => h (.atm (.r 0)) rfl

example : Const.ok Facts.none .scopeRun
    { (default : CArg) with C := fun a => a = .r 0, r := Row.ofCaps fun a => a = .r 0 } :=
  fun _ h => h

/-- A cell of a capability: its contents reach region 0. -/
def hndCell : CArg := { (default : CArg) with t := .hnd (.rg 0) }

/-- A cell's capacity, the qualifier `C` of a `Ref` constant, bounds what the cell stores:
`Ref.make` rejects a capacity smaller than its contents. -/
example : ¬ Const.ok Facts.none .refMake hndCell := by
  intro h
  rcases h (.r 0) rfl with h | ⟨_, h, _⟩
  · exact h
  · exact h.elim

example : Const.ok Facts.none .refMake { hndCell with C := region } :=
  fun _ h => .inl h

/-- `Ref.set` needs its capacity live: storing into a cell that may hold a capability of
region 0 is a call whose stage keeps region 0 live, whatever its row says. -/
theorem refSet_needs_capacity (S : Sig) (ι : CArg) (hok : Const.ok Facts.none .refSet ι)
    (ht : ι.t = .hnd (.rg 0)) {T : Ty} (hT : Const.ty S .refSet ι = .arrow (.ref ι.t) .pure T Caps.none) :
    ¬ LeLive S T (.arrow ι.t emptyStage (.base .unit) Caps.none) := by
  injection hT with _ _ hT; subst hT
  intro h
  have hC : ι.C (.r 0) := by
    rcases hok (.r 0) (by rw [ht]; rfl) with h | ⟨_, h, _⟩
    · exact h
    · exact h.elim
  exact (h.arrow_use.1 (.atm (.r 0)) (Or.inr hC)).elim id id

/-! ### The hidden row of data (regions.md §2.2, §6)

Subtyping widens the hidden row of data with hidden storage, and nothing else. -/

/-- `G C u v`, the hidden row first, as #217 spells data with a stored function. -/
def GT (C : Caps) (u v : Ty) : Ty := Ty.tcon "G" [.caps C, u, v]

/-- `cf` composes data at the rows `{c}` and `{d}` into data at `{c, d}`: the hidden row
widens. -/
theorem hidden_row_widens {S : Sig} {C C' : Caps} (hG : S.hidden "G" = some 0)
    (hCC : Caps.Sub C C') (u v : Ty) : LeLive S (GT C u v) (GT C' u v) := by
  refine .hide (us := [.caps C, u, v]) (us' := [.caps C', u, v]) hG rfl (fun j hj => ?_)
    (fun r r' h h' => ?_) rfl rfl
  · rcases j with _ | _ | _ | j <;> simp_all
  · simp only [List.getElem?_cons_zero, Option.some.injEq] at h h'
    subst h; subst h'
    exact hCC

/-- `leak6` stops at the `scope` that returns the composition (the compiler reports G001
there): widening never drops the region the row holds, so a value that needs the scope's
region `v 0` has no type `t.up 1`. -/
theorem leak6_scope_rejected {S : Sig} {C : Caps} (hC : C (.v 0)) (u v t : Ty) :
    ¬ LeLive S (GT C u v) (t.up 1) := fun h =>
  Ty.up_avoid_v0 t (h.reach (.v 0) (by simp [GT, Ty.tcon, Ty.reach, hC]))

/-- A row the declaration names stays put: data without a hidden row, whose parameters are
all invariant, are below only themselves. -/
theorem named_row_invariant {S : Sig} {T : Name} {us : List Ty} {b : Ty}
    (hT : S.hidden T = none) (hv : ∀ j, S.var T j = .inv) (h : LeLive S (Ty.tcon T us) b) :
    b = Ty.tcon T us := by
  generalize hX : Ty.tcon T us = X at h
  cases h with
  | refl => rfl
  | arrow => exact absurd hX.symm Ty.arrow_ne_tcon
  | eff => exact absurd hX.symm Ty.eff_ne_tcon
  | hide hi _ _ _ ha _ =>
    obtain ⟨rfl, -⟩ := Ty.tcon_eq (hX.trans ha)
    rw [hT] at hi; cases hi
  | data hl _ _ _ hinv ha hb =>
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hX.trans ha)
    rw [hb, ← hX]
    exact congrArg (Ty.tcon T) (List.ext_getElem hl fun j h1 h2 => hinv j _ _
      (List.getElem?_eq_getElem h1) (List.getElem?_eq_getElem h2) (by rw [hT]; simp) (hv j)).symm

/-- The field of `type K (c : CaptureRow) = K ((Unit ->{} Unit)^c ->{} Unit)`: the row
qualifies a parameter of the field. -/
def kRow : Caps := fun a => a = .v 0
def kCallback : Ty := .arrow (.base .unit) .pure (.base .unit) kRow
def kField : Ty := .arrow kCallback .pure (.base .unit) Caps.none

theorem kCallback_not_noV : ¬ kCallback.NoV 0 := by
  intro h
  simp only [Ty.NoV, kCallback, Ty.substT, Stage.substT, Ty.arrow.injEq, true_and] at h
  have := congrFun h (.v 0)
  simp [Caps.substT, kRow, dropV, Ty.reach, Caps.none] at this

/-- No signature makes that row hidden: widening it would let a callback that needs a
region outlive its `scope` (regions.md §6), and `Sig.hidden_ok` asks the row to stand only
where widening it widens the field. -/
theorem contra_row_not_hidden (S : Sig) (hK : S.ctor "K" = some (1, [kField], "K")) :
    S.hidden "K" ≠ some 0 := by
  intro h
  obtain ⟨-, -, -, hc⟩ := S.hidden_ok "K" 0 h "K" 1 [kField] hK
  rcases hc kField (.head _) with h | ⟨a, φ, b, C, he, ha, -, -⟩ | ⟨r, e, t, C, he, -⟩ |
      ⟨T, us, j, he, -⟩
  · have : kCallback.substT (dropV 0) = kCallback := (Ty.arrow.inj h).1
    exact kCallback_not_noV this
  · obtain ⟨rfl, -⟩ := Ty.arrow.inj he
    exact kCallback_not_noV ha
  · cases he
  · exact Ty.arrow_ne_tcon he

end Flick.LiveChecks
