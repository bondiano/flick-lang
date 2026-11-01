import Formal.Effects

/-!
# Generalization: lemma 3.1 and T3 (core.md §2.5, §4)

(Let-Gen) types its right-hand side with `P` and with the generalized
variables fresh: the store typing is shifted past them (`Ψ.map (Ty.up k)`), so
no cell type mentions them. Л3 (`ctx_typing`) says that a hole under such a
right-hand side is pure, and lemma 3.1 that a pure computation does not touch
the store.
-/

namespace Flick

variable {S : Sig}

/-- `t` does not mention the type variables `0 … k-1`. -/
def Ty.Avoids (k : Nat) : Ty → Prop
  | .tvar n => k ≤ n
  | .arrow a _ r => a.Avoids k ∧ r.Avoids k
  | .ref t => t.Avoids k
  | .rcons _ t r => t.Avoids k ∧ r.Avoids k
  | .eff _ _ t => t.Avoids k
  | _ => True

theorem Ty.up_avoids (t : Ty) (k : Nat) : (t.up k).Avoids k := by
  induction t <;> simp_all [Ty.up, Ty.substT, Ty.Avoids, shiftV]

/-- **Lemma 3.1.** While a subterm with `Γ ⊢ e : τ ! P` is evaluated, the store
does not grow (nor change). -/
theorem lemma_3_1 {root Ψ μ e t μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (h : Typ S Ψ [] e (.ex t .pure)) (hs : Steps root (fun _ => True) μ e μ' e') : μ' = μ :=
  (T2a hroot hμ h hs).2.1

/-- **T3.** Every configuration reachable from a program `· ⊢ e₀ : τ ! φ₀` has a
store typing `Ψ` with `⊢ μ : Ψ` and `·;Ψ ⊢ e : τ ! φ₀`, and later configurations
extend `Ψ`: a cell keeps the type it was allocated with, so `Ref.get ℓ` returns
and `Ref.set ℓ v` writes a value of type `Ψ(ℓ)`. A redex under the right-hand
side of a (Let-Gen) with `ᾱ ≠ ∅` is pure, and the cell types it sees do not
mention `ᾱ`. -/
theorem T3 {root e₀ t φ₀ μ e} (hroot : RootTy S root) (h₀ : Typ S [] [] e₀ (.ex t φ₀))
    (hs : Steps root (fun _ => True) [] e₀ μ e) :
    ∃ Ψ, StoreTy S Ψ μ ∧ Typ S Ψ [] e (.ex t φ₀) ∧
      (∀ {μ' e'}, Steps root (fun _ => True) μ e μ' e' →
        ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) [] e' (.ex t φ₀)) ∧
      ∀ E r, OkE E → ¬ Value r → e = plug E r →
        ∃ K t' φ', Typ S (Ψ.map (Ty.up K)) [] r (.ex t' φ') ∧
          (K ≠ 0 → φ' = .pure ∧ ∀ u ∈ Ψ.map (Ty.up K), u.Avoids K) := by
  obtain ⟨Ψ, hμ, h⟩ := steps_typed hroot hs ⟨rfl, by simp⟩ h₀
  simp only [List.nil_append] at hμ h
  refine ⟨Ψ, hμ, h, fun hs' => steps_typed hroot hs' hμ h, ?_⟩
  intro E r hE hr he
  subst he
  obtain ⟨K, t', φ', hr', _, hK, _⟩ := ctx_typing E hE hr h
  refine ⟨K, t', φ', hr', fun hk => ⟨hK hk, ?_⟩⟩
  intro u hu
  simp only [List.mem_map] at hu
  obtain ⟨u₀, _, rfl⟩ := hu
  exact u₀.up_avoids K

end Flick
