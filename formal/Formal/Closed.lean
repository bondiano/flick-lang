import Formal.Reach

/-!
# Renaming and substitution that fix the free variables

A renaming or a substitution that is the identity on the free variables of a term leaves it
unchanged; in particular a closed term is unchanged by any of them.
-/

namespace Flick

theorem liftR_fix {n : Nat} {ξ : Nat → Nat} {b : Tm} (h : ∀ i, b.Free (i + n) → ξ i = i) :
    ∀ j, b.Free j → liftR n ξ j = j := by
  intro j hj
  unfold liftR
  split
  · rfl
  · rw [h (j - n) (by rwa [Nat.sub_add_cancel (by omega)])]; omega

theorem liftS_fix {n : Nat} {σ : Nat → Tm} {b : Tm} (h : ∀ i, b.Free (i + n) → σ i = .var i) :
    ∀ j, b.Free j → liftS n σ j = .var j := by
  intro j hj
  unfold liftS
  split
  · rfl
  · rw [h (j - n) (by rwa [Nat.sub_add_cancel (by omega)])]
    simp only [Tm.rename]; congr 1; omega

theorem Tm.rename_fix :
    ∀ (e : Tm) (ξ : Nat → Nat), (∀ i, e.Free i → ξ i = i) → e.rename ξ = e := by
  have bind : ∀ {n : Nat} {b : Tm} {ξ : Nat → Nat},
      (∀ ξ', (∀ i, b.Free i → ξ' i = i) → b.rename ξ' = b) →
      (∀ i, b.Free (i + n) → ξ i = i) → b.rename (liftR n ξ) = b :=
    fun ih h => ih _ (liftR_fix h)
  intro e
  induction e with
  | var i => intro ξ h; simp [Tm.rename, h i rfl]
  | lam b ih => intro ξ h; simp only [Tm.rename]; rw [bind ih (by simpa using h)]
  | fix f ih => intro ξ h; simp only [Tm.rename]; rw [bind ih (by simpa using h)]
  | scope e ih => intro ξ h; simp only [Tm.rename]; rw [bind ih (by simpa using h)]
  | let_ e b ih₁ ih₂ =>
    intro ξ h; simp only [Tm.rename]
    rw [ih₁ ξ fun i hi => h i (.inl hi), bind ih₂ fun i hi => h i (.inr hi)]
  | letrec f b ih₁ ih₂ =>
    intro ξ h; simp only [Tm.rename]
    rw [bind ih₁ fun i hi => h i (.inl hi), bind ih₂ fun i hi => h i (.inr hi)]
  | acons p b r ih₁ ih₂ =>
    intro ξ h; simp only [Tm.rename]
    rw [bind ih₁ fun i hi => h i (.inl hi), ih₂ ξ fun i hi => h i (.inr hi)]
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro ξ h; simp only [Tm.rename]
    rw [bind ih₁ fun i hi => h i (.inl hi), bind ih₂ fun i hi => h i (.inr (.inl hi)),
      ih₃ ξ fun i hi => h i (.inr (.inr hi))]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro ξ h; simp only [Tm.rename]
    rw [ih₁ ξ fun i hi => h i (.inl hi), ih₂ ξ fun i hi => h i (.inr hi)]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih
  | drec _ _ ih => intro ξ h; simp only [Tm.rename]; rw [ih ξ h]
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro ξ h; simp only [Tm.rename]
    rw [ih₁ ξ fun i hi => h i (.inl hi), ih₂ ξ fun i hi => h i (.inr (.inl hi)), ih₃ ξ fun i hi => h i (.inr (.inr hi))]
  | hctl _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro ξ h; simp only [Tm.rename]
    rw [ih₁ ξ fun i hi => h i (.inl hi), ih₂ ξ fun i hi => h i (.inr (.inl hi)), ih₃ ξ fun i hi => h i (.inr (.inr (.inl hi))), ih₄ ξ fun i hi => h i (.inr (.inr (.inr hi)))]
  | hres _ _ _ ih₁ ih₂ | hab _ _ _ ih₁ ih₂ =>
    intro ξ h; simp only [Tm.rename]
    rw [ih₁ ξ fun i hi => h i (.inl hi), ih₂ ξ fun i hi => h i (.inr hi)]
  | hdone _ _ ih₁ =>
    intro ξ h; simp only [Tm.rename]
    rw [ih₁ ξ fun i hi => h i hi]
  | _ => intro ξ _; rfl

