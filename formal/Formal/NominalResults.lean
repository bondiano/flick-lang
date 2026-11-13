import Formal.ShapeEquivalence

/-!
# Nominal results through type conversion

Conversion preserves the final result of an arrow spine modulo congruence.
A nominal result retains its constructor and pointwise congruent arguments.
-/

namespace Flick

theorem Ty.tcon_res (T : Name) (us : List Ty) : (Ty.tcon T us).res = Ty.tcon T us := by
  have h := Ty.tcon_not_arrow T us
  generalize Ty.tcon T us = t at h
  cases t with
  | arrow => exact absurd rfl (h _ _ _ _)
  | _ => rfl

theorem Ty.Equiv.res_tcon {a b : Ty} (he : Ty.Equiv a b) {T us}
    (ha : a.res = Ty.tcon T us) :
    ∃ vs, b.res = Ty.tcon T vs ∧ Ty.ArgsEquiv us vs := by
  have hr := he.res
  rw [ha] at hr
  exact hr.tcon_left

theorem Ty.not_all_of_res_tcon {a : Ty} {T us} (ha : a.res = Ty.tcon T us) :
    ¬ a.isAll := by
  cases a with
  | all => exact (Ty.res_tcon_not_all ha).elim
  | _ => simp [Ty.isAll]

end Flick
