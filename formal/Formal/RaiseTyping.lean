import Formal.HandlerTyping

/-!
# The stage and error value of `raise`

The value result type is arbitrary, including after conversion. Its stage
still contains the error row of the raised value.
-/

namespace Flick

variable {S : Sig}

theorem Typ.raise_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ w T φ, e = .raise w → J = .ex T φ → ∃ ε, (Stage.rt Row.none ε).le φ := by
  induction h with
  | conv _ _ ih => intro w T φ hv hJ; cases hJ; exact ih w _ _ hv rfl
  | raise _ hφ | raiseAdt _ _ hφ => intro w T φ hv hJ; cases hv; cases hJ; exact ⟨_, hφ⟩
  | allI hv | errVal hv => intro w T φ he _; cases he; cases hv
  | _ => intro w T φ hv _; cases hv

theorem Typ.raise_value {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ w T φ, e = .raise w → J = .ex T φ → Value w →
      ∃ ε, Typ S Ψ Θ Γ w (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε).le φ := by
  induction h with
  | conv _ _ ih => intro w T φ hv hJ hw; cases hJ; exact ih w _ _ hv rfl hw
  | raise hb hφ => intro w T φ hv hJ hw; cases hv; cases hJ; exact ⟨_, hb.valueAny hw _, hφ⟩
  | raiseAdt hb hU hφ =>
    intro w T φ hv hJ hw; cases hv; cases hJ
    exact ⟨_, .errVal hw (hb.valueAny hw _) hU (Row.Sub.refl _), hφ⟩
  | allI hv | errVal hv => intro w T φ he _ _; cases he; cases hv
  | _ => intro w T φ hv _ _; cases hv

end Flick
