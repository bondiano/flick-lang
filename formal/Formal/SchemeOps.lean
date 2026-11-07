import Formal.TyOps

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

/-- `Outlives` is kept by a substitution. -/
theorem Outl.substT {Θ C s} (σ : Atom → Ty) (h : Outl Θ C s) :
    Outl (Θ.substT σ) (C.substT σ) (s.substT σ) := by
  intro k ⟨b, hb, hk⟩
  rcases h b hb with h | ⟨C', h₁, h₂⟩
  · exact .inl ((Ty.reach_substT σ s k).mpr ⟨b, h, hk⟩)
  · exact .inr ⟨C'.substT σ, ⟨C', s, h₁, rfl, rfl⟩, b, h₂, hk⟩

theorem Outl.mono {Θ Θ' C s} (h : Outl Θ C s) (hΘ : ∀ C s, Θ C s → Θ' C s) : Outl Θ' C s :=
  fun k hk => (h k hk).imp id fun ⟨C', h₁, h₂⟩ => ⟨C', hΘ _ _ h₁, h₂⟩

theorem Const.ok_substT {Θ c ι} (σ : Atom → Ty) (h : Const.ok Θ c ι) :
    Const.ok (Θ.substT σ) c (ι.substT σ) := by
  cases c <;> simp only [Const.ok, CArg.substT] at h ⊢
  case addFin => exact h.substT σ
  case spawn =>
    have := h.substT σ
    refine fun k hk => this k ?_
    rcases hk with ⟨b, hb, h'⟩ | hk
    · exact ⟨b, .inl hb, h'⟩
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ k).mp hk
      exact ⟨b, .inr hb, h'⟩

theorem Const.ok_mono {Θ Θ' c ι} (h : Const.ok Θ c ι)
    (hΘ : ∀ C s, Outl Θ C s → Outl Θ' C s) : Const.ok Θ' c ι := by
  cases c <;> simp only [Const.ok] at h ⊢ <;> first | exact h | exact hΘ _ _ h

def Scheme.substT (σ : Atom → Ty) (s : Scheme) : Scheme :=
  ⟨s.k, s.cs.substT (liftT s.k σ), s.t.substT (liftT s.k σ)⟩

def Scheme.up (k : Nat) (s : Scheme) : Scheme := s.substT (shiftV k)

/-- `σ[ᾱ := τ̄]`. -/
def Scheme.inst (s : Scheme) (ts : List Ty) : Ty := s.t.substT (instT ts)

/-- The facts of a scheme at an instance hold. -/
def Scheme.Holds (Θ : Facts) (s : Scheme) (ts : List Ty) : Prop :=
  ∀ C r, s.cs C r → Outl Θ (C.substT (instT ts)) (r.substT (instT ts))

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
    simp [shiftV, Ty.substT, liftT, Ty.up, this]
  | r p => simp [shiftV, Ty.substT, liftT, Ty.up]

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
