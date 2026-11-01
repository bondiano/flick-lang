import Formal.CanonForms
import Formal.Semantics

/-!
# Canonical forms of environments, layers and errors
-/

namespace Flick

variable {S : Sig}

theorem canonical_env {Ψ Γ v} (hv : Value v) : ∀ {ρ φ}, Typ S Ψ Γ v (.ex (.env ρ) φ) → IsEnv v := by
  induction hv with
  | enil => intros; exact .enil
  | econs hv₁ _ _ ih => intro _ _ h; cases h with | econs _ _ _ hθ _ => exact .econs hv₁ (ih hθ)
  | ctorApp _ hf _ =>
    intro _ _ h; have hk := h.vkind (.ctorApp ‹_› hf ‹_›) rfl
    simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp hc hw =>
    intro _ _ h; have hk := h.vkind (.constApp hc hw) rfl
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
    have := Const.resKind_cases ‹Const›; omega
  | constApp2 h₁ h₂ hw₁ hw₂ =>
    intro _ _ h; have hk := h.vkind (.constApp2 h₁ h₂ hw₁ hw₂) rfl
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
  | _ =>
    intro _ _ h; have hk := h.vkind (by constructor <;> assumption) rfl
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk

theorem env_lookup_typ {Ψ Γ N} :
    ∀ {θ v ρ φ}, Typ S Ψ Γ θ (.ex (.env ρ) φ) → θ.envLookup N = some v →
      Typ S Ψ Γ v (.ex (S.impl N) φ) := by
  intro θ
  induction θ with
  | econs N' w θ _ ih =>
    intro v _ _ h hl
    cases h with
    | econs _ _ hw hθ _ =>
      simp only [Tm.envLookup] at hl
      split at hl
      · next he => subst he; cases hl; exact hw
      · exact ih hθ hl
  | _ => intro _ _ _ _ hl; simp [Tm.envLookup] at hl

theorem env_lookup_some {Ψ Γ N} {θ : Tm} (hθ : IsEnv θ) :
    ∀ {ρ φ}, Typ S Ψ Γ θ (.ex (.env ρ) φ) → ρ N → ∃ v, θ.envLookup N = some v := by
  induction hθ with
  | enil => intro _ _ h hN; cases h with | enil hρ => exact (hρ _ hN).elim
  | @econs v θ N' _ _ ih =>
    intro _ _ h hN
    cases h with
    | econs _ _ _ hθ' hρ =>
      by_cases he : N' = N
      · exact ⟨v, by simp [Tm.envLookup, he]⟩
      · rcases hρ _ hN with h | h
        · exact absurd h.symm he
        · obtain ⟨w, hw⟩ := ih hθ' h
          exact ⟨w, by simp [Tm.envLookup, he, hw]⟩

theorem IsEnv.append {θ₁ θ₂ : Tm} (h₁ : IsEnv θ₁) (h₂ : IsEnv θ₂) :
    IsEnv (θ₁.envAppend θ₂) := by
  induction h₁ with
  | enil => exact h₂
  | econs hv _ ih => exact .econs hv ih

theorem env_append_typ {Ψ Γ φ θ₂ ρ₂} (h₂ : Typ S Ψ Γ θ₂ (.ex (.env ρ₂) φ)) (hθ₂ : IsEnv θ₂) :
    ∀ {θ₁ ρ₁ ρ}, IsEnv θ₁ → Typ S Ψ Γ θ₁ (.ex (.env ρ₁) φ) → (∀ n, ρ n → ρ₁ n ∨ ρ₂ n) →
      Typ S Ψ Γ (θ₁.envAppend θ₂) (.ex (.env ρ) φ) := by
  intro θ₁ ρ₁ ρ hθ₁ h₁ hρ
  induction hθ₁ generalizing ρ₁ ρ with
  | enil =>
    cases h₁ with
    | enil hρ₁ =>
      simp only [Tm.envAppend]
      cases hθ₂ with
      | enil => cases h₂ with
        | enil hρ₂ => exact .enil fun n h => by rcases hρ n h with h | h <;> simp_all
      | econs => cases h₂ with
        | econs hv₀ hθ₀ hv hθ hρ' =>
          exact .econs hv₀ hθ₀ hv hθ fun n h => by
            rcases hρ n h with h | h
            · exact (hρ₁ _ h).elim
            · exact hρ' n h
  | econs _ hθe ih =>
    cases h₁ with
    | econs hv₀ _ hv hθ hρ₁ =>
      simp only [Tm.envAppend]
      refine .econs hv₀ (hθe.append hθ₂) hv (ih hθ (ρ := Row.union _ ρ₂) fun n h => h) fun n h => ?_
      rcases hρ n h with h | h
      · rcases hρ₁ n h with h | h
        · exact .inl h
        · exact .inr (.inl h)
      · exact .inr (.inr h)

/-- Л1 for errors: a value of `Errors ε` is a constructor of an error in `ε`. -/
theorem canonical_errors {Ψ Γ v ε φ} (hv : Value v) (h : Typ S Ψ Γ v (.ex (.errors ε) φ)) :
    ∃ K ts T, v.head = .ctor K ∧ S.ctor K = some (ts, T) ∧ ε T ∧
      Typ S Ψ Γ v (.ex (.adt T) .pure) := by
  have hk := h.vkind hv rfl
  have hs := (h.shape hv rfl).2
  obtain ⟨K, hK⟩ : ∃ K, v.head = .ctor K := by
    cases hv with
    | ctor => exact ⟨_, rfl⟩
    | ctorApp _ hf _ => exact ⟨_, hf⟩
    | constApp =>
      simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
      have := Const.resKind_cases ‹Const›; omega
    | _ => simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk
  rcases hs K hK with ⟨U, hU, hs'⟩ | ⟨_, E, he, hE, hs', hty⟩
  · obtain ⟨_, _, T₀, ts', _, _, rfl⟩ := hs'.false_arrows
    have h₁ := Ty.arrows_kind ts' T₀
    rw [hU.kind] at h₁; simp [Ty.kind] at h₁
  · cases he
    obtain ⟨K', ts, T, ts', hK', hS, hU⟩ := hs'.false_arrows
    rw [hK] at hK'; cases hK'
    cases ts' with
    | nil => cases hU; exact ⟨K, ts, E, hK, hS, hE, hty.valueAny hv _⟩
    | cons => cases hU

end Flick
