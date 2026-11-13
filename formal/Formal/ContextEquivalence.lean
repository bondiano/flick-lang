import Formal.FactsCongruence

/-!
# Pointwise congruence of typing contexts
-/

namespace Flick

inductive CtxEquiv : List Scheme → List Scheme → Prop
  | nil : CtxEquiv [] []
  | cons {a b Γ Γ'} : a.Equiv b → CtxEquiv Γ Γ' → CtxEquiv (a :: Γ) (b :: Γ')

theorem CtxEquiv.refl (Γ : List Scheme) : CtxEquiv Γ Γ := by
  induction Γ with
  | nil => exact .nil
  | cons s Γ ih => exact .cons (.refl _) ih

theorem CtxEquiv.length {Γ Γ'} (h : CtxEquiv Γ Γ') : Γ.length = Γ'.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem CtxEquiv.append {Γ Γ' Δ Δ'} (h : CtxEquiv Γ Γ') (h' : CtxEquiv Δ Δ') :
    CtxEquiv (Γ ++ Δ) (Γ' ++ Δ') := by
  induction h with
  | nil => exact h'
  | cons he _ ih => exact .cons he ih

theorem CtxEquiv.lookup {Γ Γ'} (h : CtxEquiv Γ Γ') :
    ∀ {i : Nat} {s : Scheme}, Γ[i]? = some s → ∃ s', Γ'[i]? = some s' ∧ s.Equiv s' := by
  induction h with
  | nil => intros; simp_all
  | cons he _ ih =>
    intro i s hs
    cases i with
    | zero => simp only [List.getElem?_cons_zero, Option.some.injEq] at hs
              subst hs; exact ⟨_, rfl, he⟩
    | succ i => exact ih hs

theorem CtxEquiv.substT {Γ Γ'} (h : CtxEquiv Γ Γ') (σ : Atom → Ty) :
    CtxEquiv (Γ.map (Scheme.substT σ)) (Γ'.map (Scheme.substT σ)) := by
  induction h with
  | nil => exact .nil
  | cons he _ ih => exact .cons (he.substT σ) ih

theorem CtxEquiv.up {Γ Γ'} (h : CtxEquiv Γ Γ') (k : Nat) :
    CtxEquiv (Γ.map (Scheme.up k)) (Γ'.map (Scheme.up k)) := h.substT _

theorem CtxEquiv.of_subst (Γ : List Scheme) {σ τ : Atom → Ty}
    (h : ∀ a, Ty.Equiv (σ a) (τ a)) :
    CtxEquiv (Γ.map (Scheme.substT σ)) (Γ.map (Scheme.substT τ)) := by
  induction Γ with
  | nil => exact .nil
  | cons s Γ ih => exact .cons (.of_subst h s) ih

theorem CtxEquiv.mono {Δ Δ' : List Ty} (h : Ty.ArgsEquiv Δ Δ') :
    CtxEquiv (Δ.map Scheme.mono) (Δ'.map Scheme.mono) := by
  induction h with
  | nil => exact .nil
  | cons he _ ih => exact .cons (.mono he) ih

theorem Ty.ArgsEquiv.lookup {Ψ Ψ'} (h : Ty.ArgsEquiv Ψ Ψ') {i : Nat} {t : Ty}
    (ht : Ψ[i]? = some t) : ∃ t', Ψ'[i]? = some t' ∧ Ty.Equiv t t' := by
  have hi : i < Ψ'.length := h.length ▸ (List.getElem?_eq_some_iff.mp ht).1
  exact ⟨_, List.getElem?_eq_getElem hi, h.get i _ _ ht (List.getElem?_eq_getElem hi)⟩

theorem Ty.ArgsEquiv.up {Ψ Ψ'} (h : Ty.ArgsEquiv Ψ Ψ') (k : Nat) :
    Ty.ArgsEquiv (Ψ.map (Ty.up k)) (Ψ'.map (Ty.up k)) := by
  induction h with
  | nil => exact .nil
  | cons he _ ih => exact .cons (he.up k) ih

end Flick
