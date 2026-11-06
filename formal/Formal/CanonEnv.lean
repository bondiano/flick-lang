import Formal.CanonForms
import Formal.Semantics

/-!
# Canonical forms of environments, layers and errors
-/

namespace Flick

variable {S : Sig}

theorem canonical_env {Ψ Θ Γ v} (hv : Value v) : ∀ {ρ φ}, Typ S Ψ Θ Γ v (.ex (.env ρ) φ) → IsEnv v := by
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

theorem env_lookup_typ {Ψ Θ Γ N} :
    ∀ {θ v ρ φ}, Typ S Ψ Θ Γ θ (.ex (.env ρ) φ) → θ.envLookup N = some v →
      Typ S Ψ Θ Γ v (.ex (S.impl N) φ) := by
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

theorem env_lookup_some {Ψ Θ Γ N} {θ : Tm} (hθ : IsEnv θ) :
    ∀ {ρ φ}, Typ S Ψ Θ Γ θ (.ex (.env ρ) φ) → ρ (.nm N) → ∃ v, θ.envLookup N = some v := by
  induction hθ with
  | enil => intro _ _ h hN; cases h with | enil hρ => exact (hρ _ hN).elim
  | @econs v θ N' _ _ ih =>
    intro _ _ h hN
    cases h with
    | econs _ _ _ hθ' hρ =>
      by_cases he : N' = N
      · exact ⟨v, by simp [Tm.envLookup, he]⟩
      · rcases (hρ _).mp hN with h | h
        · cases h; exact absurd rfl he
        · obtain ⟨w, hw⟩ := ih hθ' h
          exact ⟨w, by simp [Tm.envLookup, he, hw]⟩

theorem IsEnv.append {θ₁ θ₂ : Tm} (h₁ : IsEnv θ₁) (h₂ : IsEnv θ₂) :
    IsEnv (θ₁.envAppend θ₂) := by
  induction h₁ with
  | enil => exact h₂
  | econs hv _ ih => exact .econs hv ih

theorem env_append_typ {Ψ Θ Γ φ θ₂ ρ₂} (h₂ : Typ S Ψ Θ Γ θ₂ (.ex (.env ρ₂) φ)) (hθ₂ : IsEnv θ₂) :
    ∀ {θ₁ ρ₁ ρ}, IsEnv θ₁ → Typ S Ψ Θ Γ θ₁ (.ex (.env ρ₁) φ) → (∀ n, ρ n ↔ ρ₁ n ∨ ρ₂ n) →
      Typ S Ψ Θ Γ (θ₁.envAppend θ₂) (.ex (.env ρ) φ) := by
  intro θ₁ ρ₁ ρ hθ₁ h₁ hρ
  induction hθ₁ generalizing ρ₁ ρ with
  | enil =>
    cases h₁ with
    | enil hρ₁ =>
      simp only [Tm.envAppend]
      have : ρ = ρ₂ := funext fun n => propext ((hρ n).trans ⟨fun h => h.resolve_left (hρ₁ n), .inr⟩)
      subst this; exact h₂
  | econs _ hθe ih =>
    cases h₁ with
    | econs hv₀ _ hv hθ hρ₁ =>
      simp only [Tm.envAppend]
      refine .econs hv₀ (hθe.append hθ₂) hv (ih hθ (ρ := Row.union _ ρ₂) fun n => Iff.rfl) fun n => ?_
      rw [hρ, hρ₁]; simp only [Row.union]; exact or_assoc

/-- L1 for errors: a value of `Errors ε` is a constructor of an error in `ε`, at a
type whose `⌈·⌉` is within the atoms of `ε`. -/
theorem canonical_errors {Ψ Θ Γ v ε φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.errors ε) φ)) :
    ∃ K k ts T U, v.head = .ctor K ∧ S.ctor K = some (k, ts, T) ∧ ε (.nm T) ∧ U.hd = .adt T ∧
      Typ S Ψ Θ Γ v (.ex U .pure) ∧ Row.Sub (Row.err T U) ε := by
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
  rcases hs K hK with ⟨U, us, hU, hs'⟩ | ⟨_, E, U, he, hE, hUE, hs', hty, hsub⟩
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

theorem IsEnv.lookup_value {θ : Tm} {N : Name} {v : Tm} (hθ : IsEnv θ) (hl : θ.envLookup N = some v) :
    Value v := by
  induction hθ with
  | enil => simp [Tm.envLookup] at hl
  | econs hv _ ih =>
    simp only [Tm.envLookup] at hl
    split at hl
    · cases hl; exact hv
    · exact ih hl

/-- An environment provides no effect: its value would be of `Errors ∅`. -/
theorem env_no_op {Ψ Θ Γ θ ρ φ N} (hθ : IsEnv θ) (h : Typ S Ψ Θ Γ θ (.ex (.env ρ) φ))
    (hl : (θ.envLookup N).isSome) : ∀ o, S.op N o = none := by
  intro o
  cases hop : S.op N o with
  | none => rfl
  | some x =>
    obtain ⟨c, a, b⟩ := x
    obtain ⟨v, hv⟩ := Option.isSome_iff_exists.mp hl
    have ht := env_lookup_typ h hv
    rw [S.op_impl _ _ _ _ _ hop] at ht
    obtain ⟨_, _, _, _, _, _, _, hN, _⟩ := canonical_errors (hθ.lookup_value hv) ht
    exact hN.elim

theorem env_nm {Ψ Θ Γ θ ρ φ} (hθ : IsEnv θ) (h : Typ S Ψ Θ Γ θ (.ex (.env ρ) φ)) {x : Elem} :
    ρ x → ∃ N, x = .nm N := by
  induction hθ generalizing ρ with
  | enil => cases h with | enil hρ => exact fun h => (hρ _ h).elim
  | econs _ _ ih =>
    cases h with
    | econs _ _ _ hθ' hρ =>
      intro ha
      rcases (hρ _).mp ha with h | h
      · exact ⟨_, h⟩
      · exact ih hθ' h

theorem env_no_atm {Ψ Θ Γ θ ρ φ} (hθ : IsEnv θ) (h : Typ S Ψ Θ Γ θ (.ex (.env ρ) φ)) (a : Atom) :
    ¬ ρ (.atm a) := fun ha => by obtain ⟨_, h'⟩ := env_nm hθ h ha; cases h'

end Flick
