import Formal.Generalization
import Formal.Regions
import Formal.Live.T4Live

/-!
# Checks that the rules are usable

`let id = λx. x in let _ = id 1 in id True` uses `id` at `Int` and at `Bool`.
`scope s { op (open s) }` acquires a capability of its region and uses it: its
type is `Unit`, and `Scope s` leaves the row of the scope.
`let app = λf. f () in app (λ_. let _ = S.op () in ())` generalizes `app` over the row of
services of its argument and uses it at `{S}`.
`λf. try f () with _ → ()` removes the row variable of the errors of `f`.
`(λf. let _ = f 1 in f True) (λx. x)` uses a parameter of type `∀a. a → a` at `Int` and at
`Bool`, and takes a lambda or a generalized variable as the argument.
`let f = λo. op (open o) in Scope.run f` passes a variable at the rank-2 type of `Scope.run`,
and T4.1 holds of it.
`let mk = λx. Ref.make x in mk h` generalizes `mk` under the value bound `α^{≤c}`, a fact its
body assumes, and an instance whose contents reach more than the capacity is rejected.
-/

namespace Flick

example (S : Sig) :
    Typ S [] Facts.none [] (.let_ (.lam (.var 0))
      (.let_ (.app (.var 0) (.lit (.int 1))) (.app (.var 1) (.lit (.bool true)))))
      (.ex (.base .bool) .pure) := by
  refine .letGen (k := 1) (Φ := Facts.none) (a := .arrow (.tvar 0) .pure (.tvar 0) Caps.none) ?_ ?_
  · exact .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (fun h => h) .refl
  · refine .let_ (a := .base .int) ?_ ?_
    · exact .app (.var (ts := [.base .int]) rfl rfl (fun _ _ h => h.elim) .refl) .lit trivial .refl
    · exact .app (.var (ts := [.base .bool]) rfl rfl (fun _ _ h => h.elim) .refl) .lit trivial .refl

/-- `∀a. a → a`. -/
def idAll : Ty := .all 1 (.arrow (.tvar 0) .pure (.tvar 0) Caps.none)

/-- `apply_twice : (∀a. a → a) → Bool`. -/
theorem apply_twice_lam (S : Sig) :
    Typ S [] Facts.none [] (.lam (.let_ (.app (.var 0) (.lit (.int 1))) (.app (.var 1) (.lit (.bool true)))))
      (.ex (.arrow idAll .pure (.base .bool) Caps.none) .pure) := by
  have inst : ∀ {Γ i} (t : Ty), Γ[i]? = some (Scheme.mono idAll) →
      Typ S [] Facts.none Γ (.var i) (.ex (.arrow t .pure t Caps.none) .pure) := fun t hi =>
    .var (ts := []) hi rfl (fun _ _ h => h.elim) (.inst (us := [t]) rfl (by
      simp [Ty.substT]; exact .refl))
  refine .lam (a := idAll) (ψ := .pure) (.let_ (a := .base .int) ?_ ?_)
    (fun h => by simp [Tm.Has] at h) .refl
  · exact .app (inst (.base .int) rfl) .lit trivial .refl
  · exact .app (inst (.base .bool) rfl) .lit trivial .refl

