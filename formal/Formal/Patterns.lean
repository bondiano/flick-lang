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
    | errVal => simp [Ty.field] at hR
  | drec =>
    intro _ _ _ h _ hR
    cases h with
    | drec => simp [Ty.field] at hR
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
      | errVal => simp [Ty.field] at hR
  | drec =>
    intro _ _ _ _ h _ hR
    cases h with
    | drec => simp [Ty.field] at hR
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

/-- Subtyping does not change a final result that is not a computation. -/
theorem Le.res_eq {a b : Ty} (h : Le a b) (hb : ∀ r e t C, b.res ≠ .eff r e t C) :
    a.res = b.res := by
  induction h with
  | refl => rfl
  | arrow _ _ _ _ _ ih => exact ih hb
  | eff => exact (hb _ _ _ _ rfl).elim

theorem Ty.tcon_res (T : Name) (us : List Ty) : (Ty.tcon T us).res = Ty.tcon T us := by
  have h := Ty.tcon_not_arrow T us
  generalize Ty.tcon T us = t at h
  cases t with
  | arrow => exact absurd rfl (h _ _ _ _)
  | _ => rfl

theorem Ty.tcon_not_eff (T : Name) (us : List Ty) : ∀ r e t C, Ty.tcon T us ≠ .eff r e t C := by
  intro r e t C h
  have := Ty.tcon_kind T us; rw [h] at this; cases this

theorem Le.res_tcon {a b : Ty} {T : Name} {us : List Ty} (h : Le a b) (ha : a.res = Ty.tcon T us) :
    b.res = Ty.tcon T us := by
  induction h with
  | refl => exact ha
  | arrow _ _ _ _ _ ih => exact ih ha
  | eff => exact absurd ha.symm (Ty.tcon_not_eff _ _ _ _ _ _)

theorem Le.res_tcon' {a b : Ty} {T : Name} {us : List Ty} (h : Le a b) (hb : b.res = Ty.tcon T us) :
    a.res = Ty.tcon T us := by
  induction h with
  | refl => exact hb
  | arrow _ _ _ _ _ ih => exact ih hb
  | eff => exact absurd hb.symm (Ty.tcon_not_eff _ _ _ _ _ _)

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

