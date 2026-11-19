import Formal.Proofs.CheckSound

/-! # A replay by hand: the identity law of `map` at `Option` -/

namespace Flick.PT.Examples

open Flick.PT

/-- `Option a` is `unit + a`. -/
def option (a : Ty) : Ty := .sum .unit a

/-- `map f v = match v with | None -> None | Some x -> Some (f x)`. -/
def mapBody : Tm :=
  .lam (.arrow (.var 0) (.var 1)) <| .lam (option (.var 0)) <|
    .case (.var 0) (.inl .unit (.var 1)) (.inr .unit (.app (.var 2) (.var 0)))

def prog : Prog :=
  [ (mapBody, .arrow (.arrow (.var 0) (.var 1)) (.arrow (option (.var 0)) (option (.var 1)))),
    (.lam (.var 0) (.var 0), .arrow (.var 0) (.var 0)) ]

/-- `map identity v = v` for every `v : Option a`. -/
def identityLaw : Stmt where
  ctx := [option (.var 0)]
  facts := []
  lhs := .app (.app (.glob 0 [.var 0, .var 0]) (.glob 1 [.var 0])) (.var 0)
  rhs := .var 0
  ty := option (.var 0)

/-- A split of `v`, and of the `unit` a `None` holds. -/
def identityProof : Cert := .split 0 (.split 0 .refl .refl) .refl

theorem prog_wf : prog.WF := Prog.wf?_sound (by decide)

theorem identityLaw_holds : identityLaw.Holds prog.bodies :=
  check_sound prog_wf (L := []) (by simp) identityProof identityLaw (k := 64) (by decide)

end Flick.PT.Examples
