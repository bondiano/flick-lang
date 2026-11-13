import Formal.AllElim
import Formal.Cut

/-!
# The substitutions of the substitution lemma

A variable with scheme `∀ᾱ. Θ' ⇒ τ` stands for a pure term typed at `τ` with `ᾱ`
fresh: `Γ`, `Θ` and `Ψ` shifted past them and `Θ'` assumed. The type
substitution lemma then gives the term every instance of the scheme whose facts
hold, and (Sub) any stage. The substituted terms are values or variables: they
have no `inside` form and hold no more than their schemes show (`Typ.holds`),
which keeps the premises of (Lam) and (Task).
-/

namespace Flick

variable {S : Sig}

/-- The facts of the context at the level of a scheme. -/
def Facts.at (Θ : Facts) (s : Scheme) : Facts := Facts.union (Θ.substT (shiftV s.k)) s.cs

/-- A substitution `σ : Γ → Γ'`: each variable goes to a pure term of its scheme,
without `inside`, a value or a variable. -/
def SubOk (S : Sig) (Ψ : List Ty) (Θ : Facts) (Γ Γ' : List Scheme)
    (σ : Nat → Tm) : Prop :=
  ∀ (i : Nat) s, Γ[i]? = some s →
    Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) (σ i) (.ex s.t .pure) ∧
      (σ i).NoIns ∧ (σ i).StandsAt Γ' s

@[simp] theorem idTs_length (k : Nat) : (idTs k).length = k := by simp [idTs]

theorem atom_up_inst (k : Nat) (a : Atom) :
    (liftT k (shiftV k) a).substT (instT (idTs k)) = Ty.ident a := by
  cases a with
  | v i =>
    by_cases hi : i < k
    · simp [liftT, hi, Ty.substT, instT, idTs, Ty.ident]
    · have : ¬ i - k + k + k < k := by omega
      simp [liftT, hi, Ty.substT, instT, idTs, Ty.ident, Ty.up_def, shiftV, this]
      omega
  | r p => simp [liftT, Ty.up_def, Ty.substT, shiftV, instT, Ty.ident]

theorem Scheme.up_inst_self (s : Scheme) : (s.up s.k).inst (idTs s.k) = s.t := by
  simp only [Scheme.up, Scheme.substT, Scheme.inst, Ty.substT_comp]
  conv => rhs; rw [← s.t.substT_ident]
  exact Ty.substT_congr (atom_up_inst s.k)

theorem Scheme.up_holds_self (s : Scheme) (Θ : Facts) :
    (s.up s.k).Holds (Θ.at s) (idTs s.k) := by
  rintro C r ⟨C₀, r₀, h₀, rfl, rfl⟩
  have e₁ : (C₀.substT (liftT s.k (shiftV s.k))).substT (instT (idTs s.k)) = C₀ := by
    rw [Caps.substT_comp]; conv => rhs; rw [← C₀.substT_ident]
    congr 1; funext a; exact atom_up_inst _ a
  have e₂ : (r₀.substT (liftT s.k (shiftV s.k))).substT (instT (idTs s.k)) = r₀ := by
    rw [Ty.substT_comp]; conv => rhs; rw [← r₀.substT_ident]
    exact Ty.substT_congr (atom_up_inst _)
  rw [e₁, e₂]
  exact Outl.fact (.inr h₀)

/-- A variable at the level of its own scheme. -/
theorem Typ.var_self {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {i : Nat} {s : Scheme}
    {φ : Stage} (hi : Γ[i]? = some s) :
    Typ S Ψ (Θ.at s) (Γ.map (Scheme.up s.k)) (.var i) (.ex s.t φ) :=
  .var (σ := s.up s.k) (ts := idTs s.k) (by simp [hi]) (by simp [Scheme.up, Scheme.substT])
    (s.up_holds_self Θ) (by rw [Scheme.up_inst_self]; exact LeLiveI.refl)

/-- A term at the level of a scheme, at an instance whose facts hold. -/
theorem Typ.instantiate {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {s : Scheme}
    {ts : List Ty} (hts : ts.length = s.k) (hH : s.Holds Θ ts) (hn : e.NoIns)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ.map (Scheme.up s.k)) e (.ex s.t .pure)) :
    Typ S Ψ Θ Γ e (.ex (s.inst ts) .pure) := by
  have := Typ.substT h (instT ts) (Fits.noIns hn fun _ => rfl)
  simp only [List.map_map, Function.comp_def, Ty.up_substT _ _ _ hts,
    Scheme.up_substT _ _ _ hts, List.map_id', Flick.J.substT, Facts.at, Facts.union_substT,
    Facts.up_substT _ _ _ hts] at this
  refine Typ.cut this (Entails.of_facts ?_)
  rintro C r (hΘ | ⟨C₀, r₀, h₀, rfl, rfl⟩)
  · exact Outl.fact hΘ
  · exact hH C₀ r₀ h₀

