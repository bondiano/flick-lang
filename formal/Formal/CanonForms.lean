import Formal.Shapes

/-!
# Canonical forms (L1)
-/

namespace Flick

variable {S : Sig}

def Tm.args : Tm → Nat
  | .app f _ => f.args + 1
  | _ => 0

/-- The kinds a value's type can have, read from the value's head. -/
def VKind (e : Tm) (k : Nat) : Prop :=
  match e.head with
  | .ctor _ => k = 1 ∨ k = 3 ∨ k = 7
  | .const _ | .lam _ | .fix _ | .kont _ _ => k = 1
  | .lit _ => k = 0
  | .loc _ => k = 2
  | .rnil | .rcons .. => k = 4
  | .task _ => k = 5
  | .cap _ => k = 10
  | .own _ => k = 11
  | .drec .. => k = 13
  | _ => True

theorem Typ.vkind {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : Value e → ∀ {T φ}, J = .ex T φ →
    ¬ T.isAll → VKind e T.kind := by
  induction h with
  | allI => intro _ _ _ hJ h; cases hJ; exact (h trivial).elim
  | lam _ _ _ hT => intro _ _ _ hJ _; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | fix _ _ _ hT => intro _ _ _ hJ _; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | task _ _ _ _ hT => intro _ _ _ hJ _; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | cap => intro _ _ _ hJ _; cases hJ; rfl
  | own => intro _ _ _ hJ _; cases hJ; rfl
  | lit => intro _ _ _ hJ _; cases hJ; rfl
  | loc => intro _ _ _ hJ _; cases hJ; rfl
  | rnil => intro _ _ _ hJ _; cases hJ; rfl
  | rcons => intro _ _ _ hJ _; cases hJ; rfl
  | drec => intro _ _ _ hJ _; cases hJ; rfl
  | const _ hT =>
    intro _ _ _ hJ _; cases hJ
    simp only [VKind, Tm.head]; rw [← hT.kind, Const.ty_kind]
  | perform _ hT =>
    intro _ _ _ hJ _; cases hJ
    simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | kont _ _ hT => intro _ _ _ hJ _; cases hJ; simp only [VKind, Tm.head]; rw [← hT.kind]; rfl
  | ctor _ _ _ hT =>
    intro _ _ _ hJ _; cases hJ
    simp only [VKind, Tm.head]; rw [← hT.kind]; unfold Sig.ctorTyR
    rcases Ty.arrows_kind _ _ _ with h | h <;> rw [h] <;> simp
  | errVal hv' _ hU _ ih =>
    intro _ _ _ hJ _; cases hJ
    have ih' := ih hv' rfl (Ty.not_all_of_hd hU)
    rw [Ty.kind_of_hd hU] at ih'
    revert ih'
    simp only [VKind, Ty.kind]
    split
    · intro; simp
    · intro h; cases h
    all_goals simp
  | app hf ha hψ hT _ _ =>
    intro hv _ _ hJ hA; cases hJ
    have hs := (Typ.app hf ha hψ hT).shape hv rfl
    cases hv with
    | ctorApp _ hK _ =>
      rcases hs.2 _ hK hA with ⟨U, us, hU, hs'⟩ | ⟨_, _, _, h', -⟩
      · obtain ⟨K', _, _, T₀, ts', _, _, rfl⟩ := hs'.false_arrows
        have := Ty.arrows_kind (ts'.map (Ty.substT (instT us))) T₀
          ((S.res K').map (Ty.substT (instT us)))
        simp only [Ty.arrows_substT, Ty.tcon_substT] at hU
        simp only [VKind, Tm.head, hK]; rw [← hU.kind]; omega
      · rw [h']; simp [VKind, Tm.head, hK, Ty.kind]
    | constApp hc _ =>
      simp [VKind, Tm.head, (Typ.app hf ha hψ hT).constApp_kind hc hA]
  | _ => intro hv _ _ hJ _; cases hJ <;> cases hv

/-- L1 for functions. -/
theorem canonical_arrow {Ψ Θ Γ v a ψ t C φ} (hv : Value v)
    (h : Typ S Ψ Θ Γ v (.ex (.arrow a ψ t C) φ)) :
    (∃ b, v = .lam b) ∨ (∃ f, v = .fix f) ∨ (∃ K, v.head = .ctor K) ∨ (∃ c, v = .const c) ∨
      (∃ c w, v = .app (.const c) w ∧ c.arity = 2 ∧ Value w) ∨ ∃ p ℓ, v = .kont p ℓ := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll])
  cases hv with
  | lam => exact .inl ⟨_, rfl⟩
  | fix => exact .inr (.inl ⟨_, rfl⟩)
  | ctor => exact .inr (.inr (.inl ⟨_, rfl⟩))
  | ctorApp _ hf _ => exact .inr (.inr (.inl ⟨_, hf⟩))
  | const => exact .inr (.inr (.inr (.inl ⟨_, rfl⟩)))
  | constApp hc hw =>
    simp [VKind, Tm.head, Ty.kind] at hk
    exact .inr (.inr (.inr (.inr (.inl ⟨_, _, rfl, hc, hw⟩))))
  | kont => exact .inr (.inr (.inr (.inr (.inr ⟨_, _, rfl⟩))))
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

/-- L1 for references. -/
theorem canonical_ref {Ψ Θ Γ v t φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.ref t) φ)) :
    ∃ l, v = .loc l ∧ Ψ[l]? = some t := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll])
  cases hv with
  | loc => cases h with | loc hl => exact ⟨_, rfl, hl⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Ty.kind] at hk
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

