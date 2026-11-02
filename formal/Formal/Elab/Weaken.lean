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

theorem Exp.free_rename {ρ : Nat → Nat} {e : Exp} {y : Nat} (h : (e.rename ρ).Free y) :
    ∃ x, e.Free x ∧ ρ x = y := by
  induction e generalizing ρ y with
  | var x => exact ⟨x, rfl, h⟩
  | meth | const => cases h
  | app f a ihf iha =>
    rcases h with h | h
    · obtain ⟨x, hx, rfl⟩ := ihf h; exact ⟨x, .inl hx, rfl⟩
    · obtain ⟨x, hx, rfl⟩ := iha h; exact ⟨x, .inr hx, rfl⟩
  | lam b ih =>
    obtain ⟨x, hx, hρ⟩ := ih h
    cases x with
    | zero => cases hρ
    | succ x => exact ⟨x, hx, Nat.succ.inj hρ⟩
  | let_ e b ihe ihb | letSig _ e b ihe ihb =>
    rcases h with h | h
    · obtain ⟨x, hx, rfl⟩ := ihe h; exact ⟨x, .inl hx, rfl⟩
    · obtain ⟨x, hx, hρ⟩ := ihb h
      cases x with
      | zero => cases hρ
      | succ x => exact ⟨x, .inr hx, Nat.succ.inj hρ⟩
  | ite c a b ihc iha ihb =>
    rcases h with h | h | h
    · obtain ⟨x, hx, rfl⟩ := ihc h; exact ⟨x, .inl hx, rfl⟩
    · obtain ⟨x, hx, rfl⟩ := iha h; exact ⟨x, .inr (.inl hx), rfl⟩
    · obtain ⟨x, hx, rfl⟩ := ihb h; exact ⟨x, .inr (.inr hx), rfl⟩
  | caseOpt e n s ihe ihn ihs =>
    rcases h with h | h | h
    · obtain ⟨x, hx, rfl⟩ := ihe h; exact ⟨x, .inl hx, rfl⟩
    · obtain ⟨x, hx, rfl⟩ := ihn h; exact ⟨x, .inr (.inl hx), rfl⟩
    · obtain ⟨x, hx, hρ⟩ := ihs h
      cases x with
      | zero => cases hρ
      | succ x => exact ⟨x, .inr (.inr hx), Nat.succ.inj hρ⟩

theorem heldS_rename {Γ Γ' : List Scheme} {ρ : Nat → Nat} (hρ : ∀ x, Γ'[ρ x]? = Γ[x]?)
    {e : Exp} {n : Nat} (h : heldS Γ' (e.rename ρ) n) : heldS Γ e n := by
  obtain ⟨y, s, hy, hs, hr⟩ := h
  obtain ⟨x, hx, rfl⟩ := Exp.free_rename hy
  exact ⟨x, s, hx, by rw [← hρ]; exact hs, hr⟩

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
  | @lam Γ Δ b a t q φ ψ b' _ hh ih =>
    exact .lam (ih (liftR_get hρ _)) fun n hn => hh n (heldS_rename (e := .lam b) hρ hn)
  | let_ _ _ ih₁ ih₂ => exact .let_ (ih₁ hρ) (ih₂ (liftR_get hρ _))
  | @letSig Γ Δ σ e b t φ₁ φ e' b' _ hp hφ hq _ ih₁ ih₂ =>
    have hρ' : ∀ x, (Γ'.map (·.subst (up σ.k)))[ρ x]? = (Γ.map (·.subst (up σ.k)))[x]? := by
      intro x; simp only [List.getElem?_map, hρ]
    exact .letSig (ih₁ hρ') hp hφ (fun hc n hn => hq hc n (heldS_rename hρ' hn))
      (ih₂ (liftR_get hρ _))
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
  | lam h hh _ => exact .lam h hh
  | let_ _ _ ih₁ ih₂ => exact .let_ (ih₁ hφ ψ) (ih₂ hφ ψ)
  | letSig h₁ hp hφ₁ hq _ _ ih₂ =>
    subst hφ
    exact .letSig h₁ hp (.inl (by rcases hφ₁ with h | h <;> exact h)) hq (ih₂ rfl ψ)
  | ite _ _ _ ihc iha ihb => exact .ite (ihc hφ ψ) (iha hφ ψ) (ihb hφ ψ)
  | caseOpt _ _ _ ihe ihn ihs => exact .caseOpt (ihe hφ ψ) (ihn hφ ψ) (ihs hφ ψ)

end Flick.Elab
