import Formal.CanonLayer

/-!
# Records and pattern matching on well-typed values
-/

namespace Flick

variable {S : Sig}

theorem field_typ {Ψ Γ l φ} :
    ∀ {v w R t}, Typ S Ψ Γ v (.ex R φ) → v.field l = some w → R.field l = some t →
      Typ S Ψ Γ w (.ex t φ) := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro w R t h hw hR
    cases h with
    | rcons _ he hr =>
      simp only [Tm.field, Ty.field] at hw hR
      split at hw
      · simp_all
      · next hne => simp only [hne, if_false] at hR; exact ih hr hw hR
    | errVal => simp [Ty.field] at hR
  | _ => intro _ _ _ _ hw; simp [Tm.field] at hw

theorem setField_typ {Ψ Γ l w t φ} (hw : Typ S Ψ Γ w (.ex t φ)) :
    ∀ {v v' R}, Typ S Ψ Γ v (.ex R φ) → v.setField l w = some v' → R.field l = some t →
      Typ S Ψ Γ v' (.ex R φ) := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro v' R h hv hR
    cases h with
    | rcons hl he hr =>
      simp only [Tm.setField, Ty.field] at hv hR
      split at hv
      · simp_all only [if_true, Option.some.injEq]
        subst hv hR; exact .rcons hl hw hr
      · simp_all only [if_false]
        obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp hv
        exact .rcons hl he (ih hr hr' hR)
    | errVal => simp [Ty.field] at hR
  | _ => intro _ _ _ hv; simp [Tm.setField] at hv

/-- Typing of `match(p, v)`. For a constructor spine (flag `false`) the type of
the pattern is below the type of the value. -/
theorem PatTy.typ_match {Ψ Γ fl p t Δ} (hp : PatTy S fl p t Δ) :
    ∀ {v δ t'}, Typ S Ψ Γ v (.ex t' .pure) → matchP p v = some δ →
      (fl = true → t' = t) → (fl = false → t'.kind ≠ 7) → Le t t' ∧ Inst S Ψ Γ δ Δ := by
  induction hp with
  | wild =>
    intro _ _ _ _ hm ht _; simp [matchP] at hm; subst hm; obtain rfl := ht rfl
    exact ⟨.refl, .nil⟩
  | var =>
    intro _ _ _ hv hm ht _; simp [matchP] at hm; subst hm
    obtain rfl := ht rfl; exact ⟨.refl, .cons hv .nil⟩
  | lit =>
    intro v _ _ _ hm ht _
    obtain rfl := ht rfl
    cases v <;> simp [matchP] at hm
    exact ⟨.refl, hm.2 ▸ .nil⟩
  | head hK =>
    intro v _ _ hv hm _ hk
    cases v <;> simp [matchP] at hm
    obtain ⟨rfl, rfl⟩ := hm
    cases hv with
    | ctor hK' hT => rw [hK] at hK'; cases hK'; exact ⟨hT, .nil⟩
    | errVal => exact absurd rfl (hk rfl)
  | app _ _ ih₁ ih₂ =>
    intro v δ _ hv hm _ hk
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp hm
      cases hv with
      | app hf he hψ hT =>
        obtain rfl := Stage.eq_pure hψ
        obtain ⟨hl, i₁⟩ := ih₁ hf h1 (fun h => by cases h) (fun _ => by simp [Ty.kind])
        obtain ⟨_, _, _, h, la, _, lt⟩ := hl.arrow_left
        cases h
        obtain ⟨_, i₂⟩ := ih₂ (he.sub ⟨la, Stage.le_refl _⟩) h2 (fun _ => rfl) (fun h => by cases h)
        exact ⟨lt.trans hT, i₁.append i₂⟩
      | errVal => exact absurd rfl (hk rfl)
    | _ => simp [matchP] at hm
  | adt _ ih =>
    intro _ _ _ hv hm ht _
    obtain rfl := ht rfl
    exact ih hv hm (fun h => by cases h) (fun _ => by simp [Ty.kind])
  | rany =>
    intro v _ _ _ hm ht _
    obtain rfl := ht rfl
    cases v <;> simp [matchP] at hm <;> exact ⟨.refl, hm ▸ .nil⟩
  | rfield hR _ _ ih₁ ih₂ =>
    intro v δ _ hv hm ht _
    obtain rfl := ht rfl
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp hm
    obtain ⟨_, i₁⟩ := ih₁ (field_typ hv hx hR) h1 (fun _ => rfl) (fun h => by cases h)
    obtain ⟨_, i₂⟩ := ih₂ hv h2 (fun _ => rfl) (fun h => by cases h)
    exact ⟨.refl, i₁.append i₂⟩
  | or _ _ ih₁ ih₂ =>
    intro v δ _ hv hm ht hk
    rcases matchP_or.mp hm with h | ⟨_, h⟩
    · exact ih₁ hv h ht hk
    · exact ih₂ hv h ht hk

/-- Matching at the type of a value. -/
theorem PatTy.inst {Ψ Γ p t Δ v δ} (hp : PatTy S true p t Δ) (hv : Typ S Ψ Γ v (.ex t .pure))
    (hm : matchP p v = some δ) : Inst S Ψ Γ δ Δ :=
  (hp.typ_match hv hm (fun _ => rfl) (fun h => by cases h)).2

/-- Matching a raised value against a `try` arm. -/
theorem HPat.inst {Ψ Γ ε₀ p θ Δ v δ} (hp : HPat S ε₀ p θ Δ) (hvv : Value v)
    (hv : Typ S Ψ Γ v (.ex (.errors ε₀) .pure)) (hm : matchP p v = some δ) :
    Inst S Ψ Γ δ Δ ∧ Typ S Ψ Γ v (.ex θ .pure) := by
  cases hp with
  | wild => simp [matchP] at hm; subst hm; exact ⟨.nil, hv⟩
  | var => simp [matchP] at hm; subst hm; exact ⟨.cons hv .nil, hv⟩
  | ctor hp =>
    obtain ⟨_, _, _, _, _, _, hv'⟩ := canonical_errors hvv hv
    obtain ⟨hl, i⟩ := hp.typ_match hv' hm (fun h => by cases h) (fun _ => by simp [Ty.kind])
    cases hl; exact ⟨i, hv'⟩

end Flick