/-- A value at the level of its scheme is a fit replacement. -/
theorem SubOk.value {Ψ : List Ty} {Θ : Facts} {Γ' : List Scheme} {v : Tm} {s : Scheme} (hv : Value v)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) v (.ex s.t .pure)) :
    v.NoIns ∧ v.StandsAt Γ' s :=
  ⟨Typ.noIns h hv, .inl hv⟩

theorem SubOk.lift {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    SubOk S Ψ Θ (Δ ++ Γ) (Δ ++ Γ') (liftS n σ) := by
  intro i s hi
  subst hn
  unfold liftS
  split
  · rw [List.getElem?_append_left (by omega)] at hi
    exact ⟨Typ.var_self (by rw [List.getElem?_append_left (by omega)]; exact hi), (fun h => by cases h),
      .inr (.inl ⟨_, rfl, by rw [List.getElem?_append_left (by omega)]; exact hi⟩)⟩
  · rw [List.getElem?_append_right (by omega)] at hi
    obtain ⟨ht, hnI, hV⟩ := h _ _ hi
    have hr : RenOk Γ' (Δ ++ Γ') (· + Δ.length) := fun j s hj => by
      show (Δ ++ Γ')[j + Δ.length]? = some s
      rw [List.getElem?_append_right (by omega)]; simpa using hj
    refine ⟨Typ.rename ht fun j s hj => ?_, hnI.rename _, hV.rename hr⟩
    rw [List.map_append, List.getElem?_append_right (by simp only [List.length_map]; omega)]
    simpa using hj

theorem SubOk.up {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (k : Nat) (Φ : Facts) :
    SubOk S (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k))
      (Γ'.map (Scheme.up k)) σ := by
  intro i s hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
  obtain ⟨s₀, hs₀, rfl⟩ := hi
  obtain ⟨ht, hnI, hV⟩ := h _ _ hs₀
  have := Typ.substT ht (liftT s₀.k (shiftV k)) (Fits.noIns hnI fun _ => by simp [liftT, Ty.up_def, Ty.substT, shiftV])
  simp only [Flick.J.substT] at this
  have e₁ : (Ψ.map (Ty.up s₀.k)).map (Ty.substT (liftT s₀.k (shiftV k))) =
      (Ψ.map (Ty.up k)).map (Ty.up s₀.k) := by
    simp [Ty.up_comm]; intro _ _; rw [Ty.up_def k]
  have e₂ : (Γ'.map (Scheme.up s₀.k)).map (Scheme.substT (liftT s₀.k (shiftV k))) =
      (Γ'.map (Scheme.up k)).map (Scheme.up s₀.k) := by
    simp [Scheme.up_comm]; intro _ _; rfl
  have e₃ : (Θ.at s₀).substT (liftT s₀.k (shiftV k)) =
      Facts.union ((Θ.substT (shiftV k)).substT (shiftV s₀.k)) (s₀.up k).cs := by
    simp only [Facts.at, Facts.union_substT, Facts.up_comm]; rfl
  rw [e₁, e₂, e₃] at this
  refine ⟨Typ.cut this ?_, hnI, hV.substT _⟩
  have hk : (s₀.up k).k = s₀.k := rfl
  simp only [Facts.at, hk]
  apply Entails.of_facts
  rintro C r (⟨C₀, r₀, h₀, rfl, rfl⟩ | hBound)
  · exact Outl.fact (.inl ⟨C₀, r₀, .inl h₀, rfl, rfl⟩)
  · exact Outl.fact (.inr hBound)

theorem noIns_subst {Γ : List Scheme} {e : Tm} {σ : Nat → Tm} (hΓ : ∀ i, e.Free i → i < Γ.length) (hn : e.NoIns)
    (hσ : ∀ i s, Γ[i]? = some s → (σ i).NoIns) : (e.subst σ).NoIns := by
  intro h
  rcases Tm.has_subst e σ _ h with h | ⟨i, hi, h⟩
  · exact hn h
  · exact hσ i _ (List.getElem?_eq_getElem (hΓ i hi)) h

theorem SubOk.mono {Ψ Θ Θ' Γ Γ'} {σ : Nat → Tm} (h : SubOk S Ψ Θ Γ Γ' σ)
    (hΘ : Entails Θ Θ') : SubOk S Ψ Θ' Γ Γ' σ := fun i s hi =>
  let ⟨ht, hn, hc⟩ := h i s hi
  ⟨Typ.cut ht (hΘ.lift s.k s.cs), hn, hc⟩

theorem SubOk.noIns {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {σ : Nat → Tm}
    (hσ : SubOk S Ψ Θ Γ Γ' σ)
    (hΓ : ∀ i, e.Free i → i < Γ.length) (h : e.NoIns) : (e.subst σ).NoIns :=
  noIns_subst hΓ h fun i s hs => (hσ i s hs).2.1

end Flick
