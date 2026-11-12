import Formal.Canonical

/-!
# Widening the hidden row of data

Subtyping may widen the hidden row of data with hidden storage and nothing else
(regions.md §2.2, `LeLive.hide`). The row stands in the fields only where widening it
widens them (`Ty.CovRow`), so a value built at a narrower row stores fields below the
fields at the wider one (`field_widen`), and its constructor builds the wider type too
(`Sig.builds_vary` in `Vary`).
-/

namespace Flick

variable {S : Sig}

/-- Substitutions that agree but at `v i` agree on what does not depend on it. -/
theorem Ty.NoV.substT_eq {i : Nat} {t : Ty} (h : t.NoV i) {σ σ' : Atom → Ty}
    (hσ : ∀ a, a ≠ .v i → σ a = σ' a) : t.substT σ = t.substT σ' := by
  have h' : t.substT (dropV i) = t := h
  rw [← h', Ty.substT_comp, Ty.substT_comp]
  apply Ty.substT_congr
  intro a
  by_cases ha : a = .v i
  · subst ha; simp [dropV, Ty.substT]
  · simp only [dropV, ha, if_false]
    cases a <;> simp [Ty.ident, Ty.substT, hσ _ ha]

theorem Stage.NoV.substT_eq {i : Nat} {φ : Stage} (h : φ.NoV i) {σ σ' : Atom → Ty}
    (hσ : ∀ a, a ≠ .v i → σ a = σ' a) : φ.substT σ = φ.substT σ' := by
  have h' : φ.substT (dropV i) = φ := h
  rw [← h', Stage.substT_comp, Stage.substT_comp]
  congr 1; funext a
  by_cases ha : a = .v i
  · subst ha; simp [dropV, Ty.substT]
  · simp only [dropV, ha, if_false]
    cases a <;> simp [Ty.ident, Ty.substT, hσ _ ha]

/-- Two instances that differ only at `i` agree at every other atom. -/
theorem instT_agree {i : Nat} {us us' : List Ty} (hl : us.length = us'.length)
    (heq : ∀ j, j ≠ i → us[j]? = us'[j]?) : ∀ a, a ≠ .v i → instT us a = instT us' a := by
  intro a ha
  cases a with
  | v j =>
    have hj : j ≠ i := fun h => ha (h ▸ rfl)
    have e := heq j hj
    by_cases h1 : j < us.length
    · have h2 : j < us'.length := hl ▸ h1
      rw [List.getElem?_eq_getElem h1, List.getElem?_eq_getElem h2] at e
      simp only [instT, h1, h2, dite_true]
      exact Option.some.inj e
    · have h2 : ¬ j < us'.length := hl ▸ h1
      simp only [instT, h2, dite_false, hl]
  | r p => rfl

theorem reach_pointwise {i : Nat} {σ σ' : Atom → Ty} (hσ : ∀ a, a ≠ .v i → σ a = σ' a)
    (hr : Caps.Sub (σ (.v i)).reach (σ' (.v i)).reach) : ∀ b, Caps.Sub (σ b).reach (σ' b).reach := by
  intro b
  by_cases hb : b = .v i
  · subst hb; exact hr
  · rw [hσ b hb]; exact Caps.Sub.refl _

theorem Ty.reach_substT_mono {t : Ty} {σ σ' : Atom → Ty}
    (h : ∀ b, Caps.Sub (σ b).reach (σ' b).reach) :
    Caps.Sub (t.substT σ).reach (t.substT σ').reach := fun a ha => by
  obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ t a).mp ha
  exact (Ty.reach_substT σ' t a).mpr ⟨b, hb, h b a h'⟩

theorem Caps.substT_widen {C : Caps} {σ σ' : Atom → Ty}
    (h : ∀ b, Caps.Sub (σ b).reach (σ' b).reach) : Caps.Sub (C.substT σ) (C.substT σ') :=
  fun x ⟨b, hb, h'⟩ => ⟨b, hb, h b x h'⟩

/-- A field at a narrower hidden row is below the field at a wider one. -/
theorem field_widen {i : Nat} {f : Ty} (hf : f.CovRow S.hidden i) {σ σ' : Atom → Ty}
    (hσ : ∀ a, a ≠ .v i → σ a = σ' a) (hr : Caps.Sub (σ (.v i)).reach (σ' (.v i)).reach) :
    LeLive S (f.substT σ) (f.substT σ') := by
  have hp := reach_pointwise hσ hr
  rcases hf with h | ⟨a, φ, b, C, rfl, ha, hφ, hb⟩ | ⟨r, e, t, C, rfl, hr', he, ht⟩ |
      ⟨T, us, j, rfl, hj, hus⟩
  · rw [h.substT_eq hσ]; exact .refl
  · simp only [Ty.substT]
    rw [ha.substT_eq hσ, hφ.substT_eq hσ, hb.substT_eq hσ]
    have hC := Caps.substT_widen (C := C) hp
    refine .arrow .refl (Stage.use_mono (Stage.le_refl _) hC) .refl fun x hx => ?_
    rcases hx with hx | hx | hx
    · exact .inl hx
    · exact .inr (.inl hx)
    · exact .inr (.inr (hC x hx))
  · simp only [Ty.substT]
    have er := hr'.substT_eq hσ
    have ee := he.substT_eq hσ
    simp only [Ty.substT, Ty.row.injEq] at er ee
    rw [er, ee, ht.substT_eq hσ]
    have hC := Caps.substT_widen (C := C) hp
    refine .eff (Stage.use_mono (Stage.le_refl _) hC) .refl fun x hx => ?_
    rcases hx with hx | hx
    · exact .inl hx
    · exact .inr (hC x hx)
  · rw [Ty.tcon_substT, Ty.tcon_substT]
    refine .hide hj (by simp) (fun l hl => ?_) (fun r r' h h' => ?_) rfl rfl
    · simp only [List.getElem?_map]
      cases hu : us[l]? with
      | none => rfl
      | some u => simp [(hus l u hu hl).substT_eq hσ]
    · simp only [List.getElem?_map] at h h'
      cases hu : us[j]? with
      | none => rw [hu] at h; cases h
      | some u =>
        rw [hu] at h h'; cases h; cases h'
        exact Ty.reach_substT_mono hp

end Flick
