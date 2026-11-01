import Formal.Shapes

/-!
# Canonical forms (Л1)
-/

namespace Flick

variable {S : Sig}

def Tm.args : Tm → Nat
  | .app f _ => f.args + 1
  | _ => 0

/-- The kind of a constant applied to `n` arguments. -/
def Const.kindAt (c : Const) : Nat → Nat
  | 0 => 1
  | 1 => c.resKind
  | _ => 6

/-- The kinds a value's type can have, read from the value's head. -/
def VKind (e : Tm) (k : Nat) : Prop :=
  match e.head with
  | .ctor _ => k = 1 ∨ k = 3 ∨ k = 7
  | .const c => k = c.kindAt e.args
  | .lam _ | .fix _ => k = 1
  | .lit _ => k = 0
  | .loc _ => k = 2
  | .rnil | .rcons .. => k = 4
  | .task _ => k = 5
  | .enil | .econs .. => k = 8
  | _ => True

theorem Const.kindAt_ne3 (c : Const) (n : Nat) : c.kindAt n ≠ 3 := by
  have := Const.resKind_cases c
  cases n with
  | zero => simp [Const.kindAt]
  | succ n => cases n <;> simp [Const.kindAt] <;> omega

theorem Typ.vkind {Ψ Γ e J} (h : Typ S Ψ Γ e J) : Value e → ∀ {T φ}, J = .ex T φ →
    VKind e T.kind := by
  induction h with
  | lam _ hT => intro _ _ _ hJ; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | fix _ hT => intro _ _ _ hJ; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | task _ _ hT => intro _ _ _ hJ; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | lit => intro _ _ _ hJ; cases hJ; rfl
  | loc => intro _ _ _ hJ; cases hJ; rfl
  | rnil => intro _ _ _ hJ; cases hJ; rfl
  | rcons => intro _ _ _ hJ; cases hJ; rfl
  | enil => intro _ _ _ hJ; cases hJ; rfl
  | econs => intro _ _ _ hJ; cases hJ; rfl
  | const _ hT =>
    intro _ _ _ hJ; cases hJ
    simp only [VKind, Tm.head, Tm.args, Const.kindAt]; rw [← hT.kind, Const.ty_kind]
  | ctor _ hT =>
    intro _ _ _ hJ; cases hJ
    have := Ty.arrows_kind ‹List Ty› ‹Name›
    simp only [VKind, Tm.head]; rw [← hT.kind]; omega
  | errVal hv' _ _ ih =>
    intro _ _ _ hJ; cases hJ
    have ih' := ih hv' rfl
    revert ih'
    simp only [VKind, Ty.kind]
    split
    · intro; simp
    · intro h; exact absurd h.symm (Const.kindAt_ne3 _ _)
    all_goals simp
  | app hf ha hψ hT _ _ =>
    intro hv _ _ hJ; cases hJ
    have hs := (Typ.app hf ha hψ hT).shape hv rfl
    cases hv with
    | ctorApp _ hK _ =>
      rcases hs.2 _ hK with ⟨U, hU, hs'⟩ | ⟨_, _, h', -⟩
      · obtain ⟨_, _, _, ts', _, _, rfl⟩ := hs'.false_arrows
        have := Ty.arrows_kind ts' ‹Name›
        simp only [VKind, Tm.head, hK]; rw [← hU.kind]; omega
      · rw [h']; simp [VKind, Tm.head, hK, Ty.kind]
    | constApp hc _ =>
      simp [VKind, Tm.head, Tm.args, Const.kindAt, (Typ.app hf ha hψ hT).constApp_kind hc]
    | constApp2 h₁ h₂ _ _ =>
      simp [VKind, Tm.head, Tm.args, Const.kindAt, (Typ.app hf ha hψ hT).constApp2_kind h₁ h₂]
  | _ => intro hv _ _ hJ; cases hJ <;> cases hv

theorem Const.arity_of_resKind {c : Const} (h₁ : c.resKind = 1) (h₂ : c.arity = 2 ∨ c.layer) :
    c.arity = 2 := by
  cases c <;> simp [Const.resKind, Const.arity, Const.layer] at h₁ h₂ ⊢

/-- Л1 for functions. -/
theorem canonical_arrow {Ψ Γ v a ψ t φ} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.arrow a ψ t) φ)) :
    (∃ b, v = .lam b) ∨ (∃ f, v = .fix f) ∨ (∃ K, v.head = .ctor K) ∨ (∃ c, v = .const c) ∨
      (∃ c w, v = .app (.const c) w ∧ c.arity = 2 ∧ Value w) := by
  have hk := h.vkind hv rfl
  cases hv with
  | lam => exact .inl ⟨_, rfl⟩
  | fix => exact .inr (.inl ⟨_, rfl⟩)
  | ctor => exact .inr (.inr (.inl ⟨_, rfl⟩))
  | ctorApp _ hf _ => exact .inr (.inr (.inl ⟨_, hf⟩))
  | const => exact .inr (.inr (.inr (.inl ⟨_, rfl⟩)))
  | constApp hc hw =>
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
    exact .inr (.inr (.inr (.inr ⟨_, _, rfl, Const.arity_of_resKind hk.symm hc, hw⟩)))
  | _ => simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk

/-- Л1 for references. -/
theorem canonical_ref {Ψ Γ v t φ} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.ref t) φ)) :
    ∃ l, v = .loc l ∧ Ψ[l]? = some t := by
  have hk := h.vkind hv rfl
  cases hv with
  | loc => cases h with | loc hl => exact ⟨_, rfl, hl⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
    have := Const.resKind_cases ‹Const›; omega
  | _ => simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk

/-- Л1 for computations. -/
theorem canonical_eff {Ψ Γ v r ε t φ} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.eff r ε t) φ)) :
    ∃ e t₀ ψ, v = .task e ∧ Typ S Ψ Γ e (.ex t₀ ψ) ∧ ψ.le (.rt r ε) ∧ Le t₀ t := by
  have hk := h.vkind hv rfl
  cases hv with
  | task =>
    cases h with
    | task he hψ hT =>
      obtain ⟨_, _, _, h, hr, he', ht⟩ := hT.eff_left
      cases h
      exact ⟨_, _, _, rfl, he, Stage.le_trans hψ ⟨hr, he'⟩, ht⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
    have := Const.resKind_cases ‹Const›; omega
  | _ => simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk

theorem canonical_unit {Ψ Γ v φ} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.base .unit) φ)) :
    v = .lit .unit := by
  have hs := (h.shape hv rfl).1
  generalize hT : Ty.base Base.unit = T at hs
  cases hs with
  | hidden h => subst hT; cases h
  | @lit l => cases l <;> simp [Lit.base] at hT; rfl
  | _ => cases hT

/-- Л1 for records: field lookup and update succeed. -/
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
  | hidden ho => cases ‹Ty› <;> simp_all [Ty.field, Ty.Opaque]
  | lit => simp [Ty.field] at hR
  | rnil => simp [Ty.field] at hR
  | adt => simp [Ty.field] at hR

end Flick
