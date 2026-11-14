import Formal.ShapeEquivalence

/-!
# A structural size invariant under row congruence

Rows have size zero. Ordinary type constructors add one, so a variable below
an application or arrow cannot become the whole type by an equivalent row
representation. This is the measure for the occurs check modulo congruence.
-/

namespace Flick

def Ty.shapeSize : Ty → Nat
  | .arrow a _ b _ | .tapp a b | .rcons _ a b => 1 + a.shapeSize + b.shapeSize
  | .ref a | .eff _ _ a _ | .hnd a | .owner a | .dict _ a | .all _ a => 1 + a.shapeSize
  | .rt r e => 1 + r.shapeSize + e.shapeSize
  | .caps _ | .row _ | .cnil | .rnm .. | .run .. | .rless .. => 0
  | _ => 1

theorem Ty.RowForm.shapeSize {r : Ty} (h : r.RowForm) : r.shapeSize = 0 := by
  cases h <;> rfl

theorem Ty.Equiv.shapeSize {a b : Ty} (h : Ty.Equiv a b) : a.shapeSize = b.shapeSize := by
  induction h with
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | run_idem r hr | run_nil r hr | rless_none r hr => exact hr.shapeSize.symm
  | _ => simp_all [Ty.shapeSize]

end Flick
