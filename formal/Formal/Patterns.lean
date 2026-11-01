import Formal.Canonical

/-!
# Records and pattern matching on well-typed values
-/

namespace Flick

variable {S : Sig}

theorem field_typ {Ψ Γ l} :
    ∀ {v w R t}, Typ S Ψ Γ v (.ex R) → v.field l = some w → R.field l = some t →
      Typ S Ψ Γ w (.ex t)
  | .rcons l' e r, w, _, t, h, hw, hR => by
    cases h with
    | rcons _ he hr =>
      simp only [Tm.field, Ty.field] at hw hR
      split at hw
      · simp_all
      · simp_all only [if_false]; exact field_typ hr hw hR
  | .var _, _, _, _, _, hw, _ | .lam _, _, _, _, _, hw, _ | .app _ _, _, _, _, _, hw, _
  | .let_ _ _, _, _, _, _, hw, _ | .letrec _ _, _, _, _, _, hw, _ | .fix _, _, _, _, _, hw, _
  | .lit _, _, _, _, _, hw, _ | .loc _, _, _, _, _, hw, _ | .const _, _, _, _, _, hw, _
  | .ctor _, _, _, _, _, hw, _ | .rnil, _, _, _, _, hw, _ | .proj _ _, _, _, _, _, hw, _
  | .update _ _ _, _, _, _, _, hw, _ | .match_ _ _, _, _, _, _, hw, _ | .anil, _, _, _, _, hw, _
  | .acons _ _ _, _, _, _, _, hw, _ | .aguard _ _ _ _, _, _, _, _, hw, _
  | .todo, _, _, _, _, hw, _ => by simp [Tm.field] at hw

