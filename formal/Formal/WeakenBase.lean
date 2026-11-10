import Formal.Covers
import Formal.Closed

/-!
# What patterns bind, and cells of a store typing
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

theorem PatTy.binds {fl p t Δ} (h : PatTy S fl p t Δ) : Δ.length = p.binds := by
  induction h <;> simp_all [Pat.binds]

theorem Pat.spine_binds (K : Name) (qs : List Pat) :
    (Pat.spine K qs).binds = (qs.map Pat.binds).sum := by
  have key : ∀ (qs : List Pat) (p : Pat), (qs.foldl .app p).binds = p.binds + (qs.map Pat.binds).sum := by
    intro qs
    induction qs with
    | nil => intro p; simp
    | cons q qs ih => intro p; simp only [List.foldl, ih, Pat.binds, List.map_cons, List.sum_cons]; omega
  simpa [Pat.spine, Pat.binds] using key qs (.ctor K)

theorem PatsTy.binds {qs us Δ} (h : PatsTy S qs us Δ) : Δ.length = (qs.map Pat.binds).sum := by
  induction h with
  | nil => rfl
  | cons hq _ ih => simp [hq.binds, ih]

/-- The arm of a GADT binds what its patterns bind, under any instance. -/
theorem PatsTy.spine_binds {K qs us Δ} (h : PatsTy S qs us Δ) :
    (Δ.map Scheme.mono).length = (Pat.spine K qs).binds := by
  simp [h.binds, Pat.spine_binds]

theorem getElem?_lt {α} {l : List α} {i : Nat} {a : α} (h : l[i]? = some a) : i < l.length := by
  obtain ⟨hi, _⟩ := List.getElem?_eq_some_iff.mp h
  exact hi

