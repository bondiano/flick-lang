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
-/

namespace Flick

example (S : Sig) :
    TypM S .live [] Facts.none [] (.let_ (.lam (.var 0))
      (.let_ (.app (.var 0) (.lit (.int 1))) (.app (.var 1) (.lit (.bool true)))))
      (.ex (.base .bool) .pure) := by
  refine .letGen (k := 1) (Φ := Facts.none) (a := .arrow (.tvar 0) .pure (.tvar 0) Caps.none) ?_ ?_
  · refine .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (fun h => h) ?_ .refl
    trivial
  · refine .let_ (a := .base .int) ?_ ?_
    · exact .app (.var (ts := [.base .int]) rfl rfl (fun _ _ h => h.elim) .refl) .lit trivial .refl
    · exact .app (.var (ts := [.base .bool]) rfl rfl (fun _ _ h => h.elim) .refl) .lit trivial .refl

/-- `∀a. a → a`. -/
def idAll : Ty := .all 1 (.arrow (.tvar 0) .pure (.tvar 0) Caps.none)

/-- `apply_twice : (∀a. a → a) → Bool`. -/
theorem apply_twice_lam (S : Sig) :
    TypM S .live [] Facts.none [] (.lam (.let_ (.app (.var 0) (.lit (.int 1))) (.app (.var 1) (.lit (.bool true)))))
      (.ex (.arrow idAll .pure (.base .bool) Caps.none) .pure) := by
  have inst : ∀ {Γ i} (t : Ty), Γ[i]? = some (Scheme.mono idAll) →
      TypM S .live [] Facts.none Γ (.var i) (.ex (.arrow t .pure t Caps.none) .pure) := fun t hi =>
    .var (ts := []) hi rfl (fun _ _ h => h.elim) (.inst (us := [t]) rfl (by
      simp [Ty.substT, Stage.substT]; exact .refl))
  refine .lam (a := idAll) (ψ := .pure) (.let_ (a := .base .int) ?_ ?_)
    (fun h => by simp [Tm.Has] at h) ?_ .refl
  · exact .app (inst (.base .int) rfl) .lit trivial .refl
  · exact .app (inst (.base .bool) rfl) .lit trivial .refl
  · trivial

