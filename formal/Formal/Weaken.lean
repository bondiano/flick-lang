import Formal.Covers

/-!
# Structural lemmas: store weakening and renaming
-/

namespace Flick

variable {S : Sig}

theorem PatTy.binds {fl p t Δ} (h : PatTy S fl p t Δ) : Δ.length = p.binds := by
  induction h <;> simp_all [Pat.binds]

theorem getElem?_lt {α} {l : List α} {i : Nat} {a : α} (h : l[i]? = some a) : i < l.length := by
  obtain ⟨hi, _⟩ := List.getElem?_eq_some_iff.mp h
  exact hi

/-- Store weakening: `Ψ ⊆ Ψ'` keeps every judgment. -/
theorem Typ.weakenStore {Ψ Γ e J} (h : Typ S Ψ Γ e J) (X : List Ty) : Typ S (Ψ ++ X) Γ e J := by
  induction h with
  | loc hl =>
    exact .loc (by rw [List.getElem?_append_left (getElem?_lt hl)]; exact hl)
  | var hi => exact .var hi
  | lam _ ih => exact .lam ih
  | app _ _ ih₁ ih₂ => exact .app ih₁ ih₂
  | let_ _ _ ih₁ ih₂ => exact .let_ ih₁ ih₂
  | letrec _ _ ih₁ ih₂ => exact .letrec ih₁ ih₂
  | fix _ ih => exact .fix ih
  | lit => exact .lit
  | const => exact .const
  | ctor hK => exact .ctor hK
  | rnil => exact .rnil
  | rcons hl _ _ ih₁ ih₂ => exact .rcons hl ih₁ ih₂
  | proj _ hf ih => exact .proj ih hf
  | update _ hf _ ih₁ ih₂ => exact .update ih₁ hf ih₂
  | match_ _ _ hc ih₁ ih₂ => exact .match_ ih₁ ih₂ hc
  | anil => exact .anil
  | acons hp _ _ ih₁ ih₂ => exact .acons hp ih₁ ih₂
  | aguard hp _ _ _ ih₁ ih₂ ih₃ => exact .aguard hp ih₁ ih₂ ih₃
  | todo => exact .todo

/-- A renaming `ξ : Γ → Γ'`. -/
def RenOk (Γ Γ' : List Ty) (ξ : Nat → Nat) : Prop :=
  ∀ (i : Nat) t, Γ[i]? = some t → Γ'[ξ i]? = some t

theorem RenOk.lift {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) (Δ : List Ty) {n} (hn : Δ.length = n) :
    RenOk (Δ ++ Γ) (Δ ++ Γ') (liftR n ξ) := by
  intro i t hi
  subst hn
  unfold liftR
  split
  · rw [List.getElem?_append_left (by omega)] at hi ⊢; exact hi
  · rw [List.getElem?_append_right (by omega)] at hi ⊢
    simpa using h _ _ hi

theorem Typ.rename {Ψ Γ e J} (h : Typ S Ψ Γ e J) :
    ∀ {Γ' ξ}, RenOk Γ Γ' ξ → Typ S Ψ Γ' (e.rename ξ) J := by
  induction h with
  | var hi => intro _ _ hξ; exact .var (hξ _ _ hi)
  | lam _ ih => intro _ _ hξ; exact .lam (ih (hξ.lift [_] rfl))
  | app _ _ ih₁ ih₂ => intro _ _ hξ; exact .app (ih₁ hξ) (ih₂ hξ)
  | let_ _ _ ih₁ ih₂ => intro _ _ hξ; exact .let_ (ih₁ hξ) (ih₂ (hξ.lift [_] rfl))
  | letrec _ _ ih₁ ih₂ =>
    intro _ _ hξ; exact .letrec (ih₁ (hξ.lift [_, _] rfl)) (ih₂ (hξ.lift [_] rfl))
  | fix _ ih => intro _ _ hξ; exact .fix (ih (hξ.lift [_, _] rfl))
  | lit => intro _ _ _; exact .lit
  | loc hl => intro _ _ _; exact .loc hl
  | const => intro _ _ _; exact .const
  | ctor hK => intro _ _ _; exact .ctor hK
  | rnil => intro _ _ _; exact .rnil
  | rcons hl _ _ ih₁ ih₂ => intro _ _ hξ; exact .rcons hl (ih₁ hξ) (ih₂ hξ)
  | proj _ hf ih => intro _ _ hξ; exact .proj (ih hξ) hf
  | update _ hf _ ih₁ ih₂ => intro _ _ hξ; exact .update (ih₁ hξ) hf (ih₂ hξ)
  | match_ _ _ hc ih₁ ih₂ =>
    intro _ _ hξ
    exact .match_ (ih₁ hξ) (ih₂ hξ) (by rw [Tm.unguarded_rename]; exact hc.rename)
  | anil => intro _ _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ => intro _ _ hξ; exact .acons hp (ih₁ (hξ.lift _ hp.binds)) (ih₂ hξ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ _ hξ
    exact .aguard hp (ih₁ (hξ.lift _ hp.binds)) (ih₂ (hξ.lift _ hp.binds)) (ih₃ hξ)
  | todo => intro _ _ _; exact .todo

end Flick