theorem getElem?_ext {α} {l l' : List α} {i : Nat} {a : α} (h : l[i]? = some a) :
    (l ++ l')[i]? = some a := by
  rw [List.getElem?_append_left (getElem?_lt h)]; exact h

/-- A typed term names only cells of its store typing. -/
theorem TypM.locs {Ψ Θ Γ e J} (h : TypM S mode Ψ Θ Γ e J) : ∀ l, e.Has (.loc l) → l < Ψ.length := by
  induction h with
  | loc hl => intro l h; cases h; exact getElem?_lt hl
  | letGen _ _ ih₁ ih₂ =>
    intro l h; rcases h with h | h
    · simpa using ih₁ l h
    · exact ih₂ l h
  | scope _ _ _ _ _ _ ih => intro l h; simpa using ih l h
  | aref _ _ _ _ hlc _ _ _ _ ih => intro l h; exact h.elim (hlc l) (ih l)
  | hctl _ _ _ _ _ _ _ _ hk _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro l h; subst hk
    simp only [Tm.Has] at h
    rcases h with h | h | h | h | h | h
    · cases h
    · cases h
    · exact ih₁ l h
    · exact ih₂ l h
    · exact ih₄ l h
    · rcases plug_has_iff.mp h with h | h
      · exact ih₃ l (plug_has_iff.mpr (.inl h))
      · simp [Tm.Has] at h
  | hkarm _ _ _ _ hk _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro l h; subst hk
    simp only [Tm.Has] at h
    rcases h with h | h | h | h | h
    · cases h
    · cases h
    · exact ih₁ l h
    · exact ih₃ l h
    · rcases plug_has_iff.mp h with h | h
      · exact ih₂ l (plug_has_iff.mpr (.inl h))
      · simp [Tm.Has] at h
  | hres _ _ _ ha _ _ _ _ _ ih₁ ih₂ =>
    intro l h; subst ha
    simp only [Tm.Has] at h
    rcases h with h | h | h | h
    · cases h
    · cases h
    · rcases plug_has_iff.mp h with h | h
      · exact ih₂ l (plug_has_iff.mpr (.inl h))
      · simp [Tm.Has] at h
    · exact ih₁ l h
  | _ => intro l h; (try simp only [Tm.Has] at h) <;> grind

theorem Typ.locs {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : ∀ l, e.Has (.loc l) → l < Ψ.length :=
  TypM.locs h

theorem HPat.binds {ε₀ p θ Δ} (h : HPat S ε₀ p θ Δ) : Δ.length = p.binds := by
  cases h with
  | wild => rfl
  | var => rfl
  | ctor hp => exact hp.binds


/-- A typed term names only variables of its context. -/
theorem TypM.frees {Ψ Θ Γ e J} (h : TypM S mode Ψ Θ Γ e J) : ∀ i, e.Free i → i < Γ.length := by
  induction h with
  | var => intro i h; cases h; exact getElem?_lt ‹_›
  | allVar hi => intro i h; cases h; exact getElem?_lt hi
  | allI _ _ ih => intro i h; simpa using ih i h
  | lam _ _ _ _ ih => intro i h; have := ih _ h; simp at this; omega
  | let_ _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · exact ih₁ i h
    · have := ih₂ _ h; simp at this; omega
  | letGen _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · simpa using ih₁ i h
    · have := ih₂ _ h; simp at this; omega
  | letrec _ _ _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · have := ih₁ _ h; simp at this; omega
    · have := ih₂ _ h; simp at this; omega
  | fix _ _ _ _ ih => intro i h; have := ih _ h; simp at this; omega
  | task _ _ _ _ _ ih => intro i h; exact ih i h
  | scope _ _ _ _ _ _ ih => intro i h; have := ih _ h; simp at this; omega
  | acons hp _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · have := ih₁ _ h; simp [hp.binds] at this; omega
    · exact ih₂ i h
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro i h; rcases h with h | h | h
    · have := ih₁ _ h; simp [hp.binds] at this; omega
    · have := ih₂ _ h; simp [hp.binds] at this; omega
    · exact ih₃ i h
  | aref _ _ _ hfr _ _ _ _ _ ih₂ =>
    intro i h; rcases h with h | h
    · have := hfr _ h; omega
    · exact ih₂ i h
  | hcons hp _ _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · have := ih₁ _ h; simp [hp.binds] at this; omega
    · exact ih₂ i h
  | hguard hp _ _ _ _ ih₁ ih₂ ih₃ =>
    intro i h; rcases h with h | h | h
    · have := ih₁ _ h; simp [hp.binds] at this; omega
    · have := ih₂ _ h; simp [hp.binds] at this; omega
    · exact ih₃ i h
  | hatail _ _ _ ih₁ ih₂ | haabort _ _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · have := ih₁ _ h; simp [Pat.spine_binds, Pat.binds] at this; omega
    · exact ih₂ i h
  | hactl _ _ _ ih₁ ih₂ =>
    intro i h; rcases h with h | h
    · have := ih₁ _ h; simp [Pat.spine_binds, Pat.binds] at this; omega
    · exact ih₂ i h
  | hctl _ _ _ _ _ _ _ _ hk _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro i h; subst hk
    rcases h with h | h | h | h
    · exact ih₁ i h
    · exact ih₂ i h
    · exact ih₄ i h
    · exact absurd (ih₃ i (plug_free_of_hole h)) (Nat.not_lt_zero _)
  | hkarm _ _ _ _ hk _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro i h; subst hk
    rcases h with h | h | h
    · exact ih₁ i h
    · exact ih₃ i h
    · exact absurd (ih₂ i (plug_free_of_hole h)) (Nat.not_lt_zero _)
  | hres _ _ _ ha _ _ _ _ _ ih₁ ih₂ =>
    intro i h; subst ha
    rcases h with h | h
    · exact absurd (ih₂ i (plug_free_of_hole h)) (Nat.not_lt_zero _)
    · exact ih₁ i h
  | _ => intro i h; (try simp only [Tm.Free] at h) <;> grind

theorem Typ.frees {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : ∀ i, e.Free i → i < Γ.length :=
  TypM.frees h

end Flick
