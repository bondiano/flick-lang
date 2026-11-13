import Formal.Fresh

/-!
# Freshness modulo type congruence

An intermediate converted type need only be fixed modulo congruence by `kill`.
New store cells still use the actual killed type, which is fixed syntactically.
-/

namespace Flick

def Ty.YFMod (n : Nat) (t : Ty) : Prop := Ty.Equiv (t.substT (kill n)) t

theorem Ty.YF.modulo {n : Nat} {t : Ty} (h : t.YF n) : t.YFMod n := by
  unfold Ty.YFMod
  rw [h]
  exact .refl

theorem Ty.YFMod.equiv {n : Nat} {a b : Ty} (h : a.YFMod n) (he : Ty.Equiv a b) :
    b.YFMod n :=
  ((he.symm.substT (kill n)).trans h).trans he

theorem Ty.YFMod.ref {n : Nat} {a : Ty} (h : (Ty.ref a).YFMod n) : a.YFMod n := by
  unfold Ty.YFMod at h ⊢
  cases h.form with
  | rows hr _ => cases hr
  | ref ha => exact ha

end Flick
