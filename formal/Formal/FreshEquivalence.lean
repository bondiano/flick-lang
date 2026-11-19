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

/-- An error of a row fixed by `kill n` is fixed by it: the row's elements are what `kill n`
makes of them, which it fixes. -/
theorem Ty.YFMod.of_err {n : Nat} {ε U : Ty} {E : Name} (h : (Ty.errors ε).YFMod n)
    (hE : Row.Sub (Row.err E U) ε.asRow) : U.YFMod n := by
  unfold Ty.YFMod at h ⊢
  obtain ⟨_, he, hε⟩ := h.errors_right
  injection he with he
  subst he
  have hm := hE _ (.inl rfl)
  rw [← hε.asRow, ← Row.substT_asRow] at hm
  simp only [Row.substT] at hm
  rcases hm with ⟨c₀, -, hc⟩ | ⟨b, G, -, -, hb⟩
  · obtain ⟨t₀, rfl⟩ := Quotient.exists_rep c₀
    have ht := Ty.classOf_eq.mp (show (t₀.substT (kill n)).classOf = U.classOf from hc)
    have hk : Ty.Equiv ((t₀.substT (kill n)).substT (kill n)) (U.substT (kill n)) :=
      ht.substT (kill n)
    rw [Ty.yf_kill n t₀] at hk
    exact hk.symm.trans ht
  · cases kill_asRow hb

end Flick
