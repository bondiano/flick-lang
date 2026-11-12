import Formal.CanonForms
import Formal.Semantics

/-!
# Records and pattern matching on well-typed values
-/

namespace Flick

variable {S : Sig}

theorem field_typ {Ψ Θ Γ l φ} :
    ∀ {v w R t}, Typ S Ψ Θ Γ v (.ex R φ) → v.field l = some w → R.field l = some t →
      Typ S Ψ Θ Γ w (.ex t φ) := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro w R t h hw hR
    cases h with
    | rcons _ _ he hr =>
      simp only [Tm.field, Ty.field] at hw hR
      split at hw
      · simp_all
      · next hne => simp only [hne, if_false] at hR; exact ih hr hw hR
    | allI hv => simp [Ty.field] at hR
    | errVal => simp [Ty.field] at hR
  | drec =>
    intro _ _ _ h _ hR
    cases h with
    | drec => simp [Ty.field] at hR
    | allI hv => simp [Ty.field] at hR
    | errVal => simp [Ty.field] at hR
  | _ => intro _ _ _ _ hw; simp [Tm.field] at hw

/-- A field of a record value is a value. A dictionary record is not a record. -/
theorem Value.field {Ψ Θ Γ l φ} : ∀ {v w R t}, Value v → Typ S Ψ Θ Γ v (.ex R φ) →
    v.field l = some w → R.field l = some t → Value w := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro w R t hv h hw hR
    cases hv with
    | rcons hv₁ hv₂ =>
      cases h with
      | rcons _ _ _ hr =>
        simp only [Tm.field, Ty.field] at hw hR
        split at hw
        · cases hw; exact hv₁
        · next hne => simp only [hne, if_false] at hR; exact ih hv₂ hr hw hR
      | allI hv => simp [Ty.field] at hR
      | errVal => simp [Ty.field] at hR
  | drec =>
    intro _ _ _ _ h _ hR
    cases h with
    | drec => simp [Ty.field] at hR
    | allI hv => simp [Ty.field] at hR
    | errVal => simp [Ty.field] at hR
  | _ => intro _ _ _ _ _ hw; simp [Tm.field] at hw

