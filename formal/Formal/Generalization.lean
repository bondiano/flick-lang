import Formal.Effects

/-!
# Generalization: lemma 3.1 and T3 (core.md §2.5, §4)

(Let-Gen) types its right-hand side with `P` and with the generalized
variables fresh: the store typing is shifted past them (`Ψ.map (Ty.up k)`), so
no cell type mentions them. L3 (`ctx_typing`) says that a hole under such a
right-hand side is pure, and lemma 3.1 that a pure computation does not touch
the store.
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

/-- `t` does not mention the variables `0 … k-1`: it is a type shifted past them. -/
def Ty.Avoids (k : Nat) (t : Ty) : Prop := ∃ t₀ : Ty, t = t₀.up k

theorem Ty.up_avoids (t : Ty) (k : Nat) : (t.up k).Avoids k := ⟨t, rfl⟩

/-- **Lemma 3.1.** While a subterm with `Γ ⊢ e : τ ! P` is evaluated, the store
and the regions do not grow (nor change). -/
theorem lemma_3_1 {root Ψ Λ μ e t Λ' μ' e'} (hroot : RootTyM S mode root) (hμ : StoreTyM S mode Ψ μ)
    (hb : CfgBelow Λ μ e) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (h : TypM S mode Ψ Facts.age [] e (.ex t .pure)) (hs : Steps root (fun _ => True) Λ μ e Λ' μ' e') :
    μ' = μ ∧ Λ' = Λ :=
  ⟨(T2a hroot hμ hb hΨ h hs).2.1, (T2a hroot hμ hb hΨ h hs).2.2.1⟩

/-- **T3.** Every configuration reachable from a program `· ⊢ e₀ : τ ! φ₀` has a
store typing `Ψ` under which it is typed with the main thread at `τ ! φ₀`, and
later configurations extend `Ψ`: a cell keeps the type it was allocated with, so
`Ref.get ℓ` returns and `Ref.set ℓ v` writes a value of type `Ψ(ℓ)`. A redex
under the right-hand side of a (Let-Gen) with `ᾱ ≠ ∅` is pure, and the cell types
it sees do not mention `ᾱ`. -/
theorem T3 {root e₀ t φ₀ c} (hroot : RootTyM S mode root) (h₀ : TypM S mode [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ∃ Ψ, CfgTyM S mode Ψ c t φ₀ ∧
      (∀ {c'}, CSteps root c c' → ∃ X, CfgTyM S mode (Ψ ++ X) c' t φ₀) ∧
      ∀ E r, OkE E → ¬ Value r → c.main = plug E r →
        ∃ K Θ' t' φ', TypM S mode (Ψ.map (Ty.up K)) Θ' [] r (.ex t' φ') ∧
          (K ≠ 0 → φ' = .pure ∧ ∀ u ∈ Ψ.map (Ty.up K), u.Avoids K) := by
  obtain ⟨Ψ, hc⟩ := steps_typed_mode hroot hs ht hφ (CfgTyM.init h₀ hp)
  simp only [List.nil_append] at hc
  refine ⟨Ψ, hc, fun hs' => steps_typed_mode hroot hs' ht hφ hc, ?_⟩
  intro E r hE hr he
  obtain ⟨_, h, _⟩ := hc
  rw [he] at h
  obtain ⟨K, Θ', t', φ', hr', _, hK, _⟩ := ctx_typing_mode E hE hr h
  refine ⟨K, Θ', t', φ', hr', fun hk => ⟨?_, ?_⟩⟩
  · rcases hK with h | ⟨h, -⟩
    · exact h
    · exact (hk h).elim
  · intro u hu
    simp only [List.mem_map] at hu
    obtain ⟨u₀, _, rfl⟩ := hu
    exact u₀.up_avoids K

end Flick
