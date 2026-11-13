import Formal.Shapes
import Formal.TypeEquiv

/-!
# Type congruence preserves the shapes used by canonical forms
-/

namespace Flick

theorem Ty.RowForm.kind {r : Ty} (h : r.RowForm) : r.kind = 12 := by
  cases h <;> rfl

theorem Ty.Equiv.kind {a b : Ty} (h : Ty.Equiv a b) : a.kind = b.kind := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | run_idem r hr | run_nil r hr | rless_none r hr => exact hr.kind.symm
  | _ => rfl

theorem Ty.RowForm.not_nominal {r : Ty} (h : r.RowForm) (T : Name) : r.hd ≠ .adt T := by
  cases h <;> simp [Ty.hd]

theorem Ty.Equiv.nominalHead {a b : Ty} (h : Ty.Equiv a b) (T : Name) :
    a.hd = .adt T ↔ b.hd = .adt T := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | tapp _ _ ih => exact ih
  | run_idem r hr | run_nil r hr | rless_none r hr =>
    simp [Ty.hd, hr.not_nominal T]
  | _ => simp [Ty.hd]

end Flick
