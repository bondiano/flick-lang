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

variable {S : Sig} {mode : TypingMode}

/-- The facts of the context at the level of a scheme. -/
def Facts.at (Θ : Facts) (s : Scheme) : Facts := Facts.union (Θ.substT (shiftV s.k)) s.cs

/-- A substitution `σ : Γ → Γ'`: each variable goes to a pure term of its scheme,
without `inside`, holding no more than its scheme shows, a value or a variable. -/
def SubOkM (S : Sig) (mode : TypingMode) (Ψ : List Ty) (Θ : Facts) (Γ Γ' : List Scheme)
    (σ : Nat → Tm) : Prop :=
  ∀ (i : Nat) s, Γ[i]? = some s →
    TypM S mode (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) (σ i) (.ex s.t .pure) ∧
      (σ i).NoIns ∧ mode.held (held Ψ Γ' (σ i)) s.reach ∧ (σ i).StandsAt Γ' s

abbrev SubOk (S : Sig) := SubOkM S .captures

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
  exact fun x hx => .inr ⟨C₀, .inr h₀, hx⟩

/-- A variable at the level of its own scheme. -/
theorem TypM.var_self {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {i : Nat} {s : Scheme}
    {φ : Stage} (hi : Γ[i]? = some s) :
    TypM S mode Ψ (Θ.at s) (Γ.map (Scheme.up s.k)) (.var i) (.ex s.t φ) :=
  .var (σ := s.up s.k) (ts := idTs s.k) (by simp [hi]) (by simp [Scheme.up, Scheme.substT])
    (s.up_holds_self Θ) (by rw [Scheme.up_inst_self]; exact TypingMode.instSubtype.refl)

/-- A term at the level of a scheme, at an instance whose facts hold. -/
theorem TypM.instantiate {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {s : Scheme}
    {ts : List Ty} (hts : ts.length = s.k) (hH : s.Holds Θ ts) (hn : e.NoIns)
    (h : TypM S mode (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ.map (Scheme.up s.k)) e (.ex s.t .pure)) :
    TypM S mode Ψ Θ Γ e (.ex (s.inst ts) .pure) := by
  have := TypM.substT h (instT ts) (Fits.noIns hn fun _ => rfl)
  simp only [List.map_map, Function.comp_def, Ty.up_substT _ _ _ hts,
    Scheme.up_substT _ _ _ hts, List.map_id', Flick.J.substT, Facts.at, Facts.union_substT,
    Facts.up_substT _ _ _ hts] at this
  refine TypM.cut this fun C r hC x hx => ?_
  rcases hC x hx with h' | ⟨C', h₁ | ⟨C₀, r₀, h₀, rfl, hr⟩, h₂⟩
  · exact .inl h'
  · exact .inr ⟨C', h₁, h₂⟩
  · subst hr; exact hH C₀ r₀ h₀ x h₂

theorem held_up {Ψ Γ e} {s : Scheme} (k : Nat) (h : Caps.Sub (held Ψ Γ e) s.reach) :
    Caps.Sub (held (Ψ.map (Ty.up k)) (Γ.map (Scheme.up k)) e) (s.up k).reach := by
  have sup : ∀ (s' : Scheme) a, (s'.up k).reach a → ∃ b, s'.reach b ∧ a = b.up k := by
    intro s' a ha
    obtain ⟨b, hb, h'⟩ := (Scheme.reach_substT _ _ _).mp ha
    exact ⟨b, hb, by cases b <;> simp_all [shiftV, Atom.up]⟩
  rintro a (⟨i, s', hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at hs
    obtain ⟨s₀, hs₀, rfl⟩ := hs
    obtain ⟨b, hb, rfl⟩ := sup _ _ ha
    exact (Scheme.reach_up _ _ _).mpr (h b (.inl ⟨i, s₀, hi, hs₀, hb⟩))
  · exact (Scheme.reach_up _ k (.r p)).mpr (h _ (.inr (.inl ⟨p, hp, rfl⟩)))
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at ht
    obtain ⟨t₀, ht₀, rfl⟩ := ht
    obtain ⟨b, hb, rfl⟩ := (Ty.reach_up _ _ _).mp ha
    exact (Scheme.reach_up _ _ _).mpr (h b (.inr (.inr ⟨l, t₀, hl, ht₀, hb⟩)))

theorem held_down {Ψ Γ e} {s : Scheme}
    (h : Caps.Sub (held (Ψ.map (Ty.up s.k)) (Γ.map (Scheme.up s.k)) e) s.t.reach) :
    Caps.Sub (held Ψ Γ e) s.reach := by
  rintro a (⟨i, s', hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · have : (s'.up s.k).reach (a.up s.k) := (Scheme.reach_up _ _ _).mpr ha
    exact ⟨_, h _ (.inl ⟨i, _, hi, by simp [hs], this⟩), Atom.down_up _ _⟩
  · exact ⟨.r p, h _ (.inr (.inl ⟨p, hp, rfl⟩)), rfl⟩
  · refine ⟨a.up s.k, h _ (.inr (.inr ⟨l, t.up s.k, hl, by simp [ht], ?_⟩)), Atom.down_up _ _⟩
    exact (Ty.reach_up _ _ _).mpr ⟨a, ha, rfl⟩

/-- A value at the level of its scheme is a fit replacement. -/
theorem SubOkM.value {Ψ : List Ty} {Θ : Facts} {Γ' : List Scheme} {v : Tm} {s : Scheme} (hv : Value v)
    (h : TypM S mode (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) v (.ex s.t .pure)) :
    v.NoIns ∧ mode.held (held Ψ Γ' v) s.reach ∧ v.StandsAt Γ' s := by
  refine ⟨TypM.noIns h hv, ?_, .inl hv⟩
  cases mode with
  | captures => exact held_down (Typ.holds h hv rfl).1
  | live => trivial

theorem SubOk.value {Ψ : List Ty} {Θ : Facts} {Γ' : List Scheme} {v : Tm} {s : Scheme} (hv : Value v)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) v (.ex s.t .pure)) :
    v.NoIns ∧ Caps.Sub (held Ψ Γ' v) s.reach ∧ v.StandsAt Γ' s := SubOkM.value hv h

theorem SubOkM.lift {Ψ Θ Γ Γ' σ} (h : SubOkM S mode Ψ Θ Γ Γ' σ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    SubOkM S mode Ψ Θ (Δ ++ Γ) (Δ ++ Γ') (liftS n σ) := by
  intro i s hi
  subst hn
  unfold liftS
  split
  · rw [List.getElem?_append_left (by omega)] at hi
    refine ⟨TypM.var_self (by rw [List.getElem?_append_left (by omega)]; exact hi), ?_, ?_⟩
    · intro h; cases h
    refine ⟨?_, .inr (.inl ⟨_, rfl, by rw [List.getElem?_append_left (by omega)]; exact hi⟩)⟩
    cases mode with
    | live => trivial
    | captures =>
    rintro a (⟨j, s', hj, hs, ha⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · cases hj; rw [List.getElem?_append_left (by omega), hi] at hs; cases hs; exact ha
    · cases hp
    · cases hl
  · rw [List.getElem?_append_right (by omega)] at hi
    obtain ⟨ht, hnI, hc, hV⟩ := h _ _ hi
    have hr : RenOk Γ' (Δ ++ Γ') (· + Δ.length) := fun j s hj => by
      show (Δ ++ Γ')[j + Δ.length]? = some s
      rw [List.getElem?_append_right (by omega)]; simpa using hj
    refine ⟨TypM.rename ht fun j s hj => ?_, hnI.rename _,
      TypingMode.held_mono (held_rename hr fun j hj => ?_) hc,
      hV.rename hr⟩
    · rw [List.map_append, List.getElem?_append_right (by simp only [List.length_map]; omega)]
      simpa using hj
    · have := TypM.frees ht j hj; simpa using this

theorem SubOkM.up {Ψ Θ Γ Γ' σ} (h : SubOkM S mode Ψ Θ Γ Γ' σ) (k : Nat) (Φ : Facts) :
    SubOkM S mode (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k))
      (Γ'.map (Scheme.up k)) σ := by
  intro i s hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
  obtain ⟨s₀, hs₀, rfl⟩ := hi
  obtain ⟨ht, hnI, hc, hV⟩ := h _ _ hs₀
  have := TypM.substT ht (liftT s₀.k (shiftV k)) (Fits.noIns hnI fun _ => by simp [liftT, Ty.up_def, Ty.substT, shiftV])
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
  refine ⟨TypM.cut this ?_, hnI, ?_, hV.substT _⟩
  rotate_left
  · cases mode with
    | captures => exact held_up k hc
    | live => trivial
  have hk : (s₀.up k).k = s₀.k := rfl
  simp only [Facts.at, hk]
  intro C r hC x hx
  rcases hC x hx with h' | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h'
  · obtain ⟨C₀, r₀, h₀, rfl, rfl⟩ := h₁
    exact .inr ⟨_, .inl ⟨C₀, r₀, .inl h₀, rfl, rfl⟩, h₂⟩
  · exact .inr ⟨C', .inr h₁, h₂⟩

theorem held_subst {Ψ Γ Γ' e} {σ : Nat → Tm} (hΓ : ∀ i, e.Free i → i < Γ.length)
    (hσ : ∀ i s, Γ[i]? = some s → Caps.Sub (held Ψ Γ' (σ i)) s.reach) :
    Caps.Sub (held Ψ Γ' (e.subst σ)) (held Ψ Γ e) := by
  have get : ∀ i, e.Free i → ∃ s, Γ[i]? = some s := fun i hi =>
    ⟨_, List.getElem?_eq_getElem (hΓ i hi)⟩
  rintro a (⟨j, s, hj, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · obtain ⟨i, hi, hj'⟩ := Tm.free_subst e σ j hj
    obtain ⟨s', hs'⟩ := get i hi
    exact .inl ⟨i, s', hi, hs', hσ i s' hs' a (.inl ⟨j, s, hj', hs, ha⟩)⟩
  · rcases Tm.has_subst e σ _ hp with h | ⟨i, hi, h⟩
    · exact .inr (.inl ⟨p, h, rfl⟩)
    · obtain ⟨s', hs'⟩ := get i hi
      exact .inl ⟨i, s', hi, hs', hσ i s' hs' _ (.inr (.inl ⟨p, h, rfl⟩))⟩
  · rcases Tm.has_subst e σ _ hl with h | ⟨i, hi, h⟩
    · exact .inr (.inr ⟨l, t, h, ht, ha⟩)
    · obtain ⟨s', hs'⟩ := get i hi
      exact .inl ⟨i, s', hi, hs', hσ i s' hs' _ (.inr (.inr ⟨l, t, h, ht, ha⟩))⟩

theorem noIns_subst {Γ : List Scheme} {e : Tm} {σ : Nat → Tm} (hΓ : ∀ i, e.Free i → i < Γ.length) (hn : e.NoIns)
    (hσ : ∀ i s, Γ[i]? = some s → (σ i).NoIns) : (e.subst σ).NoIns := by
  intro h
  rcases Tm.has_subst e σ _ h with h | ⟨i, hi, h⟩
  · exact hn h
  · exact hσ i _ (List.getElem?_eq_getElem (hΓ i hi)) h

theorem SubOkM.mono {Ψ Θ Θ' Γ Γ'} {σ : Nat → Tm} (h : SubOkM S mode Ψ Θ Γ Γ' σ)
    (hΘ : Entails Θ Θ') : SubOkM S mode Ψ Θ' Γ Γ' σ := fun i s hi =>
  let ⟨ht, hn, hc⟩ := h i s hi
  ⟨TypM.cut ht (hΘ.lift s.k s.cs), hn, hc⟩

theorem SubOkM.held {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {T : Caps}
    {σ : Nat → Tm} (hσ : SubOkM S mode Ψ Θ Γ Γ' σ)
    (hΓ : ∀ i, e.Free i → i < Γ.length) (h : mode.held (held Ψ Γ e) T) :
    mode.held (held Ψ Γ' (e.subst σ)) T := by
  cases mode with
  | captures => exact (held_subst hΓ fun i s hs => (hσ i s hs).2.2.1).trans h
  | live => trivial

theorem SubOkM.noIns {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {σ : Nat → Tm}
    (hσ : SubOkM S mode Ψ Θ Γ Γ' σ)
    (hΓ : ∀ i, e.Free i → i < Γ.length) (h : e.NoIns) : (e.subst σ).NoIns :=
  noIns_subst hΓ h fun i s hs => (hσ i s hs).2.1

theorem Typ.var_self {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {i : Nat} {s : Scheme}
    {φ : Stage} (hi : Γ[i]? = some s) :
    Typ S Ψ (Θ.at s) (Γ.map (Scheme.up s.k)) (.var i) (.ex s.t φ) := TypM.var_self hi

theorem Typ.instantiate {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {s : Scheme}
    {ts : List Ty} (hts : ts.length = s.k) (hH : s.Holds Θ ts) (hn : e.NoIns)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ.map (Scheme.up s.k)) e (.ex s.t .pure)) :
    Typ S Ψ Θ Γ e (.ex (s.inst ts) .pure) := TypM.instantiate hts hH hn h

theorem SubOk.lift {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    SubOk S Ψ Θ (Δ ++ Γ) (Δ ++ Γ') (liftS n σ) := SubOkM.lift h Δ hn

theorem SubOk.up {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (k : Nat) (Φ : Facts) :
    SubOk S (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k))
      (Γ'.map (Scheme.up k)) σ := SubOkM.up h k Φ

theorem SubOk.mono {Ψ Θ Θ' Γ Γ'} {σ : Nat → Tm} (h : SubOk S Ψ Θ Γ Γ' σ) (hΘ : Entails Θ Θ') :
    SubOk S Ψ Θ' Γ Γ' σ := SubOkM.mono h hΘ

theorem SubOk.held {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {T : Caps} {σ : Nat → Tm}
    (hσ : SubOk S Ψ Θ Γ Γ' σ) (hΓ : ∀ i, e.Free i → i < Γ.length) (h : Caps.Sub (held Ψ Γ e) T) :
    Caps.Sub (held Ψ Γ' (e.subst σ)) T := SubOkM.held hσ hΓ h

theorem SubOk.noIns {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {σ : Nat → Tm}
    (hσ : SubOk S Ψ Θ Γ Γ' σ) (hΓ : ∀ i, e.Free i → i < Γ.length) (h : e.NoIns) :
    (e.subst σ).NoIns := SubOkM.noIns hσ hΓ h

end Flick
