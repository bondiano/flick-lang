import Formal.Substitution
import Formal.TypingContextEquiv

/-!
# Substitution by variables with congruent schemes

First align the source context with the schemes of the target variables. The
existing substitution lemma then sees exactly those schemes. Closed dictionary
entries keep their own schemes, facts and typing premises.
-/

namespace Flick

variable {S : Sig}

def ClosedDictSub (S : Sig) (Ψ : List Ty) (Θ : Facts) (Γ : List Scheme)
    (e : Tm) (s : Scheme) : Prop :=
  (∀ x, ¬ e.Free x) ∧ (∀ a, ¬ e.Has a) ∧
    Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ.map (Scheme.up s.k)) e (.ex s.t .pure) ∧
    s.k = 0 ∧ ∃ c u, s.t = .dict c u

private theorem align_entries {P : Nat → Scheme → Prop} {Γ : List Scheme}
    (h : ∀ i s, Γ[i]? = some s → ∃ s', s.Equiv s' ∧ P i s') :
    ∃ Γ', CtxEquiv Γ Γ' ∧ ∀ i s, Γ'[i]? = some s → P i s := by
  induction Γ generalizing P with
  | nil => exact ⟨[], .nil, by intros; simp_all⟩
  | cons s Γ ih =>
    obtain ⟨s', he, hp⟩ := h 0 s rfl
    obtain ⟨Γ', hΓ, ht⟩ := ih (P := fun i s => P (i + 1) s) fun i s hs => h (i + 1) s hs
    refine ⟨s' :: Γ', .cons he hΓ, ?_⟩
    intro i t hi
    cases i with
    | zero => simp only [List.getElem?_cons_zero, Option.some.injEq] at hi
              subst hi; exact hp
    | succ i => exact ht i t hi

theorem Typ.subst_congruent_entries {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J)
    {Γ' : List Scheme} {τ : Nat → Tm} (hn : e.NoIns)
    (hs : ∀ i s, Γ[i]? = some s →
      (∃ j s', τ i = .var j ∧ Γ'[j]? = some s' ∧ s.Equiv s') ∨
        ClosedDictSub S Ψ Θ Γ' (τ i) s) :
    Typ S Ψ Θ Γ' (e.subst τ) J := by
  have ha : ∀ i s, Γ[i]? = some s → ∃ s', s.Equiv s' ∧
      ((∃ j, τ i = .var j ∧ Γ'[j]? = some s') ∨ ClosedDictSub S Ψ Θ Γ' (τ i) s') := by
    intro i s hi
    rcases hs i s hi with ⟨j, s', hj, hget, he⟩ | hclosed
    · exact ⟨s', he, .inl ⟨j, hj, hget⟩⟩
    · exact ⟨s, .refl _, .inr hclosed⟩
  obtain ⟨Γ₀, hΓ, hent⟩ := align_entries ha
  apply (h.context_equiv hn (.refl Ψ) hΓ).subst
  intro i s hi
  rcases hent i s hi with ⟨j, hj, hget⟩ | ⟨hf, hns, ht, hd⟩
  · rw [hj]
    exact ⟨Typ.var_self hget, fun h => False.elim h, .inr (.inl ⟨j, rfl, hget⟩)⟩
  · exact ⟨ht, hns .ins, .inr (.inr (.inr hd))⟩

end Flick
