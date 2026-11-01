import Formal.Preservation

/-!
# T1: type safety (core.md §4)
-/

namespace Flick

variable {S : Sig}

/-- Every configuration reachable from a well-typed one is well-typed at the
same type and stage. -/
theorem steps_typed {root Q μ₀ e₀ μ e} (hroot : RootTy S root) (hs : Steps root Q μ₀ e₀ μ e) :
    ∀ {Ψ₀ t φ}, StoreTy S Ψ₀ μ₀ → Typ S Ψ₀ [] e₀ (.ex t φ) →
      ∃ Ψ, StoreTy S Ψ μ ∧ Typ S Ψ [] e (.ex t φ) := by
  induction hs with
  | refl => intro Ψ₀ _ _ hμ h; exact ⟨Ψ₀, hμ, h⟩
  | step h₁ _ _ ih =>
    intro _ _ _ hμ h
    obtain ⟨_, hμ', h'⟩ := preservation hroot h hμ h₁
    exact ih hμ' h'

/-- A raised value has a type of errors of the stage. -/
theorem raise_errors {Ψ v t φ} (hv : Value v) (h : Typ S Ψ [] (.raise v) (.ex t φ)) :
    Typ S Ψ [] v (.ex (.errors φ.err) .pure) := by
  obtain ⟨ε, hv', hε⟩ := raise_inv hv h
  obtain ⟨_, _, _, hK, hS, hT, ht⟩ := canonical_errors hv hv'
  exact .errVal hv ht (Stage.le_err hε _ hT)

/-- **T1.** A program `· ⊢ e₀ : τ ! φ₀` whose services `Σ₀` provides (Г3) reaches
only values, `raise v` with `v : Errors ε₀`, or configurations that step. -/
theorem T1 {root e₀ t φ₀ μ e} (hroot : RootTy S root)
    (hcov : ∀ N, φ₀.svc N → (root N).isSome) (h₀ : Typ S [] [] e₀ (.ex t φ₀))
    (hs : Steps root (fun _ => True) [] e₀ μ e) :
    (Value e ∧ ∃ Ψ, StoreTy S Ψ μ ∧ Typ S Ψ [] e (.ex t .pure)) ∨
      (∃ v, e = .raise v ∧ ∃ Ψ, Typ S Ψ [] v (.ex (.errors φ₀.err) .pure)) ∨
      ∃ l o, Step root l μ e o := by
  obtain ⟨Ψ, hμ, h⟩ := steps_typed hroot hs ⟨rfl, by simp⟩ h₀
  rcases progress (root := root) hμ h rfl rfl with hv | ⟨v, hv, rfl⟩ | hstep | ⟨_, N, _, _, _, hN, hD⟩
  · exact .inl ⟨hv, Ψ, hμ, h.valueAny hv _⟩
  · exact .inr (.inl ⟨v, rfl, Ψ, raise_errors hv h⟩)
  · exact .inr (.inr hstep)
  · have := hcov N hD; rw [hN] at this; cases this

end Flick
