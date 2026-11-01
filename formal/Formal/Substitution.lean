import Formal.Weaken

/-!
# Substitution lemma (Л2 for monotypes)
-/

namespace Flick

variable {S : Sig}

/-- A substitution `σ : Γ → Γ'`: each variable goes to a term of its type. -/
def SubOk (S : Sig) (Ψ Γ Γ' : List Ty) (σ : Nat → Tm) : Prop :=
  ∀ (i : Nat) t, Γ[i]? = some t → Typ S Ψ Γ' (σ i) (.ex t)

theorem SubOk.lift {Ψ Γ Γ' σ} (h : SubOk S Ψ Γ Γ' σ) (Δ : List Ty) {n} (hn : Δ.length = n) :
    SubOk S Ψ (Δ ++ Γ) (Δ ++ Γ') (liftS n σ) := by
  intro i t hi
  subst hn
  unfold liftS
  split
  · rw [List.getElem?_append_left (by omega)] at hi
    exact .var (by rw [List.getElem?_append_left (by omega)]; exact hi)
  · rw [List.getElem?_append_right (by omega)] at hi
    exact (h _ _ hi).rename fun j s hj => by
      rw [List.getElem?_append_right (by omega)]; simpa using hj

theorem Typ.subst {Ψ Γ e J} (h : Typ S Ψ Γ e J) :
    ∀ {Γ' σ}, SubOk S Ψ Γ Γ' σ → Typ S Ψ Γ' (e.subst σ) J := by
  induction h with
  | var hi => intro _ _ hσ; exact hσ _ _ hi
  | lam _ ih => intro _ _ hσ; exact .lam (ih (hσ.lift [_] rfl))
  | app _ _ ih₁ ih₂ => intro _ _ hσ; exact .app (ih₁ hσ) (ih₂ hσ)
  | let_ _ _ ih₁ ih₂ => intro _ _ hσ; exact .let_ (ih₁ hσ) (ih₂ (hσ.lift [_] rfl))
  | letrec _ _ ih₁ ih₂ =>
    intro _ _ hσ; exact .letrec (ih₁ (hσ.lift [_, _] rfl)) (ih₂ (hσ.lift [_] rfl))
  | fix _ ih => intro _ _ hσ; exact .fix (ih (hσ.lift [_, _] rfl))
  | lit => intro _ _ _; exact .lit
  | loc hl => intro _ _ _; exact .loc hl
  | const => intro _ _ _; exact .const
  | ctor hK => intro _ _ _; exact .ctor hK
  | rnil => intro _ _ _; exact .rnil
  | rcons hl _ _ ih₁ ih₂ => intro _ _ hσ; exact .rcons hl (ih₁ hσ) (ih₂ hσ)
  | proj _ hf ih => intro _ _ hσ; exact .proj (ih hσ) hf
  | update _ hf _ ih₁ ih₂ => intro _ _ hσ; exact .update (ih₁ hσ) hf (ih₂ hσ)
  | match_ _ h₂ hc ih₁ ih₂ =>
    intro _ _ hσ
    exact .match_ (ih₁ hσ) (ih₂ hσ) (by rw [Typ.unguarded_subst h₂]; exact hc.subst)
  | anil => intro _ _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ => intro _ _ hσ; exact .acons hp (ih₁ (hσ.lift _ hp.binds)) (ih₂ hσ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ _ hσ
    exact .aguard hp (ih₁ (hσ.lift _ hp.binds)) (ih₂ (hσ.lift _ hp.binds)) (ih₃ hσ)
  | todo => intro _ _ _; exact .todo

/-- `δ` gives the variables `Δ` their types in `Γ`. -/
def Inst (S : Sig) (Ψ Γ : List Ty) (δ : List Tm) (Δ : List Ty) : Prop :=
  δ.length = Δ.length ∧
    ∀ (i : Nat) v t, δ[i]? = some v → Δ[i]? = some t → Typ S Ψ Γ v (.ex t)

theorem Inst.nil {Ψ Γ} : Inst S Ψ Γ [] [] := ⟨rfl, by simp⟩

theorem Inst.cons {Ψ Γ v a δ Δ} (hv : Typ S Ψ Γ v (.ex a)) (h : Inst S Ψ Γ δ Δ) :
    Inst S Ψ Γ (v :: δ) (a :: Δ) := by
  refine ⟨by simp [h.1], fun i w t hw ht => ?_⟩
  cases i with
  | zero => simp at hw ht; subst hw ht; exact hv
  | succ i => exact h.2 i w t (by simpa using hw) (by simpa using ht)

theorem Inst.append {Ψ Γ δ₁ δ₂ Δ₁ Δ₂} (h₁ : Inst S Ψ Γ δ₁ Δ₁) (h₂ : Inst S Ψ Γ δ₂ Δ₂) :
    Inst S Ψ Γ (δ₁ ++ δ₂) (Δ₁ ++ Δ₂) := by
  refine ⟨by simp [h₁.1, h₂.1], fun i w t hw ht => ?_⟩
  have hl := h₁.1
  by_cases hi : i < δ₁.length
  · rw [List.getElem?_append_left hi] at hw
    rw [List.getElem?_append_left (by omega)] at ht
    exact h₁.2 i w t hw ht
  · rw [List.getElem?_append_right (by omega)] at hw
    rw [List.getElem?_append_right (by omega)] at ht
    rw [h₁.1] at hw
    exact h₂.2 _ w t hw ht

theorem Inst.ok {Ψ Γ δ Δ} (h : Inst S Ψ Γ δ Δ) : SubOk S Ψ (Δ ++ Γ) Γ (inst δ) := by
  intro i t hi
  have hl := h.1
  unfold inst
  split
  · next hlt =>
    rw [List.getElem?_append_left (by omega)] at hi
    exact h.2 i _ t (List.getElem?_eq_getElem hlt) hi
  · rw [List.getElem?_append_right (by omega)] at hi
    exact .var (by rw [h.1]; exact hi)

/-- `e[δ]` for a body typed under `Δ`. -/
theorem Typ.inst {Ψ Γ e J δ Δ} (h : Typ S Ψ (Δ ++ Γ) e J) (hδ : Inst S Ψ Γ δ Δ) :
    Typ S Ψ Γ (e.subst (inst δ)) J :=
  h.subst hδ.ok

end Flick
