import Formal.TypeEquiv

/-!
# Substitution of facts, `Outlives`, the side conditions of constants and schemes
-/

namespace Flick

theorem Facts.substT_comp (Θ : Facts) (σ σ' : Atom → Ty) :
    (Θ.substT σ).substT σ' = Θ.substT fun a => (σ a).substT σ' := by
  funext C s; apply propext
  simp only [Facts.substT]
  constructor
  · rintro ⟨C₁, s₁, ⟨C₀, s₀, h, rfl, rfl⟩, rfl, rfl⟩
    exact ⟨C₀, s₀, h, Caps.substT_comp _ _ _, Ty.substT_comp _ _ _⟩
  · rintro ⟨C₀, s₀, h, rfl, rfl⟩
    exact ⟨_, _, ⟨C₀, s₀, h, rfl, rfl⟩, (Caps.substT_comp _ _ _).symm, (Ty.substT_comp _ _ _).symm⟩

theorem Facts.substT_ident (Θ : Facts) : Θ.substT Ty.ident = Θ := by
  funext C s; apply propext
  simp only [Facts.substT]
  constructor
  · rintro ⟨C₀, s₀, h, rfl, rfl⟩; rwa [Caps.substT_ident, Ty.substT_ident]
  · intro h; exact ⟨C, s, h, (Caps.substT_ident C).symm, (Ty.substT_ident s).symm⟩

@[simp] theorem Facts.union_substT (Θ Θ' : Facts) (σ : Atom → Ty) :
    (Facts.union Θ Θ').substT σ = Facts.union (Θ.substT σ) (Θ'.substT σ) := by
  funext C s; apply propext
  simp only [Facts.substT, Facts.union]
  constructor
  · rintro ⟨C₀, s₀, h | h, rfl, rfl⟩
    · exact .inl ⟨C₀, s₀, h, rfl, rfl⟩
    · exact .inr ⟨C₀, s₀, h, rfl, rfl⟩
  · rintro (⟨C₀, s₀, h, rfl, rfl⟩ | ⟨C₀, s₀, h, rfl, rfl⟩)
    · exact ⟨C₀, s₀, .inl h, rfl, rfl⟩
    · exact ⟨C₀, s₀, .inr h, rfl, rfl⟩

@[simp] theorem Facts.none_substT (σ : Atom → Ty) : Facts.none.substT σ = Facts.none := by
  funext C s; simp [Facts.substT, Facts.none]

@[simp] theorem Facts.close_none : Facts.none.close = Facts.none := by
  funext C s; apply propext
  constructor
  · rintro ⟨s₀, h, _⟩; exact h
  · intro h; exact h.elim

@[simp] theorem Facts.close_union (Θ Φ : Facts) :
    (Facts.union Θ Φ).close = Facts.union Θ.close Φ.close := by
  funext C s; apply propext
  constructor
  · rintro ⟨s₀, h | h, he⟩
    · exact .inl ⟨s₀, h, he⟩
    · exact .inr ⟨s₀, h, he⟩
  · rintro (⟨s₀, h, he⟩ | ⟨s₀, h, he⟩)
    · exact ⟨s₀, .inl h, he⟩
    · exact ⟨s₀, .inr h, he⟩

theorem Facts.close_idem (Θ : Facts) : Θ.close.close = Θ.close := by
  funext C s; apply propext
  constructor
  · rintro ⟨s₀, ⟨s₁, h, he₁⟩, he₀⟩; exact ⟨s₁, h, he₁.trans he₀⟩
  · intro h; exact ⟨s, h, .refl⟩

/-- A declared fact entails its outlives requirement. -/
theorem Outl.fact {Θ C s} (h : Θ C s) : Outl Θ C s :=
  fun _ hk => .inr ⟨C, ⟨s, h, .refl⟩, hk⟩

/-- `Outlives` is kept by a substitution. -/
theorem Outl.substT {Θ C s} (σ : Atom → Ty) (h : Outl Θ C s) :
    Outl (Θ.substT σ) (C.substT σ) (s.substT σ) := by
  intro k ⟨b, hb, hk⟩
  rcases h b hb with h | ⟨C', h₁, h₂⟩
  · exact .inl ((Ty.reach_substT σ s k).mpr ⟨b, h, hk⟩)
  · obtain ⟨s₀, h₀, he⟩ := h₁
    exact .inr ⟨C'.substT σ, ⟨s₀.substT σ, ⟨C', s₀, h₀, rfl, rfl⟩, he.substT σ⟩, b, h₂, hk⟩

theorem Outl.mono {Θ Θ' C s} (h : Outl Θ C s) (hΘ : ∀ C s, Θ C s → Θ' C s) : Outl Θ' C s :=
  fun k hk => (h k hk).imp id fun ⟨C', ⟨s₀, h₁, he⟩, h₂⟩ => ⟨C', ⟨s₀, hΘ _ _ h₁, he⟩, h₂⟩

/-- Congruent type expressions name the same outlives requirement. -/
theorem Outl.equiv {Θ C a b} (h : Outl Θ C a) (he : Ty.Equiv a b) : Outl Θ C b := by
  intro k hk
  rcases h k hk with hr | ⟨D, ⟨s₀, hD, hs⟩, hK⟩
  · exact .inl (by rwa [← he.reach])
  · exact .inr ⟨D, ⟨s₀, hD, hs.trans he⟩, hK⟩

/-- `Outlives C D` for a capture set `D` is `C ⊆ D` up to the facts. -/
theorem Outl.caps_iff {Θ C D} : Outl Θ C (.caps D) ↔ ∀ k, C k → D k ∨ ∃ C', Θ.close C' (.caps D) ∧ C' k :=
  Iff.rfl

theorem Outl.caps_of_sub {Θ C D} (h : ∀ k, C k → D k) : Outl Θ C (.caps D) :=
  fun k hk => .inl (h k hk)

/-- The value bound `αᵢ^{≤D}` of a scheme (regions.md §2.1): the fact `Outlives αᵢ D`, so
(Inst) asks `⌈τᵢ⌉ ⊆ D[ᾱ := τ̄]` of every instance, and the body of the scheme may assume it. -/
def Facts.bnd (i : Nat) (D : Caps) : Facts := fun C s => C = (· = .v i) ∧ s = .caps D

def Scheme.substT (σ : Atom → Ty) (s : Scheme) : Scheme :=
  ⟨s.k, s.cs.substT (liftT s.k σ), s.t.substT (liftT s.k σ)⟩

def Scheme.up (k : Nat) (s : Scheme) : Scheme := s.substT (shiftV k)

/-- `σ[ᾱ := τ̄]`. -/
def Scheme.inst (s : Scheme) (ts : List Ty) : Ty := s.t.substT (instT ts)

/-- The facts of a scheme at an instance hold. -/
def Scheme.Holds (Θ : Facts) (s : Scheme) (ts : List Ty) : Prop :=
  ∀ C r, s.cs C r → Outl Θ (C.substT (instT ts)) (r.substT (instT ts))

/-- The facts of a scheme with one value bound hold at an instance iff the instance of the
variable reaches no more than the instance of the bound. -/
theorem Scheme.holds_bnd {Θ k i D t ts} :
    (⟨k, Facts.bnd i D, t⟩ : Scheme).Holds Θ ts ↔
      Outl Θ (instT ts (.v i)).reach (.caps (D.substT (instT ts))) := by
  have hC : Caps.substT (instT ts) (· = .v i) = (instT ts (.v i)).reach := by
    funext a; apply propext
    exact ⟨fun ⟨_, h, h'⟩ => h ▸ h', fun h => ⟨_, rfl, h⟩⟩
  constructor
  · intro h; have := h _ _ ⟨rfl, rfl⟩; rwa [hC] at this
  · rintro h C r ⟨rfl, rfl⟩; rw [hC]; exact h

theorem Scheme.substT_comp (s : Scheme) (σ σ' : Atom → Ty) :
    (s.substT σ).substT σ' = s.substT fun a => (σ a).substT σ' := by
  simp only [Scheme.substT, Ty.substT_comp, Facts.substT_comp, Scheme.mk.injEq, true_and]
  constructor <;> congr 1 <;> funext a <;> exact liftT_comp _ _ _ _

theorem Scheme.substT_ident (s : Scheme) : s.substT Ty.ident = s := by
  simp [Scheme.substT, liftT_ident, Ty.substT_ident, Facts.substT_ident]

theorem Scheme.substT_congr {s : Scheme} {σ σ' : Atom → Ty} (h : ∀ a, σ a = σ' a) :
    s.substT σ = s.substT σ' := by
  rw [funext h]

@[simp] theorem Scheme.up_zero (s : Scheme) : s.up 0 = s := by
  have : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
  simp only [Scheme.up, this]; exact s.substT_ident

theorem Scheme.up_comm (s : Scheme) (k : Nat) (σ : Atom → Ty) :
    (s.up k).substT (liftT k σ) = (s.substT σ).up k := by
  simp only [Scheme.up, Scheme.substT_comp]
  apply Scheme.substT_congr
  intro a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, liftT, Ty.up_def, this]
  | r p => simp [shiftV, Ty.substT, liftT, Ty.up_def]

theorem Scheme.up_substT (s : Scheme) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (s.up k).substT (instT ts) = s := by
  simp only [Scheme.up, Scheme.substT_comp]
  conv => rhs; rw [← s.substT_ident]
  apply Scheme.substT_congr
  intro a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, instT, h, this, Ty.ident]
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

@[simp] theorem List.map_sup_zero (Γ : List Scheme) : Γ.map (Scheme.up 0) = Γ := by
  induction Γ <;> simp_all

@[simp] theorem Scheme.mono_k (t : Ty) : (Scheme.mono t).k = 0 := rfl
@[simp] theorem Scheme.mono_t (t : Ty) : (Scheme.mono t).t = t := rfl
@[simp] theorem Scheme.mono_cs (t : Ty) : (Scheme.mono t).cs = Facts.none := rfl

@[simp] theorem Scheme.mono_substT (t : Ty) (σ : Atom → Ty) :
    (Scheme.mono t).substT σ = Scheme.mono (t.substT σ) := by
  simp [Scheme.substT, Scheme.mono]

@[simp] theorem Scheme.inst_mono (t : Ty) : (Scheme.mono t).inst [] = t := by
  simp only [Scheme.inst, Scheme.mono]
  conv => rhs; rw [← t.substT_ident]
  apply Ty.substT_congr; intro a; cases a <;> simp [instT, Ty.ident]

theorem Scheme.inst_substT (s : Scheme) (σ : Atom → Ty) (ts : List Ty) (h : ts.length = s.k) :
    (s.substT σ).inst (ts.map (Ty.substT σ)) = (s.inst ts).substT σ := by
  simp only [Scheme.substT, Scheme.inst]
  rw [← h]; exact Ty.inst_substT _ _ _

theorem Caps.inst_substT (C : Caps) (σ : Atom → Ty) (ts : List Ty) :
    (C.substT (liftT ts.length σ)).substT (instT (ts.map (Ty.substT σ))) =
      (C.substT (instT ts)).substT σ := by
  rw [Caps.substT_comp, Caps.substT_comp]
  congr 1; funext a
  have := Ty.inst_substT (Ty.ident a) σ ts
  cases a <;> simpa [Ty.substT, Ty.ident] using this

theorem Scheme.Holds.substT {Θ : Facts} {s : Scheme} {ts : List Ty} (σ : Atom → Ty) (h : s.Holds Θ ts) (hk : ts.length = s.k) :
    (s.substT σ).Holds (Θ.substT σ) (ts.map (Ty.substT σ)) := by
  intro C r ⟨C₀, r₀, h₀, hC, hr⟩
  subst hC hr
  have := (h C₀ r₀ h₀).substT σ
  rw [← hk, Caps.inst_substT, Ty.inst_substT]
  exact this

end Flick