/-- L1 for computations. -/
theorem canonical_eff {Ψ Θ Γ v r ε t C φ} (hv : Value v)
    (h : Typ S Ψ Θ Γ v (.ex (.eff r ε t C) φ)) :
    ∃ e t₀ ψ, v = .task e ∧ Typ S Ψ Θ Γ e (.ex t₀ ψ) ∧ ψ.le (.rt r ε) ∧ Le t₀ t := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll])
  cases hv with
  | task =>
    cases h with
    | task he hψ _ _ hT =>
      obtain ⟨_, _, _, _, h, hr, he', ht, -⟩ := hT.eff_left
      cases h
      exact ⟨_, _, _, rfl, he, Stage.le_trans hψ ⟨hr, he'⟩, ht⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Ty.kind] at hk
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

theorem canonical_unit {Ψ Θ Γ v φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.base .unit) φ)) :
    v = .lit .unit := by
  have hs := (h.shape hv rfl).1
  generalize hT : Ty.base Base.unit = T at hs
  cases hs with
  | hidden h => subst hT; cases h
  | @lit l => cases l <;> simp [Lit.base] at hT; rfl
  | adt _ _ h => subst hT; simp [Ty.hd] at h
  | _ => cases hT

theorem canonical_int {Ψ Θ Γ v φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.base .int) φ)) :
    ∃ n, v = .lit (.int n) := by
  have hs := (h.shape hv rfl).1
  generalize hT : Ty.base Base.int = T at hs
  cases hs with
  | hidden h => subst hT; cases h
  | @lit l => cases l <;> simp [Lit.base] at hT; exact ⟨_, rfl⟩
  | adt _ _ h => subst hT; simp [Ty.hd] at h
  | _ => cases hT

/-- L1 for owners. -/
theorem canonical_owner {Ψ Θ Γ v s φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.owner s) φ)) :
    ∃ p, v = .own p ∧ s = .rg p := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll])
  cases hv with
  | own => cases h with | own => exact ⟨_, rfl, rfl⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Ty.kind] at hk
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

/-- L1 for capabilities. -/
theorem canonical_hnd {Ψ Θ Γ v s φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.hnd s) φ)) :
    ∃ p, v = .cap p ∧ s = .rg p := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll])
  cases hv with
  | cap => cases h with | cap => exact ⟨_, rfl, rfl⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Ty.kind] at hk
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

/-- L1 for records: field lookup and update succeed. -/
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
  | hidden ho =>
    cases ‹Ty› <;> first | (simp [Ty.Opaque, Ty.hd] at ho; done) | (simp [Ty.field] at hR; done)
  | lit => simp [Ty.field] at hR
  | rnil => simp [Ty.field] at hR
  | adt _ _ h => cases ‹Ty› <;> first | (simp [Ty.hd] at h; done) | (simp [Ty.field] at hR; done)

/-- L1 for errors: a value of `Errors ε` is a constructor of an error in `ε`, at a
type whose `⌈·⌉` is within the atoms of `ε`. -/
theorem canonical_errors {Ψ Θ Γ v ε φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.errors ε) φ)) :
    ∃ K k ts T U, v.head = .ctor K ∧ S.ctor K = some (k, ts, T) ∧ ε (.nm T) ∧ U.hd = .adt T ∧
      Typ S Ψ Θ Γ v (.ex U .pure) ∧ Row.Sub (Row.err T U) ε := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll])
  have hs := (h.shape hv rfl).2
  obtain ⟨K, hK⟩ : ∃ K, v.head = .ctor K := by
    cases hv with
    | ctor => exact ⟨_, rfl⟩
    | ctorApp _ hf _ => exact ⟨_, hf⟩
    | constApp =>
      simp [VKind, Tm.head, Ty.kind] at hk
    | _ => simp [VKind, Tm.head, Ty.kind] at hk
  rcases hs K hK (by simp [Ty.isAll]) with ⟨U, us, hU, hs'⟩ | ⟨_, E, U, he, hE, hUE, hs', hty, hsub⟩
  · obtain ⟨K', k, _, T₀, ts', _, _, rfl⟩ := hs'.false_arrows
    simp only [Ty.arrows_substT, Ty.tcon_substT] at hU
    have h₁ := Ty.arrows_kind (ts'.map (Ty.substT (instT us))) T₀ ((S.res K').map (Ty.substT (instT us)))
    rw [hU.kind] at h₁; simp [Ty.kind] at h₁
  · cases he
    cases hs' with
    | hidden ho => simp [Ty.Opaque, hUE] at ho
    | adt hsp hU₁ hU₂ =>
      rw [hUE] at hU₂; cases hU₂
      obtain ⟨K', k, ts, T, ts', hK', hS, rfl⟩ := hsp.false_arrows
      rw [hK] at hK'; cases hK'
      cases ts' with
      | nil =>
        simp only [Ty.arrows, Ty.tcon_hd] at hU₁; cases hU₁
        exact ⟨K, k, ts, _, U, hK, hS, hE, hUE, hty.valueAny hv _, hsub⟩
      | cons => simp [Ty.arrows, Ty.hd] at hU₁
    | lit | rnil | rcons => simp [Ty.hd] at hUE

end Flick
