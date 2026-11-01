import Formal.Preservation

/-!
# T1 without effects (core.md §4, stage P1a)
-/

namespace Flick

variable {S : Sig}

/-- Every reachable configuration is well-typed at the type of the program. -/
theorem steps_typed {μ₀ e₀ μ e} (hs : Steps μ₀ e₀ μ e) :
    ∀ {Ψ₀ t}, StoreTy S Ψ₀ μ₀ → Typ S Ψ₀ [] e₀ (.ex t) →
      ∃ Ψ, StoreTy S Ψ μ ∧ Typ S Ψ [] e (.ex t) := by
  induction hs with
  | refl => intro Ψ₀ _ hμ h; exact ⟨Ψ₀, hμ, h⟩
  | step h₁ _ ih =>
    intro _ _ hμ h
    obtain ⟨X, hμ', h'⟩ := preservation h hμ h₁
    exact ih hμ' h'

/-- **T1 (type safety), without effects.** A closed well-typed program either
reaches a well-typed value or can make a step (to a configuration or `die`). -/
theorem T1 {e₀ t μ e} (h₀ : Typ S [] [] e₀ (.ex t)) (hs : Steps [] e₀ μ e) :
    (Value e ∧ ∃ Ψ, StoreTy S Ψ μ ∧ Typ S Ψ [] e (.ex t)) ∨ ∃ o, Step μ e o := by
  obtain ⟨Ψ, hμ, h⟩ := steps_typed hs ⟨rfl, by simp⟩ h₀
  rcases progress hμ h rfl t rfl with hv | hstep
  · exact .inl ⟨hv, Ψ, hμ, h⟩
  · exact .inr hstep

end Flick