theorem Tm.subst_fix :
    ∀ (e : Tm) (σ : Nat → Tm), (∀ i, e.Free i → σ i = .var i) → e.subst σ = e := by
  have bind : ∀ {n : Nat} {b : Tm} {σ : Nat → Tm},
      (∀ σ', (∀ i, b.Free i → σ' i = .var i) → b.subst σ' = b) →
      (∀ i, b.Free (i + n) → σ i = .var i) → b.subst (liftS n σ) = b :=
    fun ih h => ih _ (liftS_fix h)
  intro e
  induction e with
  | var i => intro σ h; simp [Tm.subst, h i rfl]
  | lam b ih => intro σ h; simp only [Tm.subst]; rw [bind ih (by simpa using h)]
  | fix f ih => intro σ h; simp only [Tm.subst]; rw [bind ih (by simpa using h)]
  | scope e ih => intro σ h; simp only [Tm.subst]; rw [bind ih (by simpa using h)]
  | let_ e b ih₁ ih₂ =>
    intro σ h; simp only [Tm.subst]
    rw [ih₁ σ fun i hi => h i (.inl hi), bind ih₂ fun i hi => h i (.inr hi)]
  | letrec f b ih₁ ih₂ =>
    intro σ h; simp only [Tm.subst]
    rw [bind ih₁ fun i hi => h i (.inl hi), bind ih₂ fun i hi => h i (.inr hi)]
  | acons p b r ih₁ ih₂ =>
    intro σ h; simp only [Tm.subst]
    rw [bind ih₁ fun i hi => h i (.inl hi), ih₂ σ fun i hi => h i (.inr hi)]
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro σ h; simp only [Tm.subst]
    rw [bind ih₁ fun i hi => h i (.inl hi), bind ih₂ fun i hi => h i (.inr (.inl hi)),
      ih₃ σ fun i hi => h i (.inr (.inr hi))]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro σ h; simp only [Tm.subst]
    rw [ih₁ σ fun i hi => h i (.inl hi), ih₂ σ fun i hi => h i (.inr hi)]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih
  | drec _ _ ih => intro σ h; simp only [Tm.subst]; rw [ih σ h]
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro σ h; simp only [Tm.subst]
    rw [ih₁ σ fun i hi => h i (.inl hi), ih₂ σ fun i hi => h i (.inr (.inl hi)), ih₃ σ fun i hi => h i (.inr (.inr hi))]
  | hctl _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro σ h; simp only [Tm.subst]
    rw [ih₁ σ fun i hi => h i (.inl hi), ih₂ σ fun i hi => h i (.inr (.inl hi)), ih₃ σ fun i hi => h i (.inr (.inr (.inl hi))), ih₄ σ fun i hi => h i (.inr (.inr (.inr hi)))]
  | hres _ _ _ ih₁ ih₂ | hab _ _ _ ih₁ ih₂ =>
    intro σ h; simp only [Tm.subst]
    rw [ih₁ σ fun i hi => h i (.inl hi), ih₂ σ fun i hi => h i (.inr hi)]
  | hdone _ _ ih₁ =>
    intro σ h; simp only [Tm.subst]
    rw [ih₁ σ fun i hi => h i hi]
  | _ => intro σ _; rfl

/-- A closed term: no free variables. -/
def Tm.Closed (e : Tm) : Prop := ∀ i, ¬ e.Free i

theorem Tm.Closed.rename {e : Tm} (h : e.Closed) (ξ : Nat → Nat) : e.rename ξ = e :=
  e.rename_fix ξ fun i hi => (h i hi).elim

theorem Tm.Closed.subst {e : Tm} (h : e.Closed) (σ : Nat → Tm) : e.subst σ = e :=
  e.subst_fix σ fun i hi => (h i hi).elim

end Flick