/-- `λx. x` as a value of `∀a. a → a`. -/
theorem id_lam_all (S : Sig) : TypM S .live [] Facts.none [] (.lam (.var 0)) (.ex idAll .pure) := by
  refine .allI .lam (.lam (a := .tvar 0) (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
    (fun h => h) ?_ .refl)
  trivial

/-- A generalized variable as a value of its polymorphic type: the argument need not be a
lambda where it is passed. -/
example (S : Sig) (i : Nat) (Γ : List Scheme) (h : Γ[i]? = some ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩) :
    TypM S .live [] Facts.none Γ (.var i) (.ex idAll .pure) :=
  .allVar (σ := ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩) h Nat.one_pos fun _ _ h => h

/-- The instance of `open_` and `op` at the region `s` of the scope. -/
def regionArg : CArg := { (default : CArg) with s := .tvar 0 }

example (S : Sig) :
    TypM S .live [] Facts.none [] (.scope (.app (.const .op) (.app (.const .open_) (.var 0))))
      (.ex (.base .unit) (.rt Row.none Row.none)) := by
  refine .scope (F := Facts.none) (φ' := .rt (Row.scope (.tvar 0)) Row.none) (r := Row.none)
    (ε := Row.none) (fun h => by simp [Tm.Has] at h) (fun _ _ h => h.elim) ?_ ?_
    (Stage.le_refl _) .refl
  · show TypM S .live _ _ _ _ (.ex (.base .unit) (.rt (Row.scope (.tvar 0)) Row.none))
    refine TypM.app (TypM.const (c := .op) (ι := regionArg) (by simp [Const.ok]) .refl) ?_ ?_ .refl
    · exact .app (.const (c := .open_) (ι := regionArg) (by simp [Const.ok]) .refl)
        (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (by simp only [TypingMode.use_none]; exact Stage.le_refl _) .refl
    · rw [TypingMode.use_none]; exact Stage.le_refl _
  · exact ⟨fun x h => .inl h, fun _ h => h.elim⟩

/-- `∀s. Owner s →R⟨Scope s; ∅⟩ Unit`, the argument of `Scope.run` here, without its `∀`. -/
def runBodyTy : Ty :=
  .arrow (.owner (.tvar 0))
    (.rt (Row.union (Row.scope (.tvar 0)) (Row.union (Row.none.substT (shiftV 1))
      (Row.obs (Row.none.substT (shiftV 1)) (.tvar 0)))) (Row.none.substT (shiftV 1)))
    (.base .unit) (Caps.none.substT (shiftV 1))

/-- `let f = λo. op (open o) in Scope.run f`: the argument is a variable, and its type has the
region of the scope as a binder. -/
def runProg : Tm :=
  .let_ (.lam (.app (.const .op) (.app (.const .open_) (.var 0)))) (.app (.const .scopeRun) (.var 0))

theorem runProg_typed (S : Sig) :
    TypM S .live [] Facts.none [] runProg (.ex (.base .unit) (.rt Row.none Row.none)) := by
  refine .letGen (k := 1) (Φ := Facts.none) (a := runBodyTy) ?_ ?_
  · refine .lam (a := .owner (.tvar 0)) ?_ (fun h => by simp [Tm.Has] at h) ?_ .refl
    · refine TypM.app (TypM.const (c := .op) (ι := regionArg) (by simp [Const.ok]) .refl) ?_ ?_ .refl
      · exact .app (.const (c := .open_) (ι := regionArg) (by simp [Const.ok]) .refl)
          (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
          (by rw [TypingMode.use_none]; exact ⟨fun _ h => .inl (.inl h), fun _ h => h.elim⟩) .refl
      · rw [TypingMode.use_none]; exact ⟨fun _ h => .inl (.inl h), fun _ h => h.elim⟩
    · trivial
  · exact .app (.const (c := .scopeRun) (ι := { (default : CArg) with })
      (fun x h => by cases x <;> exact h.elim) .refl)
      (.allVar (σ := ⟨1, Facts.none, runBodyTy⟩) rfl Nat.one_pos fun _ _ h => h)
      (by simp only [TypingMode.use_none]; exact Stage.le_refl _) .refl

/-- T4.1 for that program: no thread reaches a step that uses a region after it closed. -/
example {S : Sig} {root : Roots} {c : Cfg} (hroot : RootTyM S .live root)
    (hs : CSteps root ⟨[], [], runProg⟩ c) :
    ∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r :=
  (T4'_live hroot _ _ _ c (runProg_typed S) (Tm.Below.empty fun _ => by simp [runProg, Tm.Has]) rfl
    (by simp [Stage.substT]) hs).1

/-- The row variable `ρ`, the variable `0` of a scheme. -/
def rowVar : Row := fun x => x = .rv (.v 0)

/-- `Unit →R⟨ρ;∅⟩ Unit`. -/
def thunkTy (r : Row) : Ty := .arrow (.base .unit) (.rt r Row.none) (.base .unit) Caps.none

theorem rowVar_inst (r : Row) : rowVar.substT (instT [.row r]) = r.plain := by
  funext x; apply propext
  cases x <;> simp [rowVar, Row.substT, instT, Ty.asRow_row, Row.plain]

/-- `app : ∀ρ. (Unit →R⟨ρ;∅⟩ Unit) →R⟨ρ;∅⟩ Unit`, used at `ρ := {S}`, a service. -/
example (S : Sig) (hop : S.op "S" "op" = some (.tail, .base .unit, .base .unit)) :
    TypM S .live [] Facts.none [] (.let_ (.lam (.app (.var 0) (.lit .unit)))
      (.app (.var 0) (.lam (.let_ (.app (.const (.perform "S" "op" .tail)) (.lit .unit)) (.lit .unit)))))
      (.ex (.base .unit) (.rt (Row.single "S") Row.none)) := by
  have hS : (Row.single "S").plain = Row.single "S" := by
    funext x; cases x <;> simp [Row.plain, Row.single]
  refine .letGen (k := 1) (Φ := Facts.none)
    (a := .arrow (thunkTy rowVar) (.rt rowVar Row.none) (.base .unit) Caps.none) ?_ ?_
  · refine .lam (.app (C := Caps.none) (.var (T := thunkTy rowVar) (ts := []) rfl rfl
      (fun _ _ h => h.elim) ?_) .lit (by simp only [TypingMode.use_none]; exact Stage.le_refl _) .refl) (fun h => by simp [Tm.Has] at h) ?_ .refl
    · rw [Scheme.inst_mono]; exact .refl
    · trivial
  · refine .app (ψ := .rt (Row.single "S") Row.none) (a := thunkTy (Row.single "S")) (C := Caps.none)
      (.var (T := .arrow (thunkTy (Row.single "S")) (.rt (Row.single "S") Row.none) (.base .unit) Caps.none)
        (ts := [.row (Row.single "S")]) rfl rfl (fun _ _ h => h.elim) ?_) ?_ (by simp only [TypingMode.use_none]; exact Stage.le_refl _) .refl
    · simp only [Scheme.inst, Ty.substT, thunkTy, Stage.substT, rowVar_inst, hS, Row.none_substT,
        Caps.none_substT]
      exact .refl
    · refine .lam (ψ := .rt (Row.single "S") Row.none) (.let_ (a := .base .unit)
        (.app (.perform (t := .base .unit) hop .refl) .lit (by simp only [TypingMode.use_none]; exact Stage.le_refl _) .refl) .lit)
        (fun h => by simp [Tm.Has] at h) ?_ .refl
      trivial

/-- `Unit →R⟨∅;ε⟩ Unit`. -/
def raiseThunkTy (ε : Row) : Ty := .arrow (.base .unit) (.rt Row.none ε) (.base .unit) Caps.none

/-- `λf. try f () with _ → ()`: the arm `_` removes the row variable `ρ` of the errors of `f`,
so the function raises nothing. -/
example (S : Sig) :
    TypM S .live [] Facts.none []
      (.lam (.try_ (.app (.var 0) (.lit .unit)) (.acons .wild (.lit .unit) .anil)))
      (.ex (.arrow (raiseThunkTy rowVar) (.rt Row.none Row.none) (.base .unit) Caps.none) .pure) := by
  refine .lam (.tryR (r := Row.none) (ε₀ := rowVar) (H := rowVar) (ε' := Row.none) ?_ ?_
    (by simp only [TypingMode.use_none]; exact Stage.le_refl _) fun n hn => .inl hn) (fun h => by simp [Tm.Has] at h) ?_ .refl
  · refine .app (.var (T := raiseThunkTy rowVar) (ts := []) rfl rfl (fun _ _ h => h.elim) ?_) .lit
      (by simp only [TypingMode.use_none]; exact Stage.le_refl _) .refl
    rw [Scheme.inst_mono]; exact .refl
  · refine .hcons .wild .lit (.hnil (H := Row.none) fun _ h => h) fun n hn => ?_
    subst hn; exact .inl ⟨rfl, rfl⟩
  · trivial

/-- The scheme `∀a. a →P a` of the field `id`. -/
def idField : Scheme := ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩

/-- A dictionary record of a class with one parameter and the field `id : ∀a. a →P a`,
projected and used at `Int` (λFlick⁺, elaboration.md §0.1). -/
example (S : Sig) (h : S.dict 0 = some (1, [("id", idField)])) :
    TypM S .live [] Facts.none [] (.app (.proj (.drec 0 (.rcons "id" (.lam (.var 0)) .rnil)) "id") (.lit (.int 1)))
      (.ex (.base .int) .pure) := by
  have hl : [("id", idField)].lookup "id" = some idField := rfl
  refine .app (a := .base .int) (ψ := .pure) (t := .base .int) (C := Caps.none)
    (.dproj (us := [.base .bool]) (ts := [.base .int]) ?_ h rfl hl rfl ?_ ?_) .lit trivial .refl
  · refine .drec h rfl (.dcons ?_ .dnil) (fun h => by simp [Tm.Has] at h) ?_
    · refine .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) ?_) (fun h => h) ?_ .refl
      · simp [idField, Scheme.inst, Ty.substT, liftT]; exact .refl
      · trivial
    · trivial
  · rintro _ _ ⟨_, _, h, _⟩; exact h.elim
  · simp [idField, Scheme.substT, Scheme.inst, Ty.substT, liftT, instT]; exact .refl

end Flick
