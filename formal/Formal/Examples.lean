import Formal.Generalization

/-!
# Checks that the rules are usable

`let id = λx. x in let _ = id 1 in id True` uses `id` at `Int` and at `Bool`.
`scope s { op (open s) }` acquires a capability of its region and uses it: its
type is `Unit`, and `Scope s` leaves the row of the scope.
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
