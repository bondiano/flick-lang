import Formal.Generalization

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
-/

namespace Flick

example (S : Sig) :
    Typ S [] Facts.none [] (.let_ (.lam (.var 0))
      (.let_ (.app (.var 0) (.lit (.int 1))) (.app (.var 1) (.lit (.bool true)))))
      (.ex (.base .bool) .pure) := by
  refine .letGen (k := 1) (Φ := Facts.none) (a := .arrow (.tvar 0) .pure (.tvar 0) Caps.none) ?_ ?_
  · refine .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (fun h => h) ?_ .refl
    rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · simp [Tm.Free] at hi
    · exact hp.elim
    · exact hl.elim
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
      simp [Ty.substT, Stage.substT]; exact .refl))
  refine .lam (a := idAll) (ψ := .pure) (.let_ (a := .base .int) ?_ ?_)
    (fun h => by simp [Tm.Has] at h) ?_ .refl
  · exact .app (inst (.base .int) rfl) .lit trivial .refl
  · exact .app (inst (.base .bool) rfl) .lit trivial .refl
  · rintro a (⟨i, s, hi, hs, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · simp at hs
    · simp [Tm.Has] at hp
    · simp [Tm.Has] at hl

/-- `λx. x` as a value of `∀a. a → a`. -/
theorem id_lam_all (S : Sig) : Typ S [] Facts.none [] (.lam (.var 0)) (.ex idAll .pure) := by
  refine .allI .lam (.lam (a := .tvar 0) (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl)
    (fun h => h) ?_ .refl)
  rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
  · simp [Tm.Free] at hi
  · exact hp.elim
  · exact hl.elim

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
        (.var (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (Stage.le_refl _) .refl
    · exact ⟨fun _ h => h.elim, fun _ h => h⟩
  · exact ⟨fun x h => .inl h, fun _ h => h.elim⟩

/-- The row variable `ρ`, the variable `0` of a scheme. -/
def rowVar : Row := fun x => x = .rv (.v 0)

/-- `Unit →R⟨ρ;∅⟩ Unit`. -/
def thunkTy (r : Row) : Ty := .arrow (.base .unit) (.rt r Row.none) (.base .unit) Caps.none

theorem rowVar_inst (r : Row) : rowVar.substT (instT [.row r]) = r.plain := by
  funext x; apply propext
  cases x <;> simp [rowVar, Row.substT, instT, Ty.asRow_row, Row.plain]

/-- `app : ∀ρ. (Unit →R⟨ρ;∅⟩ Unit) →R⟨ρ;∅⟩ Unit`, used at `ρ := {S}`, a service. -/
example (S : Sig) (hop : S.op "S" "op" = some (.tail, .base .unit, .base .unit)) :
    Typ S [] Facts.none [] (.let_ (.lam (.app (.var 0) (.lit .unit)))
      (.app (.var 0) (.lam (.let_ (.app (.const (.perform "S" "op" .tail)) (.lit .unit)) (.lit .unit)))))
      (.ex (.base .unit) (.rt (Row.single "S") Row.none)) := by
  have hS : (Row.single "S").plain = Row.single "S" := by
    funext x; cases x <;> simp [Row.plain, Row.single]
  refine .letGen (k := 1) (Φ := Facts.none)
    (a := .arrow (thunkTy rowVar) (.rt rowVar Row.none) (.base .unit) Caps.none) ?_ ?_
  · refine .lam (.app (C := Caps.none) (.var (T := thunkTy rowVar) (ts := []) rfl rfl
      (fun _ _ h => h.elim) ?_) .lit (Stage.le_refl _) .refl) (fun h => by simp [Tm.Has] at h) ?_ .refl
    · rw [Scheme.inst_mono]; exact .refl
    · rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
      · simp [Tm.Free] at hi
      · simp [Tm.Has] at hp
      · simp [Tm.Has] at hl
  · refine .app (ψ := .rt (Row.single "S") Row.none) (a := thunkTy (Row.single "S")) (C := Caps.none)
      (.var (T := .arrow (thunkTy (Row.single "S")) (.rt (Row.single "S") Row.none) (.base .unit) Caps.none)
        (ts := [.row (Row.single "S")]) rfl rfl (fun _ _ h => h.elim) ?_) ?_ (Stage.le_refl _) .refl
    · simp only [Scheme.inst, Ty.substT, thunkTy, Stage.substT, rowVar_inst, hS, Row.none_substT,
        Caps.none_substT]
      exact .refl
    · refine .lam (ψ := .rt (Row.single "S") Row.none) (.let_ (a := .base .unit)
        (.app (.perform (t := .base .unit) hop (by simpa using Le.refl)) .lit (Stage.le_refl _) .refl) .lit)
        (fun h => by simp [Tm.Has] at h) ?_ .refl
      rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
      · simp [Tm.Free] at hi
      · simp [Tm.Has] at hp
      · simp [Tm.Has] at hl

/-- `Unit →R⟨∅;ε⟩ Unit`. -/
def raiseThunkTy (ε : Row) : Ty := .arrow (.base .unit) (.rt Row.none ε) (.base .unit) Caps.none

/-- `λf. try f () with _ → ()`: the arm `_` removes the row variable `ρ` of the errors of `f`,
so the function raises nothing. -/
example (S : Sig) :
    Typ S [] Facts.none []
      (.lam (.try_ (.app (.var 0) (.lit .unit)) (.acons .wild (.lit .unit) .anil)))
      (.ex (.arrow (raiseThunkTy rowVar) (.rt Row.none Row.none) (.base .unit) Caps.none) .pure) := by
  refine .lam (.tryR (r := Row.none) (ε₀ := rowVar) (H := rowVar) (ε' := Row.none) ?_ ?_
    (Stage.le_refl _) fun n hn => .inl hn) (fun h => by simp [Tm.Has] at h) ?_ .refl
  · refine .app (.var (T := raiseThunkTy rowVar) (ts := []) rfl rfl (fun _ _ h => h.elim) ?_) .lit
      (Stage.le_refl _) .refl
    rw [Scheme.inst_mono]; exact .refl
  · refine .hcons .wild .lit (.hnil (H := Row.none) fun _ h => h) fun n hn => ?_
    subst hn; exact .inl ⟨rfl, rfl⟩
  · rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · simp [Tm.Free] at hi
    · simp [Tm.Has] at hp
    · simp [Tm.Has] at hl

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
  · refine .drec h rfl (.dcons ?_ .dnil) (fun h => by simp [Tm.Has] at h) ?_
    · refine .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) ?_) (fun h => h) ?_ .refl
      · simp [idField, Scheme.inst, Ty.substT, liftT]; exact .refl
      · rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
        · simp [Tm.Free] at hi
        · exact hp.elim
        · exact hl.elim
    · rintro a (⟨i, s, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
      · simp [Tm.Free] at hi
      · simp [Tm.Has] at hp
      · simp [Tm.Has] at hl
  · rintro _ _ ⟨_, _, h, _⟩; exact h.elim
  · simp [idField, Scheme.substT, Scheme.inst, Ty.substT, liftT, instT]; exact .refl

end Flick
