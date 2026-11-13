import Formal.ValueTyping

/-!
# Typing views of constructor spines

A conversion at the outer type is absorbed into the result comparison.
Polymorphic and error wrappers are excluded by the observable type former.
-/

namespace Flick

variable {S : Sig}

theorem Typ.ctor_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ K T φ, e = .ctor K → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      ∃ k ts N us, S.ctor K = some (k, ts, N) ∧ us.length = k ∧
        Sig.Holds ts N (S.res K) us ∧ LeLive S (Sig.ctorTyR ts N (S.res K) us) T := by
  induction h with
  | conv _ he ih =>
    intro K T φ hv hJ hA hk; cases hJ
    obtain ⟨k, ts, N, us, hK, hu, hH, hT⟩ := ih K _ _ hv rfl
      (by rw [he.isAll]; exact hA) (by rw [he.kind]; exact hk)
    exact ⟨k, ts, N, us, hK, hu, hH, hT.trans (.equiv he)⟩
  | ctor hK hu hH hT =>
    intro K T φ hv hJ _ _; cases hv; cases hJ
    exact ⟨_, _, _, _, hK, hu, hH, hT⟩
  | allI => intro K T φ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | errVal => intro K T φ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | _ => intro K T φ hv _ _ _; cases hv

theorem Typ.app_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ f x T φ, e = .app f x → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      ∃ a ψ t C, Typ S Ψ Θ Γ f (.ex (.arrow a ψ t C) φ) ∧
        Typ S Ψ Θ Γ x (.ex a φ) ∧ (Stage.use ψ C).le φ ∧ LeLive S t T := by
  induction h with
  | conv _ he ih =>
    intro f x T φ hv hJ hA hk; cases hJ
    obtain ⟨a, ψ, t, C, hf, hx, hφ, ht⟩ := ih f x _ _ hv rfl
      (by rw [he.isAll]; exact hA) (by rw [he.kind]; exact hk)
    exact ⟨a, ψ, t, C, hf, hx, hφ, ht.trans (.equiv he)⟩
  | app hf hx hφ ht =>
    intro f x T φ hv hJ _ _; cases hv; cases hJ
    exact ⟨_, _, _, _, hf, hx, hφ, ht⟩
  | allI => intro f x T φ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | errVal => intro f x T φ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | _ => intro f x T φ hv _ _ _; cases hv

/-- A running application cannot acquire value-only polymorphic or error wrappers. -/
theorem Typ.app_nonvalue {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ f x T φ, e = .app f x → J = .ex T φ → ¬ Value (.app f x) →
      ∃ a ψ t C, Typ S Ψ Θ Γ f (.ex (.arrow a ψ t C) φ) ∧
        Typ S Ψ Θ Γ x (.ex a φ) ∧ (Stage.use ψ C).le φ ∧ LeLive S t T := by
  induction h with
  | conv _ he ih =>
    intro f x T φ hv hJ hn; cases hJ
    obtain ⟨a, ψ, t, C, hf, hx, hφ, ht⟩ := ih f x _ _ hv rfl hn
    exact ⟨a, ψ, t, C, hf, hx, hφ, ht.trans (.equiv he)⟩
  | app hf hx hφ ht =>
    intro f x T φ hv hJ _; cases hv; cases hJ
    exact ⟨_, _, _, _, hf, hx, hφ, ht⟩
  | allI hv | errVal hv => intro f x T φ he _ hn; cases he; exact (hn hv).elim
  | _ => intro f x T φ hv _ _; cases hv

end Flick
