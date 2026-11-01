import Formal.Generalization

/-!
# A check that (Let-Gen) generalizes

`let id = λx. x in let _ = id 1 in id True` uses `id` at `Int` and at `Bool`.
-/

namespace Flick

example (S : Sig) :
    Typ S [] [] (.let_ (.lam (.var 0))
      (.let_ (.app (.var 0) (.lit (.int 1))) (.app (.var 1) (.lit (.bool true)))))
      (.ex (.base .bool) .pure) := by
  refine .letGen (k := 1) (a := .arrow (.tvar 0) .pure (.tvar 0)) ?_ ?_
  · exact .lam (.var (ts := []) rfl rfl .refl) .refl
  · refine .let_ (a := .base .int) ?_ ?_
    · exact .app (.var (ts := [.base .int]) rfl rfl .refl) .lit trivial .refl
    · exact .app (.var (ts := [.base .bool]) rfl rfl .refl) .lit trivial .refl

end Flick