theorem setField_typ {Ψ Γ l w t} (hw : Typ S Ψ Γ w (.ex t)) :
    ∀ {v v' R}, Typ S Ψ Γ v (.ex R) → v.setField l w = some v' → R.field l = some t →
      Typ S Ψ Γ v' (.ex R)
  | .rcons l' e r, v', _, h, hv, hR => by
    cases h with
    | rcons hl he hr =>
      simp only [Tm.setField, Ty.field] at hv hR
      split at hv
      · simp_all only [if_true, Option.some.injEq]
        subst hv hR; exact .rcons hl hw hr
      · simp_all only [if_false]
        obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp hv
        exact .rcons hl he (setField_typ hw hr hr' hR)
  | .var _, _, _, _, hv, _ | .lam _, _, _, _, hv, _ | .app _ _, _, _, _, hv, _
  | .let_ _ _, _, _, _, hv, _ | .letrec _ _, _, _, _, hv, _ | .fix _, _, _, _, hv, _
  | .lit _, _, _, _, hv, _ | .loc _, _, _, _, hv, _ | .const _, _, _, _, hv, _
  | .ctor _, _, _, _, hv, _ | .rnil, _, _, _, hv, _ | .proj _ _, _, _, _, hv, _
  | .update _ _ _, _, _, _, hv, _ | .match_ _ _, _, _, _, hv, _ | .anil, _, _, _, hv, _
  | .acons _ _ _, _, _, _, hv, _ | .aguard _ _ _ _, _, _, _, hv, _
  | .todo, _, _, _, hv, _ => by simp [Tm.setField] at hv

theorem Shp.field {fl R v l t} (h : Shp S fl R v) (hf : fl = true) (hR : R.field l = some t) :
    (∃ w, v.field l = some w) ∧ ∀ w, ∃ v', v.setField l w = some v' := by
  induction h with
  | rcons _ _ _ ih₂ =>
    simp only [Ty.field] at hR
    split at hR
    · next he => subst he; simp [Tm.field, Tm.setField]
    · next he =>
      obtain ⟨⟨w, hw⟩, hs⟩ := ih₂ rfl hR
      refine ⟨⟨w, by simp [Tm.field, he, hw]⟩, fun w' => ?_⟩
      obtain ⟨v', hv'⟩ := hs w'
      exact ⟨_, by simp only [Tm.setField, he, if_false, hv', Option.map_some]; rfl⟩
  | head => cases hf
  | app => cases hf
  | arrow => simp [Ty.field] at hR
  | ref => simp [Ty.field] at hR
  | lit => simp [Ty.field] at hR
  | rnil => simp [Ty.field] at hR
  | adt => simp [Ty.field] at hR

theorem matchP_app {p q f e δ} : matchP (.app p q) (.app f e) = some δ ↔
    ∃ δ₁ δ₂, matchP p f = some δ₁ ∧ matchP q e = some δ₂ ∧ δ₁ ++ δ₂ = δ := by
  simp [matchP, Option.bind_eq_some_iff]

theorem matchP_rfield {l p r v δ} : matchP (.rfield l p r) v = some δ ↔
    ∃ x δ₁ δ₂, v.field l = some x ∧ matchP p x = some δ₁ ∧ matchP r v = some δ₂ ∧
      δ₁ ++ δ₂ = δ := by
  simp [matchP, Option.bind_eq_some_iff]

theorem matchP_or {p q v δ} : matchP (.or p q) v = some δ ↔
    matchP p v = some δ ∨ (matchP p v = none ∧ matchP q v = some δ) := by
  simp only [matchP]; cases matchP p v <;> simp

/-- Typing of `match(p, v)`: the bound values have the types of `Δ`. For a
constructor spine (flag `false`) the type of the value is determined. -/
theorem PatTy.typ_match {Ψ Γ fl p t Δ} (hp : PatTy S fl p t Δ) :
    ∀ {v δ t'}, Typ S Ψ Γ v (.ex t') → matchP p v = some δ → (fl = true → t' = t) →
      t' = t ∧ Inst S Ψ Γ δ Δ := by
  induction hp with
  | wild => intro _ _ _ _ hm ht; simp [matchP] at hm; subst hm; exact ⟨ht rfl, .nil⟩
  | var =>
    intro _ _ _ hv hm ht; simp [matchP] at hm; subst hm
    obtain rfl := ht rfl; exact ⟨rfl, .cons hv .nil⟩
  | lit =>
    intro v _ _ _ hm ht
    cases v <;> simp [matchP] at hm
    exact ⟨ht rfl, hm.2 ▸ .nil⟩
  | head hK =>
    intro v _ _ hv hm _
    cases v <;> simp [matchP] at hm
    obtain ⟨rfl, rfl⟩ := hm
    cases hv with
    | ctor hK' => rw [hK] at hK'; cases hK'; exact ⟨rfl, .nil⟩
  | app _ _ ih₁ ih₂ =>
    intro v δ _ hv hm _
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp hm
      cases hv with
      | app hf he =>
        obtain ⟨he', i₁⟩ := ih₁ hf h1 (fun h => by cases h)
        cases he'
        obtain ⟨_, i₂⟩ := ih₂ he h2 (fun _ => rfl)
        exact ⟨rfl, i₁.append i₂⟩
    | _ => simp [matchP] at hm
  | adt _ ih => intro _ _ _ hv hm _; exact ih hv hm (fun h => by cases h)
  | rany =>
    intro v _ _ _ hm ht
    cases v <;> simp [matchP] at hm <;> exact ⟨ht rfl, hm ▸ .nil⟩
  | rfield hR _ _ ih₁ ih₂ =>
    intro v δ _ hv hm ht
    obtain rfl := ht rfl
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp hm
    obtain ⟨_, i₁⟩ := ih₁ (field_typ hv hx hR) h1 (fun _ => rfl)
    obtain ⟨_, i₂⟩ := ih₂ hv h2 (fun _ => rfl)
    exact ⟨rfl, i₁.append i₂⟩
  | or _ _ ih₁ ih₂ =>
    intro v δ _ hv hm ht
    rcases matchP_or.mp hm with h | ⟨_, h⟩
    · exact ih₁ hv h ht
    · exact ih₂ hv h ht

end Flick
