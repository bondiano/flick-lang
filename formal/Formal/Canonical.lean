import Formal.Substitution

/-!
# Canonical forms (Л1) and value shapes
-/

namespace Flick

variable {S : Sig}

/-- A constructor spine has type `τ₁ → … → τₙ → T`. -/
theorem spine_ty {Ψ Γ K} :
    ∀ {v t}, v.head = .ctor K → Typ S Ψ Γ v (.ex t) → ∃ ts T, t = .arrows ts (.adt T)
  | .ctor _, _, _, h => by cases h; exact ⟨_, _, rfl⟩
  | .app f _, _, hh, h => by
    cases h with
    | app hf _ =>
      obtain ⟨ts, T, he⟩ := spine_ty (v := f) hh hf
      cases ts with
      | nil => cases he
      | cons a ts => cases he; exact ⟨ts, T, rfl⟩
  | .var _, _, hh, _ | .lam _, _, hh, _ | .let_ _ _, _, hh, _ | .letrec _ _, _, hh, _
  | .fix _, _, hh, _ | .lit _, _, hh, _ | .loc _, _, hh, _ | .const _, _, hh, _
  | .rnil, _, hh, _ | .rcons _ _ _, _, hh, _ | .proj _ _, _, hh, _ | .update _ _ _, _, hh, _
  | .match_ _ _, _, hh, _ | .anil, _, hh, _ | .acons _ _ _, _, hh, _
  | .aguard _ _ _ _, _, hh, _ | .todo, _, hh, _ => by simp [Tm.head] at hh

theorem Shp.full {t v} (h : Shp S false t v) (ht : ∃ ts T, t = .arrows ts (.adt T)) :
    Shp S true t v := by
  obtain ⟨ts, T, rfl⟩ := ht
  cases ts with
  | nil => exact .adt h
  | cons _ _ => exact .arrow

theorem Typ.const_inv {Ψ Γ c t} (h : Typ S Ψ Γ (.const c) (.ex t)) : t = c.ty := by
  cases h; rfl

theorem Const.shape {c : Const} {v} : Shp S true c.ty v := by
  cases c <;> exact .arrow

theorem Const.ty_ne_ref {c : Const} {t} : c.ty ≠ .ref t := by
  cases c <;> simp [Const.ty]

/-- A binary constant applied once is still a function. -/
theorem Const.ty_two {c : Const} {a t} (hc : c.arity = 2) (h : c.ty = .arrow a t) :
    ∃ x y, t = .arrow x y := by
  cases c <;> simp [Const.arity, Const.ty] at hc h
  exact ⟨_, _, h.2.symm⟩

/-- Every closed value has a shape of its type; a constructor spine also has
a partial shape. -/
theorem Value.shape {Ψ Γ v} (hv : Value v) :
    ∀ {t}, Typ S Ψ Γ v (.ex t) →
      Shp S true t v ∧ (∀ K, v.head = .ctor K → Shp S false t v) := by
  induction hv with
  | lam => intro _ h; cases h; exact ⟨.arrow, by simp [Tm.head]⟩
  | fix => intro _ h; cases h; exact ⟨.arrow, by simp [Tm.head]⟩
  | lit => intro _ h; cases h; exact ⟨.lit, by simp [Tm.head]⟩
  | loc => intro _ h; cases h; exact ⟨.ref, by simp [Tm.head]⟩
  | const =>
    intro _ h; obtain rfl := h.const_inv
    exact ⟨Const.shape, by simp [Tm.head]⟩
  | ctor =>
    intro _ h
    cases h with
    | ctor hK => exact ⟨Shp.full (.head hK) ⟨_, _, rfl⟩, fun _ _ => .head hK⟩
  | rnil => intro _ h; cases h; exact ⟨.rnil, by simp [Tm.head]⟩
  | rcons _ _ ih₁ ih₂ =>
    intro _ h
    cases h with
    | rcons _ h₁ h₂ => exact ⟨.rcons (ih₁ h₁).1 (ih₂ h₂).1, by simp [Tm.head]⟩
  | ctorApp _ hf _ ih₁ ih₂ =>
    intro t h
    cases h with
    | app h₁ h₂ =>
      have hsp : Shp S false t _ := .app ((ih₁ h₁).2 _ hf) (ih₂ h₂).1
      exact ⟨hsp.full (spine_ty (by simpa [Tm.head] using hf) (.app h₁ h₂)), fun _ _ => hsp⟩
  | constApp hc _ _ =>
    intro t h
    cases h with
    | app h₁ _ =>
      obtain ⟨x, y, rfl⟩ := Const.ty_two hc h₁.const_inv.symm
      exact ⟨.arrow, by simp [Tm.head]⟩

/-- Л1 for functions. -/
theorem canonical_arrow {Ψ Γ v a t} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.arrow a t))) :
    (∃ b, v = .lam b) ∨ (∃ f, v = .fix f) ∨ (∃ K, v.head = .ctor K) ∨ (∃ c, v = .const c) ∨
      (∃ c w, v = .app (.const c) w ∧ c.arity = 2 ∧ Value w) := by
  cases hv with
  | lam => exact .inl ⟨_, rfl⟩
  | fix => exact .inr (.inl ⟨_, rfl⟩)
  | ctor => exact .inr (.inr (.inl ⟨_, rfl⟩))
  | ctorApp _ hf _ => exact .inr (.inr (.inl ⟨_, hf⟩))
  | const => exact .inr (.inr (.inr (.inl ⟨_, rfl⟩)))
  | constApp hc hw => exact .inr (.inr (.inr (.inr ⟨_, _, rfl, hc, hw⟩)))
  | lit => cases h
  | loc => cases h
  | rnil => cases h
  | rcons => cases h

/-- Л1 for references. -/
theorem canonical_ref {Ψ Γ v t} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.ref t))) :
    ∃ l, v = .loc l ∧ Ψ[l]? = some t := by
  cases hv with
  | loc => cases h with | loc hl => exact ⟨_, rfl, hl⟩
  | ctor => obtain ⟨ts, _, he⟩ := spine_ty rfl h; cases ts <;> cases he
  | ctorApp _ hf _ =>
    obtain ⟨ts, _, he⟩ := spine_ty (v := .app _ _) hf h; cases ts <;> cases he
  | const => exact absurd h.const_inv.symm Const.ty_ne_ref
  | constApp hc _ =>
    cases h with
    | app h₁ _ =>
      obtain ⟨_, _, he⟩ := Const.ty_two hc h₁.const_inv.symm
      cases he
  | lam => cases h
  | fix => cases h
  | lit => cases h
  | rnil => cases h
  | rcons => cases h

end Flick
