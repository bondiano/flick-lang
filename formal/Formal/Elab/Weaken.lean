import Formal.Elab.T6a

/-!
# Elaboration: renaming of term variables and the stage `P` (elaboration.md §2.4)

`Elab.rename`: an elaboration survives a renaming of term variables that keeps their schemes
(weakening is the shift `Nat.succ`). `Elab.anyStage`: an elaboration at `P` is one at any stage.
-/

namespace Flick.Elab

/-- A renaming under one binder. -/
def liftR (ρ : Nat → Nat) : Nat → Nat
  | 0 => 0
  | n + 1 => ρ n + 1

def Exp.rename (ρ : Nat → Nat) : Exp → Exp
  | .var x => .var (ρ x)
  | .meth m => .meth m
  | .const c => .const c
  | .app f a => .app (f.rename ρ) (a.rename ρ)
  | .lam b => .lam (b.rename (liftR ρ))
  | .let_ e b => .let_ (e.rename ρ) (b.rename (liftR ρ))
  | .letSig σ e b => .letSig σ (e.rename ρ) (b.rename (liftR ρ))
  | .ite c a b => .ite (c.rename ρ) (a.rename ρ) (b.rename ρ)
  | .caseOpt e n s => .caseOpt (e.rename ρ) (n.rename ρ) (s.rename (liftR ρ))

/-- `lamD` binds premises, not term variables. -/
def Tm.rename (ρ : Nat → Nat) : Tm → Tm
  | .var x => .var (ρ x)
  | .hole i => .hole i
  | .dict d => .dict d
  | .meth e m => .meth (e.rename ρ) m
  | .const c => .const c
  | .app f a => .app (f.rename ρ) (a.rename ρ)
  | .lam b => .lam (b.rename (liftR ρ))
  | .let_ e b => .let_ (e.rename ρ) (b.rename (liftR ρ))
  | .lamD l b => .lamD l (b.rename ρ)
  | .ite c a b => .ite (c.rename ρ) (a.rename ρ) (b.rename ρ)
  | .caseOpt e n s => .caseOpt (e.rename ρ) (n.rename ρ) (s.rename (liftR ρ))

theorem Tm.rename_dapps (ρ : Nat → Nat) (f : Tm) (ds : List Dict) :
    (f.dapps ds).rename ρ = (f.rename ρ).dapps ds := by
  induction ds generalizing f with
  | nil => rfl
  | cons d ds ih => exact ih (.app f (.dict d))

variable {P : Prog}

theorem liftR_get {Γ Γ' : List Scheme} {ρ : Nat → Nat} (h : ∀ x, Γ'[ρ x]? = Γ[x]?) (s : Scheme) :
    ∀ x, (s :: Γ')[liftR ρ x]? = (s :: Γ)[x]?
  | 0 => rfl
  | x + 1 => by simpa [liftR] using h x

/-- **Renaming.** -/
theorem Elab.rename {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') {Γ' : List Scheme} {ρ : Nat → Nat}
    (hρ : ∀ x, Γ'[ρ x]? = Γ[x]?) : Elab P Γ' Δ (e.rename ρ) τ φ (e'.rename ρ) := by
  induction h generalizing Γ' ρ with
  | var hx hk he =>
    rw [Tm.rename_dapps]; exact .var (by rw [hρ]; exact hx) hk he
  | meth hm hc hC hk h₀ he => rw [Tm.rename_dapps]; exact .meth hm hc hC hk h₀ he
  | const hc hk he => rw [Tm.rename_dapps]; exact .const hc hk he
  | app _ _ hψ ih₁ ih₂ => exact .app (ih₁ hρ) (ih₂ hρ) hψ
  | lam _ ih => exact .lam (ih (liftR_get hρ _))
  | let_ _ _ ih₁ ih₂ => exact .let_ (ih₁ hρ) (ih₂ (liftR_get hρ _))
  | letSig _ hp hφ _ ih₁ ih₂ =>
    refine .letSig (ih₁ fun x => ?_) hp hφ (ih₂ (liftR_get hρ _))
    simp only [List.getElem?_map, hρ]
  | ite _ _ _ ihc iha ihb => exact .ite (ihc hρ) (iha hρ) (ihb hρ)
  | caseOpt _ _ _ ihe ihn ihs => exact .caseOpt (ihe hρ) (ihn hρ) (ihs (liftR_get hρ _))

/-- **Weakening** by one variable. -/
theorem Elab.shift {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') (s : Scheme) :
    Elab P (s :: Γ) Δ (e.rename Nat.succ) τ φ (e'.rename Nat.succ) :=
  h.rename fun _ => rfl

/-- What runs at `P` runs at any stage. -/
theorem Elab.anyStage {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') (hφ : φ = .P) (ψ : Stage) :
    Elab P Γ Δ e τ ψ e' := by
  induction h generalizing ψ with
  | var hx hk he => exact .var hx hk he
  | meth hm hc hC hk h₀ he => exact .meth hm hc hC hk h₀ he
  | const hc hk he => exact .const hc hk he
  | app _ _ hψ ih₁ ih₂ =>
    subst hφ
    exact .app (ih₁ rfl ψ) (ih₂ rfl ψ) (.inl (by rcases hψ with h | h <;> exact h))
  | lam h _ => exact .lam h
  | let_ _ _ ih₁ ih₂ => exact .let_ (ih₁ hφ ψ) (ih₂ hφ ψ)
  | letSig h₁ hp hφ₁ _ _ ih₂ =>
    subst hφ
    exact .letSig h₁ hp (.inl (by rcases hφ₁ with h | h <;> exact h)) (ih₂ rfl ψ)
  | ite _ _ _ ihc iha ihb => exact .ite (ihc hφ ψ) (iha hφ ψ) (ihb hφ ψ)
  | caseOpt _ _ _ ihe ihn ihs => exact .caseOpt (ihe hφ ψ) (ihn hφ ψ) (ihs hφ ψ)

end Flick.Elab
