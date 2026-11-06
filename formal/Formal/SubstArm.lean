import Formal.SubOk

/-!
# Substitution into an arm on a constructor of a GADT

The arm is typed under each unifier that fits it. After a substitution of terms, a
unifier that fits the new arm fits the old one and the terms of the variables the
arm uses, so they are fit replacements under that unifier too. The other variables
go to `todo`, which has every type and changes nothing the arm uses.
-/

namespace Flick

variable {S : Sig}

theorem liftS_has_of {n : Nat} {σ : Nat → Tm} {i : Nat} {s : Site} (h : (σ i).Has s) :
    (liftS n σ (i + n)).Has s := by
  unfold liftS
  rw [if_neg (by omega), Nat.add_sub_cancel]
  exact (Tm.has_rename _ _ _).mpr h

/-- Substitution keeps what a term writes. -/
theorem Tm.has_subst_self :
    ∀ (e : Tm) (σ : Nat → Tm) (s : Site), e.Has s → (e.subst σ).Has s := by
  intro e
  induction e <;> intro σ s h <;> simp only [Tm.subst, Tm.Has] at h ⊢ <;> grind

/-- Substitution brings in what the terms of the free variables write. -/
theorem Tm.has_subst_free :
    ∀ (e : Tm) (σ : Nat → Tm) (s : Site) (i : Nat), e.Free i → (σ i).Has s → (e.subst σ).Has s := by
  have bind : ∀ {n : Nat} {b : Tm} {σ : Nat → Tm} {s : Site} {i : Nat},
      (∀ σ' s' i', b.Free i' → (σ' i').Has s' → (b.subst σ').Has s') →
      b.Free (i + n) → (σ i).Has s → (b.subst (liftS n σ)).Has s :=
    fun ih h hs => ih _ _ _ h (liftS_has_of hs)
  intro e
  induction e with
  | var j => intro σ s i h hs; cases h; exact hs
  | lam b ih | fix b ih | scope b ih => intro σ s i h hs; exact bind ih h hs
  | let_ e b ih₁ ih₂ =>
    intro σ s i h hs
    rcases h with h | h
    · exact .inl (ih₁ σ s i h hs)
    · exact .inr (bind ih₂ h hs)
  | letrec f b ih₁ ih₂ =>
    intro σ s i h hs
    rcases h with h | h
    · exact .inl (bind ih₁ h hs)
    · exact .inr (bind ih₂ h hs)
  | acons p b r ih₁ ih₂ =>
    intro σ s i h hs
    rcases h with h | h
    · exact .inl (bind ih₁ h hs)
    · exact .inr (ih₂ σ s i h hs)
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro σ s i h hs
    rcases h with h | h | h
    · exact .inl (bind ih₁ h hs)
    · exact .inr (.inl (bind ih₂ h hs))
    · exact .inr (.inr (ih₃ σ s i h hs))
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro σ s i h hs
    rcases h with h | h
    · exact .inl (ih₁ σ s i h hs)
    · exact .inr (ih₂ σ s i h hs)
  | with_ _ _ _ ih₁ ih₂ =>
    intro σ s i h hs
    rcases h with h | h
    · exact .inr (.inr (.inl (ih₁ σ s i h hs)))
    · exact .inr (.inr (.inr (ih₂ σ s i h hs)))
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | drec _ _ ih =>
    intro σ s i h hs; exact ih σ s i h hs
  | inside p e ih => intro σ s i h hs; exact .inr (.inr (.inr (ih σ s i h hs)))
  | _ => intro σ s i h hs; exact h.elim

