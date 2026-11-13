import Formal.CanonForms
import Formal.TypeEquivInvert

/-!
# Record operations through type conversion

Congruent record types preserve their labels. A field keeps its type modulo
congruence; an update converts its argument back to the stored field type.
-/

namespace Flick

variable {S : Sig}

theorem Ty.Equiv.field_source {a b : Ty} (he : Ty.Equiv a b) {l t}
    (hb : b.field l = some t) : ∃ u, a.field l = some u ∧ Ty.Equiv u t := by
  have hr := he.field l
  rw [hb] at hr
  cases ha : a.field l with
  | none => rw [ha] at hr; cases hr
  | some u => rw [ha] at hr; exact ⟨u, rfl, Option.rel_some_some.mp hr⟩

private theorem Typ.fieldCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ R φ l w t, J = .ex R φ → e.field l = some w → R.field l = some t →
      Typ S Ψ Θ Γ w (.ex t φ) := by
  induction h with
  | conv _ he ih =>
    intro R φ l w t hJ hw hR; cases hJ
    obtain ⟨u, hu, hut⟩ := he.field_source hR
    exact (ih _ _ _ _ _ rfl hw hu).conv hut
  | rcons _ _ he _ _ ih =>
    intro R φ l w t hJ hw hR; cases hJ
    simp only [Tm.field, Ty.field] at hw hR
    split at hw
    · simp_all
    · next hne => simp only [hne, if_false] at hR; exact ih _ _ _ _ _ rfl hw hR
  | allI | drec | errVal => intro R φ l w t hJ _ hR; cases hJ; simp [Ty.field] at hR
  | _ => intro R φ l w t hJ hw _; cases hJ <;> simp [Tm.field] at hw

theorem field_typ {Ψ Θ Γ l φ} {v w R t} (h : Typ S Ψ Θ Γ v (.ex R φ))
    (hw : v.field l = some w) (hR : R.field l = some t) : Typ S Ψ Θ Γ w (.ex t φ) :=
  h.fieldCore R φ l w t rfl hw hR

private theorem Typ.valueFieldCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Value e → ∀ R φ l w t, J = .ex R φ → e.field l = some w → R.field l = some t → Value w := by
  induction h with
  | conv _ he ih =>
    intro hv R φ l w t hJ hw hR; cases hJ
    obtain ⟨u, hu, _⟩ := he.field_source hR
    exact ih hv _ _ _ _ _ rfl hw hu
  | rcons _ _ _ _ _ ih =>
    intro hv R φ l w t hJ hw hR; cases hJ
    cases hv with
    | rcons hv₁ hv₂ =>
      simp only [Tm.field, Ty.field] at hw hR
      split at hw
      · cases hw; exact hv₁
      · next hne => simp only [hne, if_false] at hR; exact ih hv₂ _ _ _ _ _ rfl hw hR
  | allI | drec | errVal => intro _ R φ l w t hJ _ hR; cases hJ; simp [Ty.field] at hR
  | _ => intro _ R φ l w t hJ hw _; cases hJ <;> simp [Tm.field] at hw

/-- A field of a record value is a value. A dictionary is not a record. -/
theorem Value.field {Ψ Θ Γ l φ v w R t} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex R φ))
    (hw : v.field l = some w) (hR : R.field l = some t) : Value w :=
  h.valueFieldCore hv R φ l w t rfl hw hR

private theorem Typ.setFieldCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ R φ l w t v', J = .ex R φ → Typ S Ψ Θ Γ w (.ex t φ) →
      e.setField l w = some v' → R.field l = some t → Typ S Ψ Θ Γ v' (.ex R φ) := by
  induction h with
  | conv _ he ih =>
    intro R φ l w t v' hJ hw hv hR; cases hJ
    obtain ⟨u, hu, hut⟩ := he.field_source hR
    exact (ih _ _ _ _ _ _ rfl (hw.conv hut.symm) hv hu).conv he
  | rcons hRec hl he _ _ ih =>
    intro R φ l w t v' hJ hw hv hR; cases hJ
    simp only [Tm.setField, Ty.field] at hv hR
    split at hv
    · simp_all only [if_true, Option.some.injEq]
      subst hv hR; exact .rcons hRec hl hw ‹_›
    · simp_all only [if_false]
      obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp hv
      exact .rcons hRec hl he (ih _ _ _ _ _ _ rfl hw hr' hR)
  | allI | errVal => intro R φ l w t v' hJ _ _ hR; cases hJ; simp [Ty.field] at hR
  | _ => intro R φ l w t v' hJ _ hv _; cases hJ <;> simp [Tm.setField] at hv

theorem setField_typ {Ψ Θ Γ l w t φ} (hw : Typ S Ψ Θ Γ w (.ex t φ))
    {v v' R} (h : Typ S Ψ Θ Γ v (.ex R φ)) (hv : v.setField l w = some v')
    (hR : R.field l = some t) : Typ S Ψ Θ Γ v' (.ex R φ) :=
  h.setFieldCore R φ l w t v' rfl hw hv hR

end Flick