/-- `λx. x` as a value of `∀a. a → a`. -/
theorem id_lam_all (S : Sig) : Typ S [] Facts.none [] (.lam (.var 0)) (.ex idAll .pure) := by
  exact .allI .lam (.lam (a := .tvar 0) (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
    (fun h => h) .refl)

/-- A generalized variable as a value of its polymorphic type: the argument need not be a
lambda where it is passed. -/
example (S : Sig) (i : Nat) (Γ : List Scheme) (h : Γ[i]? = some ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩) :
    Typ S [] Facts.none Γ (.var i) (.ex idAll .pure) :=
  .allVar (σ := ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩) h Nat.one_pos fun _ _ h => h

/-- The instance of `open_` and `op` at the region `s` of the scope. -/
def regionArg : CArg := { (default : CArg) with s := .tvar 0 }

example (S : Sig) :
    Typ S [] Facts.none [] (.scope (.app (.const .op) (.app (.const .open_) (.var 0))))
      (.ex (.base .unit) (.rt Row.none Row.none)) := by
  refine .scope (F := Facts.none) (φ' := .rt (Row.scope (.tvar 0)) Row.none) (r := Row.none)
    (ε := Row.none) (fun h => by simp [Tm.Has] at h) (fun _ _ h => h.elim) ?_ ?_
    (Stage.le_refl _) .refl
  · show Typ S _ _ _ _ (.ex (.base .unit) (.rt (Row.scope (.tvar 0)) Row.none))
    refine Typ.app (Typ.const (c := .op) (ι := regionArg) (by simp [Const.ok]) .refl) ?_ ?_ .refl
    · exact .app (.const (c := .open_) (ι := regionArg) (by simp [Const.ok]) .refl)
        (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (by simp only [Stage.use_none]; exact Stage.le_refl _) .refl
    · rw [Stage.use_none]; exact Stage.le_refl _
  · exact ⟨fun x h => .inl h, fun _ h => h.elim⟩

/-- `∀s. Owner s →R⟨Scope s; ∅⟩ Unit`, the argument of `Scope.run` here, without its `∀`. -/
def runBodyTy : Ty :=
  .arrow (.owner (.tvar 0))
    (.rtob (.run (.caps (Ty.tvar 0).reach) (Ty.cnil.up 1)) (.tvar 0) (Ty.cnil.up 1))
    ((Ty.base .unit).up 1) (Caps.none.substT (shiftV 1))

/-- `let f = λo. op (open o) in Scope.run f`: the argument is a variable, and its type has the
region of the scope as a binder. -/
def runProg : Tm :=
  .let_ (.lam (.app (.const .op) (.app (.const .open_) (.var 0)))) (.app (.const .scopeRun) (.var 0))

theorem runProg_typed (S : Sig) :
    Typ S [] Facts.none [] runProg (.ex (.base .unit) (.rt Row.none Row.none)) := by
  refine .letGen (k := 1) (Φ := Facts.none) (a := runBodyTy) ?_ ?_
  · refine .lam (a := .owner (.tvar 0)) ?_ (fun h => by simp [Tm.Has] at h) .refl
    · refine Typ.app (Typ.const (c := .op) (ι := regionArg) (by simp [Const.ok]) .refl) ?_ ?_ .refl
      · exact .app (.const (c := .open_) (ι := regionArg) (by simp [Const.ok]) .refl)
          (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
          (by rw [Stage.use_none]; exact ⟨fun _ h => .inl (.inl (.inl h)), fun _ h => h.elim⟩) .refl
      · rw [Stage.use_none]; exact ⟨fun _ h => .inl (.inl (.inl h)), fun _ h => h.elim⟩
  · exact .app (.const (c := .scopeRun) (ι := { (default : CArg) with })
      (fun x h => by cases x <;> exact h.elim) .refl)
      (.allVar (σ := ⟨1, Facts.none, runBodyTy⟩) rfl Nat.one_pos fun _ _ h => h)
      (by simp only [Stage.use_none]; exact Stage.le_refl _) .refl

/-- T4.1 for that program: no thread reaches a step that uses a region after it closed. -/
example {S : Sig} {root : Roots} {c : Cfg} (hroot : RootTy S root)
    (hs : CSteps root ⟨[], [], runProg⟩ c) :
    ∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r :=
  (T4' hroot _ _ _ c (runProg_typed S) (Tm.Below.empty fun _ => by simp [runProg, Tm.Has]) rfl
    (by simp [Stage.substT]) hs).1

/-- The row variable `ρ`, the variable `0` of a scheme. -/
def rowVar : Row := fun x => x = .rv (.v 0) fun _ => False

/-- `ρ − L`, the tail of `{-L | ρ}`: the row variable without the name `L`. -/
def rowLess (L : Name) : Row := fun x => x = .rv (.v 0) (· = L)

/-- `ρ ⊆ {L | ρ − L}`: beside `L` the row gives it back to its variable. -/
example (L : Name) :
    Row.Sub rowVar (Row.union (Row.single L) ((rowLess L).back (Row.single L))) := by
  rintro x rfl
  exact .inr ⟨_, rfl, fun _ h => h.elim, fun n h => .inr ⟨_, by cases h; rfl⟩⟩

/-- `ρ ⊄ ρ − L`: the variable may hold `L`. -/
example (L : Name) : ¬ Row.Sub rowVar (rowLess L) := by
  intro h
  have h := congrFun (Elem.rv.inj (h _ rfl)).2 L
  simp at h

/-- `(ρ − L)[ρ := {L, C}]` holds `C` and not `L`, at any argument. -/
example (L C : Name) (hC : C ≠ L) (c : TyClass) :
    let r := (rowLess L).substT (instT [.run (.rnm L (.adt L)) (.rnm C (.adt C))])
    r (.nm₀ C) ∧ ¬ r (.nm L c) := by
  refine ⟨.inr ⟨.v 0, _, rfl, hC, .inr (.inl rfl)⟩, ?_⟩
  rintro (⟨_, h, -⟩ | ⟨_, _, h, hL, -⟩)
  · cases h
  · cases h; exact hL rfl

/-- `Unit →R⟨ρ;∅⟩ Unit`. -/
def thunkTy (r : Ty) : Ty := .arrow (.base .unit) (.rt r .cnil) (.base .unit) Caps.none

/-- `app : ∀ρ. (Unit →R⟨ρ;∅⟩ Unit) →R⟨ρ;∅⟩ Unit`, used at `ρ := {S}`, a service. -/
example (S : Sig) (hS₀ : S.arity "S" = 0) (hop : S.op "S" "op" = some (.tail, .base .unit, .base .unit)) :
    Typ S [] Facts.none [] (.let_ (.lam (.app (.var 0) (.lit .unit)))
      (.app (.var 0) (.lam (.let_ (.app (.const (.perform "S" "op" .tail)) (.lit .unit)) (.lit .unit)))))
      (.ex (.base .unit) (.rt (Row.single "S") Row.none)) := by
  have hS : (Ty.rt (.rnm "S" (.adt "S")) .cnil).asStage = .rt (Row.single "S") Row.none := by
    simp only [Ty.asStage_rt, Stage.rt.injEq]
    refine ⟨funext fun x => ?_, rfl⟩
    cases x <;> simp [Ty.asRow, Row.union, Row.inst, Row.single, Row.ofCaps, Ty.reach, Caps.none]
  refine .letGen (k := 1) (Φ := Facts.none)
    (a := .arrow (thunkTy (.tvar 0)) (.rt (.tvar 0) .cnil) (.base .unit) Caps.none) ?_ ?_
  · refine .lam (.app (C := Caps.none) (.var (T := thunkTy (.tvar 0)) (ts := []) rfl rfl
      (fun _ _ h => h.elim) ?_) .lit (by simp only [Stage.use_none]; exact Stage.le_refl _) .refl) (fun h => by simp [Tm.Has] at h) .refl
    · rw [Scheme.inst_mono]; exact .refl
  · refine .app (ψ := .rt (.rnm "S" (.adt "S")) .cnil) (a := thunkTy (.rnm "S" (.adt "S"))) (C := Caps.none)
      (.var (T := .arrow (thunkTy (.rnm "S" (.adt "S"))) (.rt (.rnm "S" (.adt "S")) .cnil) (.base .unit) Caps.none)
        (ts := [.rnm "S" (.adt "S")]) rfl rfl (fun _ _ h => h.elim) ?_) ?_
        (by rw [Stage.use_none, hS]; exact Stage.le_refl _) .refl
    · simp only [Scheme.inst, Ty.substT, thunkTy, Ty.isStage, if_true, instT, Caps.none_substT]
      exact .refl
    · refine .lam (ψ := .rt (.rnm "S" (.adt "S")) .cnil) (.let_ (a := .base .unit)
        (.app (.perform (t := .base .unit) (us := []) hS₀.symm hop .refl) .lit
          (by rw [Stage.use_none, hS]; exact Stage.le_refl _) .refl) (by rw [Stage.use_none, hS]; exact .lit))
        (fun h => by simp [Tm.Has] at h) .refl

/-- `Unit →R⟨∅;ε⟩ Unit`. -/
def raiseThunkTy (ε : Ty) : Ty := .arrow (.base .unit) (.rt .cnil ε) (.base .unit) Caps.none

/-- `λf. try f () with _ → ()`: the arm `_` removes the row variable `ρ` of the errors of `f`,
so the function raises nothing. -/
example (S : Sig) :
    Typ S [] Facts.none []
      (.lam (.try_ (.app (.var 0) (.lit .unit)) (.acons .wild (.lit .unit) .anil)))
      (.ex (.arrow (raiseThunkTy (.tvar 0)) (.rt .cnil .cnil) (.base .unit) Caps.none) .pure) := by
  refine .lam (.tryR (r := Row.none) (ε₀ := .tvar 0) (H := rowVar) (ε' := Row.none) ?_ ?_
    (by simp only [Stage.use_none]; exact Stage.le_refl _) fun n hn => .inl (by subst hn; rfl))
    (fun h => by simp [Tm.Has] at h) .refl
  · refine .app (.var (T := raiseThunkTy (.tvar 0)) (ts := []) rfl rfl (fun _ _ h => h.elim) ?_) .lit
      (by simp only [Stage.use_none]; exact Stage.le_refl _) .refl
    rw [Scheme.inst_mono]; exact .refl
  · refine .hcons .wild .lit (.hnil (H := Row.none) fun _ h => h) fun n hn => ?_
    subst hn; exact .inl ⟨rfl, rfl⟩

/-- The scheme `∀a. a →P a` of the field `id`. -/
def idField : Scheme := ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩

/-- A dictionary record of a class with one parameter and the field `id : ∀a. a →P a`,
projected and used at `Int` (λFlick⁺, elaboration.md §0.1). -/
example (S : Sig) (h : S.dict 0 = some (1, [("id", idField)])) :
    Typ S [] Facts.none [] (.app (.proj (.drec 0 (.rcons "id" (.lam (.var 0)) .rnil)) "id") (.lit (.int 1)))
      (.ex (.base .int) .pure) := by
  have hl : [("id", idField)].lookup "id" = some idField := rfl
  refine .app (a := .base .int) (ψ := .pure) (t := .base .int) (C := Caps.none)
    (.dproj (us := [.base .bool]) (ts := [.base .int]) ?_ h rfl hl rfl ?_ ?_) .lit trivial .refl
  · refine .drec h rfl (.dcons ?_ .dnil) (fun h => by simp [Tm.Has] at h)
    · refine .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) ?_) (fun h => h) .refl
      · simp [idField, Scheme.inst, Ty.substT, liftT]; exact .refl
  · rintro _ _ ⟨_, _, h, _⟩; exact h.elim
  · simp [idField, Scheme.substT, Scheme.inst, Ty.substT, liftT, instT]; exact .refl

/-- `mk : ∀α c. α^{≤c} ⇒ α →R⟨∅;∅⟩ Ref α`: the value bound of `α` is a fact of the scheme. -/
def mkSch : Scheme :=
  ⟨2, Facts.bnd 0 (· = .v 1), .arrow (.tvar 0) (.rt .cnil .cnil) (.ref (.tvar 0)) Caps.none⟩

/-- `let mk = λx. Ref.make x in mk h`, with `h` a capability of region 0: `mk` is generic, its
body makes a cell of capacity `c` under the bound `α^{≤c}`, and the use instantiates the
capacity with the region. -/
example (S : Sig) :
    Typ S [] Facts.none [.mono (.hnd (.rg 0))]
      (.let_ (.lam (.app (.const .refMake) (.var 0))) (.app (.var 0) (.var 1)))
      (.ex (.ref (.hnd (.rg 0))) (.rt Row.none Row.none)) := by
  refine .letGen (k := 2) (Φ := mkSch.cs) (a := mkSch.t) ?_ ?_
  · refine .lam (a := .tvar 0) (ψ := .rt .cnil .cnil) (C := Caps.none) ?_
      (fun h => by simp [Tm.Has] at h) .refl
    refine .app (.const (c := .refMake) (ι := { (default : CArg) with t := .tvar 0, C := (· = .v 1) })
      ?_ .refl) (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (Stage.le_refl _) .refl
    rintro k rfl; exact .inr ⟨_, ⟨_, .inr ⟨rfl, rfl⟩, .refl⟩, rfl⟩
  · refine .app (ψ := .rt .cnil .cnil) (C := Caps.none)
      (.var (σ := mkSch) (ts := [.hnd (.rg 0), .caps (· = .r 0)]) rfl rfl ?_ ?_)
      (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
      (by simp only [Stage.use_none]; exact Stage.le_refl _) .refl
    · refine Scheme.holds_bnd.2 (Outl.caps_of_sub fun k hk => ?_)
      exact ⟨.v 1, rfl, by simpa [instT, Ty.reach] using hk⟩
    · simp [mkSch, Scheme.inst, Ty.substT, instT]; exact .refl

/-- An instance of `mk` whose contents reach more than its capacity is rejected: (Inst) asks
`⌈H 0⌉ ⊆ ∅`. -/
example : ¬ mkSch.Holds Facts.none [.hnd (.rg 0), .caps Caps.none] := by
  intro h
  rcases Scheme.holds_bnd.1 h (.r 0) rfl with ⟨_, rfl, h⟩ | ⟨_, h, _⟩
  · simp [instT, Ty.reach, Caps.none] at h
  · simp only [Facts.close_none, Facts.none] at h

end Flick