theorem liftS_congr {n : Nat} {σ σ' : Nat → Tm} {b : Tm}
    (h : ∀ i, b.Free (i + n) → σ i = σ' i) : ∀ i, b.Free i → liftS n σ i = liftS n σ' i := by
  intro i hi
  unfold liftS
  split
  · rfl
  · rw [h (i - n) (by rwa [Nat.sub_add_cancel (by omega)])]

/-- A substitution acts only through the free variables. -/
theorem Tm.subst_congr : ∀ (e : Tm) {σ σ' : Nat → Tm}, (∀ i, e.Free i → σ i = σ' i) →
    e.subst σ = e.subst σ' := by
  intro e
  induction e with
  | var j => intro σ σ' h; exact h j rfl
  | lam b ih | fix b ih | scope b ih =>
    intro σ σ' h; simp only [Tm.subst]; rw [ih (liftS_congr fun i hi => h i hi)]
  | let_ e b ih₁ ih₂ =>
    intro σ σ' h; simp only [Tm.subst]
    rw [ih₁ fun i hi => h i (.inl hi), ih₂ (liftS_congr fun i hi => h i (.inr hi))]
  | letrec f b ih₁ ih₂ =>
    intro σ σ' h; simp only [Tm.subst]
    rw [ih₁ (liftS_congr fun i hi => h i (.inl hi)), ih₂ (liftS_congr fun i hi => h i (.inr hi))]
  | acons p b r ih₁ ih₂ =>
    intro σ σ' h; simp only [Tm.subst]
    rw [ih₁ (liftS_congr fun i hi => h i (.inl hi)), ih₂ fun i hi => h i (.inr hi)]
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro σ σ' h; simp only [Tm.subst]
    rw [ih₁ (liftS_congr fun i hi => h i (.inl hi)),
      ih₂ (liftS_congr fun i hi => h i (.inr (.inl hi))), ih₃ fun i hi => h i (.inr (.inr hi))]
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro σ σ' h; simp only [Tm.subst]
    rw [ih₁ fun i hi => h i (.inl hi), ih₂ fun i hi => h i (.inr hi)]
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih | drec _ _ ih =>
    intro σ σ' h; simp only [Tm.subst]; rw [ih h]
  | _ => intro _ _ _; rfl

/-- What a term holds, under a substitution that keeps the regions it writes. -/
theorem held_substS {Ψ : List Ty} {Γ : List Scheme} {e : Tm} {s : Scheme} {σ : Atom → Ty}
    (hσ : Fits σ e) (h : Caps.Sub (held Ψ Γ e) s.reach) :
    Caps.Sub (held (Ψ.map (Ty.substT σ)) (Γ.map (Scheme.substT σ)) e) (s.substT σ).reach := by
  rintro a (⟨i, s', hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at hs
    obtain ⟨s₀, hs₀, rfl⟩ := hs
    obtain ⟨b, hb, h'⟩ := (Scheme.reach_substT _ _ _).mp ha
    exact (Scheme.reach_substT _ _ _).mpr ⟨b, h b (.inl ⟨i, s₀, hi, hs₀, hb⟩), h'⟩
  · refine (Scheme.reach_substT _ _ _).mpr ⟨.r p, h _ (.inr (.inl ⟨p, hp, rfl⟩)), ?_⟩
    rw [hσ.1 p (.inl hp)]; rfl
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at ht
    obtain ⟨t₀, ht₀, rfl⟩ := ht
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ _).mp ha
    exact (Scheme.reach_substT _ _ _).mpr ⟨b, h b (.inr (.inr ⟨l, t₀, hl, ht₀, hb⟩)), h'⟩

open Classical in
/-- `σ` on the variables `P` picks and `todo` on the others is a fit replacement. -/
theorem SubOk.restrict {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (P : Nat → Prop) :
    SubOk S Ψ Θ Γ Γ' fun i => if P i then σ i else .todo := by
  intro i s hi
  by_cases hP : P i
  · simpa [hP] using h i s hi
  · simp only [hP, if_false]
    refine ⟨.todo, fun h => False.elim h, ?_⟩
    rintro a (⟨j, _, hj, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · exact False.elim hj
    · exact False.elim hp
    · exact False.elim hl

/-- A fit replacement whose terms fit `θ` is one under `θ`. -/
theorem SubOk.substT {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (θ : Atom → Ty)
    (hθ : ∀ i, Fits θ (σ i)) :
    SubOk S (Ψ.map (Ty.substT θ)) (Θ.substT θ) (Γ.map (Scheme.substT θ))
      (Γ'.map (Scheme.substT θ)) σ := by
  intro i s hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
  obtain ⟨s₀, hs₀, rfl⟩ := hi
  obtain ⟨ht, hnI, hc⟩ := h _ _ hs₀
  have hf : Fits (liftT s₀.k θ) (σ i) :=
    ⟨fun p hp => by simp [liftT, (hθ i).1 p hp, Ty.up, Ty.substT, shiftV],
      fun hi => (hnI hi).elim, fun hi => (hnI hi).elim, fun hi => (hnI hi).elim⟩
  have := ht.substT (liftT s₀.k θ) hf
  simp only [Flick.J.substT] at this
  rw [shift_comm, shift_comm'] at this
  have e₃ : (Θ.at s₀).substT (liftT s₀.k θ) = (Θ.substT θ).at (s₀.substT θ) := by
    simp only [Facts.at, Facts.union_substT, Facts.up_comm]; rfl
  rw [e₃] at this
  exact ⟨this, hnI, held_substS (hθ i) hc⟩

theorem SubOk.frees {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) {e : Tm}
    (hf : ∀ i, e.Free i → i < Γ.length) : ∀ j, (e.subst σ).Free j → j < Γ'.length := by
  intro j hj
  obtain ⟨i, hi, hj'⟩ := Tm.free_subst e σ j hj
  simpa using (h i _ (List.getElem?_eq_getElem (hf i hi))).1.frees j hj'

theorem SubOk.locs {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) {e : Tm}
    (hf : ∀ i, e.Free i → i < Γ.length) (hl : ∀ l, e.Has (.loc l) → l < Ψ.length) :
    ∀ l, (e.subst σ).Has (.loc l) → l < Ψ.length := by
  intro l h'
  rcases Tm.has_subst e σ _ h' with h' | ⟨i, hi, h'⟩
  · exact hl l h'
  · simpa using (h i _ (List.getElem?_eq_getElem (hf i hi))).1.locs l h'

open Classical in
/-- An arm on a constructor of a GADT after a substitution of terms. -/
theorem Typ.aref_subst {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {σ : Nat → Tm}
    {K : Name} {k : Nat} {ts : List Ty} {T₀ : Name} {qs : List Pat} {b r : Tm} {t t' : Ty}
    {φ : Stage} {Δ : (Atom → Ty) → List Ty} (hK : S.ctor K = some (k, ts, T₀))
    (hU : t.hd = .adt T₀) (hσ : SubOk S Ψ Θ Γ Γ' σ) (hn : b.NoIns)
    (hfr : ∀ i, b.Free i → i < (Pat.spine K qs).binds + Γ.length)
    (hlc : ∀ l, b.Has (.loc l) → l < Ψ.length)
    (hpA : ∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ = (t.up k).substT θ →
      PatsTy S qs (ts.map (Ty.substT θ)) (Δ θ))
    (ihA : ∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ = (t.up k).substT θ →
      ∀ {Γ'' σ''}, SubOk S ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
        ((Δ θ).map .mono ++ (Γ.map (Scheme.up k)).map (Scheme.substT θ)) Γ'' σ'' →
      Typ S ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ) Γ''
        (b.subst σ'') (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ)))
    (hr : Typ S Ψ Θ Γ' (r.subst σ) (.alts t t' φ)) :
    Typ S Ψ Θ Γ' ((Tm.acons (Pat.spine K qs) b r).subst σ) (.alts t t' φ) := by
  have hl := hσ.lift (List.replicate (Pat.spine K qs).binds (Scheme.mono .rnil))
    (n := (Pat.spine K qs).binds) (by simp)
  have hfr' : ∀ i, b.Free i → i < (List.replicate (Pat.spine K qs).binds
      (Scheme.mono .rnil) ++ Γ).length := fun i h => by simpa using hfr i h
  have sites : ∀ θ, Fits θ (b.subst (liftS (Pat.spine K qs).binds σ)) → Fits θ b :=
    fun θ hθ => hθ.sub fun s h => Tm.has_subst_self b _ s h
  refine .aref hK hU (hl.noIns hfr' hn) (fun i h => by simpa using hl.frees hfr' i h)
    (hl.locs hfr' hlc) (fun θ hθ hu => hpA θ (sites θ hθ) hu) (fun θ hθ hu => ?_) hr
  have hθb := sites θ hθ
  have hfit : ∀ i, Fits θ (if b.Free (i + (Pat.spine K qs).binds) then σ i else .todo) := by
    intro i
    by_cases hP : b.Free (i + (Pat.spine K qs).binds)
    · rw [if_pos hP]
      exact hθ.sub fun s h => Tm.has_subst_free b _ s _ hP (liftS_has_of h)
    · rw [if_neg hP]
      exact ⟨fun _ hp => hp.elim False.elim False.elim, fun h => False.elim h,
        fun h => False.elim h, fun h => False.elim h⟩
  have h₁ := ((hσ.restrict fun i => b.Free (i + (Pat.spine K qs).binds)).up k Facts.none).mono
    (Θ' := Θ.substT (shiftV k)) fun C s h => h.mono fun _ _ h => h.elim id False.elim
  have h₂ := (h₁.substT θ hfit).lift ((Δ θ).map Scheme.mono) ((hpA θ hθb hu).spine_binds (K := K))
  have e : b.subst (liftS (Pat.spine K qs).binds
      fun i => if b.Free (i + (Pat.spine K qs).binds) then σ i else .todo) =
      b.subst (liftS (Pat.spine K qs).binds σ) :=
    Tm.subst_congr b (liftS_congr fun i hi => by rw [if_pos hi])
  rw [← e]; exact ihA θ hθb hu h₂

end Flick
