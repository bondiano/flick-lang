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

end Flick