theorem setField_typ {Ψ Θ Γ l w t φ} (hw : Typ S Ψ Θ Γ w (.ex t φ)) :
    ∀ {v v' R}, Typ S Ψ Θ Γ v (.ex R φ) → v.setField l w = some v' → R.field l = some t →
      Typ S Ψ Θ Γ v' (.ex R φ) := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro v' R h hv hR
    cases h with
    | rcons hRec hl he hr =>
      simp only [Tm.setField, Ty.field] at hv hR
      split at hv
      · simp_all only [if_true, Option.some.injEq]
        subst hv hR; exact .rcons hRec hl hw hr
      · simp_all only [if_false]
        obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp hv
        exact .rcons hRec hl he (ih hr hr' hR)
    | allI hv => simp [Ty.field] at hR
    | errVal => simp [Ty.field] at hR
  | _ => intro _ _ _ hv; simp [Tm.setField] at hv

theorem Ty.foldl_tapp_inj : ∀ {us us' : List Ty} {f f' : Ty}, us.length = us'.length →
    us.foldl .tapp f = us'.foldl .tapp f' → f = f' ∧ us = us'
  | [], [], _, _, _, h => ⟨h, rfl⟩
  | x :: xs, y :: ys, f, f', hl, h => by
    obtain ⟨h₁, h₂⟩ := Ty.foldl_tapp_inj (us := xs) (us' := ys) (by simpa using hl) h
    cases h₁; exact ⟨rfl, by rw [h₂]⟩

theorem Ty.tcon_inj {T : Name} {us us' : List Ty} (hl : us.length = us'.length)
    (h : Ty.tcon T us = Ty.tcon T us') : us = us' :=
  (Ty.foldl_tapp_inj hl h).2

theorem Ty.tcon_res (T : Name) (us : List Ty) : (Ty.tcon T us).res = Ty.tcon T us := by
  have h := Ty.tcon_not_arrow T us
  generalize Ty.tcon T us = t at h
  cases t with
  | arrow => exact absurd rfl (h _ _ _ _)
  | _ => rfl

theorem Ty.tcon_not_eff (T : Name) (us : List Ty) : ∀ r e t C, Ty.tcon T us ≠ .eff r e t C := by
  intro r e t C h
  have := Ty.tcon_kind T us; rw [h] at this; cases this

/-- Subtyping keeps the final type of data, up to widening its hidden row. -/
theorem LeLive.res_tcon {a b : Ty} {T : Name} {us : List Ty}
    (h : LeLive S a b) (ha : a.res = Ty.tcon T us) :
    ∃ us', b.res = Ty.tcon T us' ∧ S.Vary T us us' := by
  induction h with
  | refl => exact ⟨us, ha, .refl⟩
  | arrow _ _ _ _ _ ih => exact ih ha
  | eff => exact absurd ha.symm (Ty.tcon_not_eff _ _ _ _ _ _)
  | hide hi hl heq hsub ha' hb' =>
    subst ha'; subst hb'
    rw [Ty.tcon_res] at ha ⊢
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq ha
    exact ⟨_, rfl, .of_hide hi hl heq hsub⟩
  | data hl hr hp hn hi ha' hb' =>
    subst ha'; subst hb'
    rw [Ty.tcon_res] at ha ⊢
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq ha
    exact ⟨_, rfl, hl, hr, hp, hn, hi⟩

theorem LeLive.res_tcon' {a b : Ty} {T : Name} {us : List Ty}
    (h : LeLive S a b) (hb : b.res = Ty.tcon T us) :
    ∃ us₀, a.res = Ty.tcon T us₀ ∧ S.Vary T us₀ us := by
  induction h with
  | refl => exact ⟨us, hb, .refl⟩
  | arrow _ _ _ _ _ ih => exact ih hb
  | eff => exact absurd hb.symm (Ty.tcon_not_eff _ _ _ _ _ _)
  | hide hi hl heq hsub ha' hb' =>
    subst ha'; subst hb'
    rw [Ty.tcon_res] at hb ⊢
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq hb
    exact ⟨_, rfl, .of_hide hi hl heq hsub⟩
  | data hl hr hp hn hi ha' hb' =>
    subst ha'; subst hb'
    rw [Ty.tcon_res] at hb ⊢
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq hb
    exact ⟨_, rfl, hl, hr, hp, hn, hi⟩

/-- The final type of a value's spine is the pattern's, or narrower at the hidden row. -/
def ResBelow (S : Sig) (t' t : Ty) : Prop :=
  t'.res = t.res ∨ ∃ T ws ws', t.res = Ty.tcon T ws' ∧ t'.res = Ty.tcon T ws ∧ S.Vary T ws ws'

theorem ResBelow.tcon {t' t : Ty} {T : Name} {ws' : List Ty} (h : ResBelow S t' t)
    (ht : t.res = Ty.tcon T ws') : ∃ ws, t'.res = Ty.tcon T ws ∧ S.Vary T ws ws' := by
  rcases h with h | ⟨T₁, ws, ws₁, h₁, h₂, hw⟩
  · exact ⟨ws', h.trans ht, .refl⟩
  · obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (h₁.symm.trans ht)
    exact ⟨ws, h₂, hw⟩
theorem Ty.tcon_eq_adt {T T' : Name} {us : List Ty} (h : Ty.tcon T us = .adt T') :
    us = [] ∧ T = T' := by
  cases us with
  | nil => simp [Ty.tcon] at h; exact ⟨rfl, h⟩
  | cons x xs =>
    have : ∀ (xs : List Ty) (acc : Ty), (∃ g b, acc = Ty.tapp g b) →
        ∃ g b, xs.foldl Ty.tapp acc = Ty.tapp g b := by
      intro xs; induction xs with
      | nil => intro acc h; exact h
      | cons y ys ih => intro acc _; exact ih _ ⟨_, _, rfl⟩
    obtain ⟨g, b, hg⟩ := this xs (.tapp (.adt T) x) ⟨_, _, rfl⟩
    simp only [Ty.tcon, List.foldl] at h; rw [hg] at h; cases h

/-- A type without parameters is below only itself. -/
theorem LeLive.adt_eq {T : Name} {U : Ty} (h : LeLive S (.adt T) U) : U = .adt T := by
  generalize hX : Ty.adt T = X at h
  cases h with
  | refl => rfl
  | arrow => cases hX
  | eff => cases hX
  | hide _ hl _ _ ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq_adt (ha.symm.trans hX.symm)
    rw [List.eq_nil_of_length_eq_zero hl.symm]; exact hX
  | data hl _ _ _ _ ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq_adt (ha.symm.trans hX.symm)
    rw [List.eq_nil_of_length_eq_zero hl.symm]; exact hX

/-- A narrower spine at a type without parameters is the type. -/
theorem SpW.adt {Z : Ty} {T : Name} (h : SpW S Z (.adt T)) : Z = .adt T := by
  generalize hY : Ty.adt T = Y at h
  cases h with
  | refl => rfl
  | res hw =>
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq_adt hY.symm
    rw [List.eq_nil_of_length_eq_zero hw.length]
  | arrow => cases hY

/-- The final type of a typed constructor spine is its type at an instance. -/
theorem Typ.spine_res {Ψ Θ Γ v J} (h : Typ S Ψ Θ Γ v J) :
    Value v → ∀ {K k ts T X φ}, v.head = .ctor K → S.ctor K = some (k, ts, T) →
      J = .ex X φ → X.kind ≠ 7 → ¬ X.isAll →
        ∃ us, us.length = k ∧ ∃ ws, X.res = Ty.tcon T ws ∧
          S.Vary T ((S.res K).map (Ty.substT (instT us))) ws := by
  induction h with
  | ctor hK hk _ hT =>
    intro _ _ _ _ _ _ _ hK' hS hJ _ _
    cases hJ; simp only [Tm.head] at hK'; cases hK'
    rw [hK] at hS; cases hS
    exact ⟨_, hk, hT.res_tcon (Sig.ctorTyR_res _ _ _ _)⟩
  | app hf _ _ hT ih₁ _ =>
    intro hv _ _ _ _ _ _ hK hS hJ hk _
    cases hJ
    cases hv with
    | ctorApp hvf hK' _ =>
      simp only [Tm.head] at hK
      obtain ⟨us, hl, ws, hr, hw⟩ := ih₁ hvf hK hS rfl (by simp [Ty.kind]) (by simp [Ty.isAll])
      simp only [Ty.res] at hr
      obtain ⟨ws', hr', hw'⟩ := hT.res_tcon hr
      exact ⟨us, hl, ws', hr', hw.trans hw'⟩
    | constApp => simp [Tm.head] at hK
  | errVal => intro _ _ _ _ _ _ _ _ _ hJ hk; cases hJ; simp [Ty.kind] at hk
  | allI => intro _ _ _ _ _ _ _ _ _ hJ _ hA; cases hJ; exact (hA trivial).elim
  | _ => intro hv _ _ _ _ _ _ hK; cases hv <;> simp [Tm.head] at hK

/-- The head of a constructor pattern, and the final type of its spine. -/
theorem PatTy.spine {fl p t Δ} (hp : PatTy S fl p t Δ) (hf : fl = false) :
    ∃ K k ts T us, S.ctor K = some (k, ts, T) ∧ us.length = k ∧
      t.res = Ty.tcon T ((S.res K).map (Ty.substT (instT us))) ∧
      ∀ v δ, matchP p v = some δ → v.head = .ctor K := by
  induction hp with
  | head hK _ hk =>
    refine ⟨_, _, _, _, _, hK, hk, Sig.ctorTyR_res _ _ _ _, fun v δ hm => ?_⟩
    cases v <;> simp [matchP] at hm; simp [Tm.head, hm.1]
  | app _ _ ih _ =>
    obtain ⟨K, k, ts, T, us, hK, hk, hr, hm⟩ := ih rfl
    refine ⟨K, k, ts, T, us, hK, hk, hr, fun v δ h => ?_⟩
    cases v with
    | app f e =>
      obtain ⟨δ₁, _, h₁, -⟩ := matchP_app.mp h
      simp only [Tm.head]; exact hm f δ₁ h₁
    | _ => simp [matchP] at h
  | _ => cases hf

/-- Typing of `match(p, v)`. For a constructor spine (flag `false`) the type of
the pattern is below the type of the value, at the same instance. -/
theorem PatTy.typ_match {Ψ Θ Γ fl p t Δ} (hp : PatTy S fl p t Δ) :
    ∀ {v δ t'}, Value v → Typ S Ψ Θ Γ v (.ex t' .pure) → matchP p v = some δ →
      (fl = true → t' = t) → (fl = false → t'.kind ≠ 7 ∧ ResBelow S t' t) →
      (∃ Z, LeLive S Z t' ∧ SpW S Z t) ∧ Inst S Ψ Θ Γ δ Δ := by
  induction hp with
  | wild =>
    intro _ _ _ _ _ hm ht _; simp [matchP] at hm; subst hm; obtain rfl := ht rfl
    exact ⟨⟨_, LeLive.refl, .refl⟩, .nil⟩
  | var =>
    intro _ _ _ hvv hv hm ht _; simp [matchP] at hm; subst hm
    obtain rfl := ht rfl; exact ⟨⟨_, LeLive.refl, .refl⟩, .cons hvv hv .nil⟩
  | lit =>
    intro v _ _ _ _ hm ht _
    obtain rfl := ht rfl
    cases v <;> simp [matchP] at hm
    exact ⟨⟨_, LeLive.refl, .refl⟩, hm.2 ▸ .nil⟩
  | head hK hj hk =>
    intro v _ _ _ hv hm _ hk'
    cases v <;> simp [matchP] at hm
    obtain ⟨rfl, rfl⟩ := hm
    obtain ⟨hk7, hres⟩ := hk' rfl
    cases hv with
    | ctor hK' hk'' _ hT =>
      rw [hK] at hK'; cases hK'
      obtain ⟨ws₁, h₁, hw₁⟩ := hT.res_tcon (Sig.ctorTyR_res _ _ _ _)
      obtain ⟨ws, h₂, hw⟩ := hres.tcon (Sig.ctorTyR_res _ _ _ _)
      obtain rfl := (Ty.tcon_eq (h₁.symm.trans h₂)).2
      -- the instance of what it builds widens to the instance of the pattern
      exact ⟨⟨_, hT, SpW.ctor hK hk'' (Sig.Vary.of_map hj hK hk'' hk (hw₁.trans hw))⟩, .nil⟩
    | allI =>
      obtain ⟨_, h₂, -⟩ := hres.tcon (Sig.ctorTyR_res _ _ _ _)
      exact (Ty.res_tcon_not_all h₂).elim
    | errVal => exact absurd rfl hk7
  | app _ _ ih₁ ih₂ =>
    intro v δ _ hvv hv hm _ hk
    obtain ⟨hk7, hres⟩ := hk rfl
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp hm
      cases hv with
      | app hf he hψ hT =>
        obtain rfl := Stage.eq_pure (Stage.le_trans (Stage.le_use _ _) hψ)
        have hvf : Value f ∧ Value e := by
          cases hvv with
          | ctorApp hf' _ he' => exact ⟨hf', he'⟩
          | constApp _ he' => exact ⟨.const, he'⟩
        obtain ⟨_, _, _, _, _, _, _, hr, _⟩ := (‹PatTy S false _ _ _›).spine rfl
        simp only [Ty.res] at hr
        obtain ⟨ws, hw1, hww⟩ := hres.tcon hr
        obtain ⟨ws₀, hres', hw0⟩ := hT.res_tcon' hw1
        obtain ⟨⟨Z, hZ, hs⟩, i₁⟩ := ih₁ hvf.1 hf h1 (fun h => by cases h)
          (fun _ => ⟨by simp [Ty.kind], .inr ⟨_, _, _, hr, hres', hw0.trans hww⟩⟩)
        obtain ⟨_, _, _, _, h, la, Z', lt, hs'⟩ := hs.arrow_step hZ
        cases h
        obtain ⟨_, i₂⟩ := ih₂ hvf.2 (he.sub ⟨la, Stage.le_refl _⟩) h2 (fun _ => rfl)
          (fun h => by cases h)
        exact ⟨⟨Z', lt.trans hT, hs'⟩, i₁.append i₂⟩
      | allI =>
        obtain ⟨_, _, _, _, _, _, _, hr, _⟩ := (‹PatTy S false _ _ _›).spine rfl
        obtain ⟨_, h₂, -⟩ := hres.tcon hr
        exact (Ty.res_tcon_not_all h₂).elim
      | errVal => exact absurd rfl hk7
    | _ => simp [matchP] at hm
  | adt _ hU ih =>
    intro _ _ _ hvv hv hm ht _
    obtain rfl := ht rfl
    exact ih hvv hv hm (fun h => by cases h)
      (fun _ => ⟨by rw [Ty.kind_of_hd hU]; decide, .inl rfl⟩)
  | rany =>
    intro v _ _ _ _ hm ht _
    obtain rfl := ht rfl
    cases v <;> simp [matchP] at hm <;> exact ⟨⟨_, LeLive.refl, .refl⟩, hm ▸ .nil⟩
  | rfield hR _ _ ih₁ ih₂ =>
    intro v δ _ hvv hv hm ht _
    obtain rfl := ht rfl
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp hm
    obtain ⟨_, i₁⟩ := ih₁ (hvv.field hv hx hR) (field_typ hv hx hR) h1 (fun _ => rfl) (fun h => by cases h)
    obtain ⟨_, i₂⟩ := ih₂ hvv hv h2 (fun _ => rfl) (fun h => by cases h)
    exact ⟨⟨_, LeLive.refl, .refl⟩, i₁.append i₂⟩
  | or _ _ ih₁ ih₂ =>
    intro v δ _ hvv hv hm ht hk
    rcases matchP_or.mp hm with h | ⟨_, h⟩
    · exact ih₁ hvv hv h ht hk
    · exact ih₂ hvv hv h ht hk

/-- Matching at the type of a value. -/
theorem PatTy.inst {Ψ Θ Γ p t Δ v δ} (hp : PatTy S true p t Δ) (hvv : Value v)
    (hv : Typ S Ψ Θ Γ v (.ex t .pure)) (hm : matchP p v = some δ) : Inst S Ψ Θ Γ δ Δ :=
  (hp.typ_match hvv hv hm (fun _ => rfl) (fun h => by cases h)).2

/-- Matching a raised value against a `try` arm. -/
theorem HPat.inst {Ψ Θ Γ ε₀ p θ Δ v δ} (hp : HPat S ε₀ p θ Δ) (hvv : Value v)
    (hv : Typ S Ψ Θ Γ v (.ex (.errors ε₀) .pure)) (hm : matchP p v = some δ) :
    Inst S Ψ Θ Γ δ Δ ∧ Typ S Ψ Θ Γ v (.ex θ .pure) := by
  cases hp with
  | wild => simp [matchP] at hm; subst hm; exact ⟨.nil, hv⟩
  | var => simp [matchP] at hm; subst hm; exact ⟨.cons hvv hv .nil, hv⟩
  | ctor hp =>
    obtain ⟨K, k, ts, T, U, hK, hS, _, hU, hv', _⟩ := canonical_errors hvv hv
    obtain ⟨K', k', ts', T', us, hS', hk', hr, hm'⟩ := hp.spine rfl
    rw [hm' v δ hm] at hK; cases hK
    rw [hS] at hS'; cases hS'
    have hU7 : U.kind ≠ 7 := by rw [Ty.kind_of_hd hU]; decide
    obtain ⟨_, _, ws, hr', hw⟩ := hv'.spine_res hvv (hm' v δ hm) hS rfl hU7 (Ty.not_all_of_hd hU)
    -- an error's constructor builds the error type itself
    obtain ⟨hnil, rfl⟩ := Ty.tcon_eq_adt hr.symm
    have hrs : S.res K = [] := List.map_eq_nil_iff.mp hnil
    rw [hrs] at hw
    obtain rfl : ws = [] := Sig.Vary.nil hw
    obtain ⟨⟨Z, hZ, hs⟩, i⟩ := hp.typ_match hvv hv' hm (fun h => by cases h)
      (fun _ => ⟨hU7, .inl (by rw [hr']; rfl)⟩)
    rw [SpW.adt hs] at hZ
    rw [LeLive.adt_eq hZ] at hv'
    exact ⟨i, hv'⟩

end Flick
