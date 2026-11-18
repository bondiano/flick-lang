import Formal.Syntax

/-!
# Judgment forms of the typing relation (`Rules.lean`)
-/

namespace Flick

/-- Judgment forms: an expression, `match` arms, `try` arms. For `try` arms,
`ε₀` is the error row of the body, a term since an `x` arm binds `Errors ε₀`, and `H` the
errors the arms remove. The fields of a
dictionary record have the schemes `fs`. The arms of a handler of the effect `E` give
the handler's type `t` at its stage `φ`, a stage term since it is also the stage of
`resume`; `resume` holds the region `s` of the handler. A row or a stage that a type
holds is a term, which a rule reads by what it stands for (`Ty.asRow`, `Ty.asStage`). -/
inductive J
  | ex (t : Ty) (φ : Stage)
  | alts (t t' : Ty) (φ : Stage)
  | halts (ε₀ : Ty) (H : Row) (t : Ty) (φ : Stage)
  | dfs (fs : List (Label × Scheme))
  | harms (E : Name) (t : Ty) (φ : Ty) (s : Ty)

end Flick
