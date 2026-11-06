import Formal.Closed

/-!
# Composition of renamings and substitutions

The four fusion laws of de Bruijn renaming and substitution, each by induction on the term
with the law for the lifted maps (`liftR_comp`, `liftS_rename`, `liftS_rename'`,
`liftS_comp`).
-/

namespace Flick

theorem liftR_comp (n : Nat) (ξ ζ : Nat → Nat) (i : Nat) :
    liftR n ζ (liftR n ξ i) = liftR n (ζ ∘ ξ) i := by
  unfold liftR
  by_cases h : i < n
  · simp [h]
  · simp only [h, if_false, Function.comp]
    rw [if_neg (by omega)]; congr 2; omega

theorem Tm.rename_rename :
    ∀ (e : Tm) (ξ ζ : Nat → Nat), (e.rename ξ).rename ζ = e.rename (ζ ∘ ξ) := by
  have bind : ∀ {n : Nat} {b : Tm} {ξ ζ : Nat → Nat},
      (∀ ξ' ζ', (b.rename ξ').rename ζ' = b.rename (ζ' ∘ ξ')) →
      (b.rename (liftR n ξ)).rename (liftR n ζ) = b.rename (liftR n (ζ ∘ ξ)) := by
    intro n b ξ ζ ih; rw [ih]; congr 1; funext i; exact liftR_comp n ξ ζ i
  intro e
  induction e with
  | var i => intro ξ ζ; rfl
  | lam _ ih | fix _ ih | scope _ ih => intro ξ ζ; simp only [Tm.rename]; rw [bind ih]
  | let_ _ _ ih₁ ih₂ => intro ξ ζ; simp only [Tm.rename]; rw [ih₁, bind ih₂]
  | letrec _ _ ih₁ ih₂ => intro ξ ζ; simp only [Tm.rename]; rw [bind ih₁, bind ih₂]
  | acons _ _ _ ih₁ ih₂ => intro ξ ζ; simp only [Tm.rename]; rw [bind ih₁, ih₂]
  | aguard _ _ _ _ ih₁ ih₂ ih₃ => intro ξ ζ; simp only [Tm.rename]; rw [bind ih₁, bind ih₂, ih₃]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ => intro ξ ζ; simp only [Tm.rename]; rw [ih₁, ih₂]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih
  | drec _ _ ih => intro ξ ζ; simp only [Tm.rename]; rw [ih]
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro ξ ζ; simp only [Tm.rename]; rw [ih₁, ih₂, ih₃]
  | hctl _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro ξ ζ; simp only [Tm.rename]; rw [ih₁, ih₂, ih₃, ih₄]
  | hres _ _ _ ih₁ ih₂ | hab _ _ _ ih₁ ih₂ =>
    intro ξ ζ; simp only [Tm.rename]; rw [ih₁, ih₂]
  | hdone _ _ ih₁ =>
    intro ξ ζ; simp only [Tm.rename]; rw [ih₁]
  | _ => intro ξ ζ; rfl

theorem liftS_rename (n : Nat) (ξ : Nat → Nat) (σ : Nat → Tm) (i : Nat) :
    liftS n σ (liftR n ξ i) = liftS n (σ ∘ ξ) i := by
  unfold liftS liftR
  by_cases h : i < n
  · simp [h]
  · simp only [h, if_false, Function.comp]
    rw [if_neg (by omega)]; congr 2; omega

theorem Tm.subst_rename :
    ∀ (e : Tm) (ξ : Nat → Nat) (σ : Nat → Tm), (e.rename ξ).subst σ = e.subst (σ ∘ ξ) := by
  have bind : ∀ {n : Nat} {b : Tm} {ξ : Nat → Nat} {σ : Nat → Tm},
      (∀ ξ' σ', (b.rename ξ').subst σ' = b.subst (σ' ∘ ξ')) →
      (b.rename (liftR n ξ)).subst (liftS n σ) = b.subst (liftS n (σ ∘ ξ)) := by
    intro n b ξ σ ih; rw [ih]; congr 1; funext i; exact liftS_rename n ξ σ i
  intro e
  induction e with
  | var i => intro ξ σ; rfl
  | lam _ ih | fix _ ih | scope _ ih => intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [bind ih]
  | let_ _ _ ih₁ ih₂ => intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih₁, bind ih₂]
  | letrec _ _ ih₁ ih₂ => intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [bind ih₁, bind ih₂]
  | acons _ _ _ ih₁ ih₂ => intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [bind ih₁, ih₂]
  | aguard _ _ _ _ ih₁ ih₂ ih₃ =>
    intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [bind ih₁, bind ih₂, ih₃]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih₁, ih₂]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih
  | drec _ _ ih => intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih]
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih₁, ih₂, ih₃]
  | hctl _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih₁, ih₂, ih₃, ih₄]
  | hres _ _ _ ih₁ ih₂ | hab _ _ _ ih₁ ih₂ =>
    intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih₁, ih₂]
  | hdone _ _ ih₁ =>
    intro ξ σ; simp only [Tm.rename, Tm.subst]; rw [ih₁]
  | _ => intro ξ σ; rfl