/-- The final type of a typed constructor spine is its type at an instance. -/
theorem Typ.spine_res {Ψ Θ Γ v J} (h : Typ S Ψ Θ Γ v J) :
    Value v → ∀ {K k ts T X φ}, v.head = .ctor K → S.ctor K = some (k, ts, T) →
      J = .ex X φ → X.kind ≠ 7 →
        ∃ us, us.length = k ∧ X.res = Ty.tcon T ((S.res K).map (Ty.substT (instT us))) := by
  induction h with
  | ctor hK hk _ hT =>
    intro _ _ _ _ _ _ _ hK' hS hJ _
    cases hJ; simp only [Tm.head] at hK'; cases hK'
    rw [hK] at hS; cases hS
    exact ⟨_, hk, hT.res_tcon (Sig.ctorTyR_res _ _ _ _)⟩
  | app hf _ _ hT ih₁ _ =>
    intro hv _ _ _ _ _ _ hK hS hJ hk
    cases hJ
    cases hv with
    | ctorApp hvf hK' _ =>
      simp only [Tm.head] at hK
      obtain ⟨us, hl, hr⟩ := ih₁ hvf hK hS rfl (by simp [Ty.kind])
      simp only [Ty.res] at hr
      exact ⟨us, hl, hT.res_tcon hr⟩
    | constApp => simp [Tm.head] at hK
  | errVal => intro _ _ _ _ _ _ _ _ _ hJ hk; cases hJ; simp [Ty.kind] at hk
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
      (fl = true → t' = t) → (fl = false → t'.kind ≠ 7 ∧ t'.res = t.res) →
      Le t t' ∧ Inst S Ψ Θ Γ δ Δ := by
  induction hp with
  | wild =>
    intro _ _ _ _ _ hm ht _; simp [matchP] at hm; subst hm; obtain rfl := ht rfl
    exact ⟨.refl, .nil⟩
  | var =>
    intro _ _ _ hvv hv hm ht _; simp [matchP] at hm; subst hm
    obtain rfl := ht rfl; exact ⟨.refl, .cons hvv hv .nil⟩
  | lit =>
    intro v _ _ _ _ hm ht _
    obtain rfl := ht rfl
    cases v <;> simp [matchP] at hm
    exact ⟨.refl, hm.2 ▸ .nil⟩
  | head hK hj hk =>
    intro v _ _ _ hv hm _ hk'
    cases v <;> simp [matchP] at hm
    obtain ⟨rfl, rfl⟩ := hm
    obtain ⟨hk7, hres⟩ := hk' rfl
    cases hv with
    | ctor hK' hk'' _ hT =>
      rw [hK] at hK'; cases hK'
      have := hT.res_tcon (Sig.ctorTyR_res _ _ _ _)
      rw [hres, Sig.ctorTyR_res] at this
      -- the instance of what it builds is the instance of the constructor
      cases Sig.inst_inj hj hk'' hk (Ty.tcon_inj (by simp) this.symm)
      exact ⟨hT, .nil⟩
    | errVal => exact absurd rfl hk7
  | app _ _ ih₁ ih₂ =>
    intro v δ _ hvv hv hm _ hk
    obtain ⟨hk7, hres⟩ := hk rfl
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp hm
      cases hv with
      | app hf he hψ hT =>
        obtain rfl := Stage.eq_pure hψ
        have hvf : Value f ∧ Value e := by
          cases hvv with
          | ctorApp hf' _ he' => exact ⟨hf', he'⟩
          | constApp _ he' => exact ⟨.const, he'⟩
        obtain ⟨_, _, _, _, _, _, _, hr, _⟩ := (‹PatTy S false _ _ _›).spine rfl
        simp only [Ty.res] at hr
        have hres' := hT.res_tcon' (by rw [hres]; exact hr)
        obtain ⟨hl, i₁⟩ := ih₁ hvf.1 hf h1 (fun h => by cases h)
          (fun _ => ⟨by simp [Ty.kind], by simp only [Ty.res]; rw [hres', hr]⟩)
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hl.arrow_left
        cases h
        obtain ⟨_, i₂⟩ := ih₂ hvf.2 (he.sub ⟨la, Stage.le_refl _⟩) h2 (fun _ => rfl)
          (fun h => by cases h)
        exact ⟨lt.trans hT, i₁.append i₂⟩
      | errVal => exact absurd rfl hk7
    | _ => simp [matchP] at hm
  | adt _ hU ih =>
    intro _ _ _ hvv hv hm ht _
    obtain rfl := ht rfl
    exact ih hvv hv hm (fun h => by cases h)
      (fun _ => ⟨by rw [Ty.kind_of_hd hU]; decide, rfl⟩)
  | rany =>
    intro v _ _ _ _ hm ht _
    obtain rfl := ht rfl
    cases v <;> simp [matchP] at hm <;> exact ⟨.refl, hm ▸ .nil⟩
  | rfield hR _ _ ih₁ ih₂ =>
    intro v δ _ hvv hv hm ht _
    obtain rfl := ht rfl
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp hm
    obtain ⟨_, i₁⟩ := ih₁ (hvv.field hv hx hR) (field_typ hv hx hR) h1 (fun _ => rfl) (fun h => by cases h)
    obtain ⟨_, i₂⟩ := ih₂ hvv hv h2 (fun _ => rfl) (fun h => by cases h)
    exact ⟨.refl, i₁.append i₂⟩
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
    obtain ⟨_, _, hr'⟩ := hv'.spine_res hvv (hm' v δ hm) hS rfl hU7
    -- an error's constructor builds the error type itself
    obtain ⟨hnil, rfl⟩ := Ty.tcon_eq_adt hr.symm
    have hrs : S.res K = [] := List.map_eq_nil_iff.mp hnil
    obtain ⟨hl, i⟩ := hp.typ_match hvv hv' hm (fun h => by cases h)
      (fun _ => ⟨hU7, by rw [hr', hrs]; rfl⟩)
    cases hl; exact ⟨i, hv'⟩

end Flick