theorem liftS_rename' (n : Nat) (ξ : Nat → Nat) (σ : Nat → Tm) (i : Nat) :
    (liftS n σ i).rename (liftR n ξ) = liftS n (fun j => (σ j).rename ξ) i := by
  unfold liftS
  by_cases h : i < n
  · simp [h, Tm.rename, liftR]
  · simp only [h, if_false, Tm.rename_rename]
    congr 1; funext j; simp only [Function.comp, liftR, if_neg (by omega : ¬ j + n < n), Nat.add_sub_cancel]

theorem Tm.rename_subst :
    ∀ (e : Tm) (σ : Nat → Tm) (ξ : Nat → Nat),
      (e.subst σ).rename ξ = e.subst fun i => (σ i).rename ξ := by
  have bind : ∀ {n : Nat} {b : Tm} {σ : Nat → Tm} {ξ : Nat → Nat},
      (∀ σ' ξ', (b.subst σ').rename ξ' = b.subst fun i => (σ' i).rename ξ') →
      (b.subst (liftS n σ)).rename (liftR n ξ) = b.subst (liftS n fun j => (σ j).rename ξ) := by
    intro n b σ ξ ih; rw [ih]; congr 1; funext i; exact liftS_rename' n ξ σ i
  intro e
  induction e with
  | var i => intro σ ξ; rfl
  | lam _ ih | fix _ ih | scope _ ih => intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [bind ih]
  | let_ _ _ ih₁ ih₂ => intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [ih₁, bind ih₂]
  | letrec _ _ ih₁ ih₂ => intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [bind ih₁, bind ih₂]
  | acons _ _ _ ih₁ ih₂ => intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [bind ih₁, ih₂]
  | aguard _ _ _ _ ih₁ ih₂ ih₃ =>
    intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [bind ih₁, bind ih₂, ih₃]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [ih₁, ih₂]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih
  | drec _ _ ih => intro σ ξ; simp only [Tm.rename, Tm.subst]; rw [ih]
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro σ ξ; simp only [Tm.subst, Tm.rename]; rw [ih₁, ih₂, ih₃]
  | hctl _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro σ ξ; simp only [Tm.subst, Tm.rename]; rw [ih₁, ih₂, ih₃, ih₄]
  | hres _ _ _ ih₁ ih₂ | hab _ _ _ ih₁ ih₂ =>
    intro σ ξ; simp only [Tm.subst, Tm.rename]; rw [ih₁, ih₂]
  | hdone _ _ ih₁ =>
    intro σ ξ; simp only [Tm.subst, Tm.rename]; rw [ih₁]
  | _ => intro σ ξ; rfl

theorem liftS_comp (n : Nat) (σ τ : Nat → Tm) (i : Nat) :
    (liftS n σ i).subst (liftS n τ) = liftS n (fun j => (σ j).subst τ) i := by
  unfold liftS
  by_cases h : i < n
  · simp [h, Tm.subst]
  · simp only [h, if_false, Tm.subst_rename, Tm.rename_subst]
    congr 1; funext j
    simp only [Function.comp, if_neg (by omega : ¬ j + n < n), Nat.add_sub_cancel]

theorem Tm.subst_subst :
    ∀ (e : Tm) (σ τ : Nat → Tm), (e.subst σ).subst τ = e.subst fun i => (σ i).subst τ := by
  have bind : ∀ {n : Nat} {b : Tm} {σ τ : Nat → Tm},
      (∀ σ' τ', (b.subst σ').subst τ' = b.subst fun i => (σ' i).subst τ') →
      (b.subst (liftS n σ)).subst (liftS n τ) = b.subst (liftS n fun j => (σ j).subst τ) := by
    intro n b σ τ ih; rw [ih]; congr 1; funext i; exact liftS_comp n σ τ i
  intro e
  induction e with
  | var i => intro σ τ; rfl
  | lam _ ih | fix _ ih | scope _ ih => intro σ τ; simp only [Tm.subst]; rw [bind ih]
  | let_ _ _ ih₁ ih₂ => intro σ τ; simp only [Tm.subst]; rw [ih₁, bind ih₂]
  | letrec _ _ ih₁ ih₂ => intro σ τ; simp only [Tm.subst]; rw [bind ih₁, bind ih₂]
  | acons _ _ _ ih₁ ih₂ => intro σ τ; simp only [Tm.subst]; rw [bind ih₁, ih₂]
  | aguard _ _ _ _ ih₁ ih₂ ih₃ => intro σ τ; simp only [Tm.subst]; rw [bind ih₁, bind ih₂, ih₃]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ => intro σ τ; simp only [Tm.subst]; rw [ih₁, ih₂]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih
  | drec _ _ ih => intro σ τ; simp only [Tm.subst]; rw [ih]
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro σ τ; simp only [Tm.subst]; rw [ih₁, ih₂, ih₃]
  | hctl _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro σ τ; simp only [Tm.subst]; rw [ih₁, ih₂, ih₃, ih₄]
  | hres _ _ _ ih₁ ih₂ | hab _ _ _ ih₁ ih₂ =>
    intro σ τ; simp only [Tm.subst]; rw [ih₁, ih₂]
  | hdone _ _ ih₁ =>
    intro σ τ; simp only [Tm.subst]; rw [ih₁]
  | _ => intro σ τ; rfl

end Flick
