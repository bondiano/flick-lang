import Formal.CaughtCfg

/-!
# Typed configurations

A configuration is typed when its store is, its main thread has the program's
type, and every child and finalizer of every region has some type; what a step at
the redex does to the regions keeps them typed.
-/

namespace Flick

variable {S : Sig}

/-- Children and finalizers of the regions have types. -/
def RgsTy (S : Sig) (Ψ : List Ty) (Λ : List Rgn) : Prop :=
  ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → (∀ k ∈ g.kids, ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ)) ∧
    ∀ f ∈ g.fins, ∃ t φ, Typ S Ψ Facts.age [] f (.ex t φ)

theorem RgsTy.weaken {Ψ Λ} (h : RgsTy S Ψ Λ) (X : List Ty) : RgsTy S (Ψ ++ X) Λ := by
  intro p g hg
  obtain ⟨hk, hf⟩ := h p g hg
  exact ⟨fun k hk' => let ⟨t, φ, h⟩ := hk k hk'; ⟨t, φ, h.weakenStore X⟩,
    fun f hf' => let ⟨t, φ, h⟩ := hf f hf'; ⟨t, φ, h.weakenStore X⟩⟩

theorem RgsTy.set {Ψ Λ p g g'} (h : RgsTy S Ψ Λ) (hg : Λ[p]? = some g)
    (hk : ∀ k ∈ g'.kids, k ∈ g.kids ∨ ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ))
    (hf : ∀ f ∈ g'.fins, f ∈ g.fins ∨ ∃ t φ, Typ S Ψ Facts.age [] f (.ex t φ)) :
    RgsTy S Ψ (setRgn Λ p g') := by
  intro q g'' hq
  by_cases hpq : p = q
  · subst hpq
    rw [setRgn, List.getElem?_set_self (getElem?_lt hg)] at hq; cases hq
    obtain ⟨hk₀, hf₀⟩ := h p g hg
    exact ⟨fun k hk' => (hk k hk').elim (hk₀ k) id, fun f hf' => (hf f hf').elim (hf₀ f) id⟩
  · rw [setRgn, List.getElem?_set_ne hpq] at hq
    exact h q g'' hq

theorem RgsTy.append {Ψ Λ} (h : RgsTy S Ψ Λ) : RgsTy S Ψ (Λ ++ [Rgn.new]) := by
  intro p g hg
  by_cases hp : p < Λ.length
  · rw [List.getElem?_append_left hp] at hg; exact h p g hg
  · rw [List.getElem?_append_right (by omega)] at hg
    cases hj : p - Λ.length with
    | zero => simp [hj, Rgn.new] at hg; subst hg; simp
    | succ => simp [hj] at hg

/-- The computation a child or a finalizer runs has a type. -/
theorem await_arg {Ψ Θ c w t₀ T φ} (h : Typ S Ψ Θ [] (.app (.app (.const c) w) t₀) (.ex T φ))
    (hv : Value t₀) (hc : ∀ ι, ∃ a b r e τ C₁ C₂ ψ, c.ty S ι = .arrow a .pure (.arrow (.eff r e τ C₁) ψ b C₂) Caps.none) :
    ∃ t φ, Typ S Ψ Θ [] (.await t₀) (.ex t φ) := by
  have hn : ¬ Value (.app (.app (.const c) w) t₀) := by
    intro hv'
    cases hv' with
    | ctorApp _ hK _ => simp [Tm.head] at hK
  obtain ⟨_, _, _, _, hf, ht, _, _⟩ := h.app_nonvalue _ _ _ _ rfl rfl hn
  obtain ⟨_, _, _, _, hc', _, _, hT₁⟩ := hf.app_arrow
  obtain ⟨ι, hT'⟩ := hc'.const_witness
  obtain ⟨_, _, _, _, _, _, _, _, hx⟩ := hc ι
  rw [hx] at hT'
  obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
  cases h
  obtain ⟨_, _, _, _, h, la, _, _, -⟩ := (lt.trans hT₁).arrow_left
  cases h
  obtain ⟨_, _, _, _, rfl, -⟩ := LeLive.eff_right la
  exact ⟨_, _, .await (ht.valueAny hv _) (Stage.le_refl _) LeLive.refl⟩

/-- The finalizer `open_` registers: `op` of the capability of its region. -/
theorem op_cap_typ {Ψ Θ} {p : Nat} : ∃ t φ, Typ S Ψ Θ [] (.app (.const .op) (.cap p)) (.ex t φ) := by
  exact ⟨_, _, .app (.const (ι := { (default : CArg) with s := .rg p }) trivial
    LeLive.refl) .cap (Stage.le_refl _) LeLive.refl⟩

/-- What a step at the redex does to the regions is typed. -/
theorem rgs_preservation {Ψ Θ Λ μ r t φ Λ' μ' r'} (hΛ : RgsTy S Ψ Λ)
    (h : Typ S Ψ Θ [] r (.ex t φ)) (hout : Entails Θ Facts.age) (hh : RHead Λ μ r Λ' μ' r') :
    RgsTy S Ψ Λ' := by
  have cut : (∃ t φ, Typ S Ψ Θ [] r' (.ex t φ)) → ∃ t φ, Typ S Ψ Facts.age [] r' (.ex t φ) :=
    fun ⟨t, φ, h⟩ => ⟨t, φ, h.cut hout⟩
  cases hh with
  | scope => exact hΛ.append
  | hdl => exact hΛ.append
  | open_ hg =>
    refine hΛ.set hg (fun k hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf
      exact .inr op_cap_typ
  | finRun hg _ _ _ hf =>
    obtain ⟨_, hf₀⟩ := hΛ _ _ hg
    refine hΛ.set hg (fun k hk => ?_) fun f hf' => .inl (by rw [hf]; exact List.mem_append_left _ hf')
    rcases List.mem_append.mp hk with hk | hk
    · exact .inl hk
    · simp at hk; subst hk; exact .inr (hf₀ _ (by rw [hf]; simp))
  | close hg => exact hΛ.set hg (fun k hk => .inl hk) fun f hf => by simp at hf
  | _ => exact hΛ

/-- A step of one thread keeps the regions typed. -/
theorem step_rgs {root Ψ Λ μ e t φ l Λ' μ' e'} (hΛ : RgsTy S Ψ Λ)
    (h : Typ S Ψ Facts.age [] e (.ex t φ)) (hbl : e.Below Λ.length) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (ha : e.Asc 0) (hs : Step root l Λ μ e (.cfg Λ' μ' e')) : RgsTy S Ψ Λ' := by
  have kid : ∀ {E c p w}, OkE E → e = plug E (.app (.app (.const c) (.own p)) w) →
      (c = .addFin ∨ c = .spawn) → Value w → ∃ t φ, Typ S Ψ Facts.age [] (withs p E (.await w)) (.ex t φ) := by
    intro E c p w hE he hc hv
    subst he
    obtain ⟨hA, -⟩ := asc_of_plug (fin_not_value hc) hE ha
    obtain ⟨t', R', ε', hk, -⟩ := withs_typing (fun h' hΘ => fin_await h' hc hΘ hv)
      (fun h' => fin_not_pure h' hc) (fin_not_value hc) _ hE h (Entails.refl _) hA
    exact ⟨_, _, hk⟩
  cases hs with
  | addFin hE hg _ hv =>
    refine hΛ.set hg (fun k hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf; exact .inr (kid hE rfl (.inl rfl) hv)
  | spawn hE hg _ hv =>
    refine hΛ.set hg (fun k hk => ?_) fun f hf => .inl hf
    rcases List.mem_append.mp hk with hk | hk
    · exact .inl hk
    · simp at hk; subst hk; exact .inr (kid hE rfl (.inr rfl) hv)
  | rt hE hh =>
    obtain ⟨K, Θ', _, _, hr, _, hK, _⟩ := ctx_typing _ hE hh.not_value h
    obtain ⟨rfl, hΘ⟩ := hK.resolve_left fun hp => by subst hp; exact hh.not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact rgs_preservation hΛ hr' (RtCtx.hole hΘ hΨ).outΘ hh
  | _ => exact hΛ

theorem Ty.YF.mono {n m : Nat} {t : Ty} (h : t.YF n) (hnm : n ≤ m) : t.YF m := by
  unfold Ty.YF at h ⊢
  rw [← h, Ty.substT_comp]
  apply Ty.substT_congr
  intro a
  cases a with
  | v i => rfl
  | r p => by_cases hp : p < n <;> simp [kill, hp, Ty.substT, show p < n → p < m from (Nat.lt_of_lt_of_le · hnm)]

theorem Stage.YF_mono {n m : Nat} {φ : Stage} (h : φ.substT (kill n) = φ) (hnm : n ≤ m) :
    φ.substT (kill m) = φ := by
  rw [← h, Stage.substT_comp]
  congr 1; funext a
  cases a with
  | v i => rfl
  | r p => by_cases hp : p < n <;> simp [kill, hp, Ty.substT, show p < n → p < m from (Nat.lt_of_lt_of_le · hnm)]

/-- A typed configuration at the program's type: the store, the main thread, the
children and finalizers are typed with the facts of creation, every term writes
only regions that exist, and the store typing names none that does not. -/
def CfgTy (S : Sig) (Ψ : List Ty) (c : Cfg) (t : Ty) (φ : Stage) : Prop :=
  StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.age [] c.main (.ex t φ) ∧ RgsTy S Ψ c.rgs ∧
    CfgBelow c.rgs c.mem c.main ∧ (∀ u ∈ Ψ, u.YF c.rgs.length) ∧ CfgAsc c.rgs c.mem c.main ∧
    ∃ hs cell, CfgCaught hs cell 0 c.rgs c.main

theorem setKid_get {Λ : List Rgn} {p j k q g} (h : (setKid Λ p j k)[q]? = some g) :
    ∃ g₀, Λ[q]? = some g₀ ∧ (q = p → g = ⟨g₀.live, g₀.kids.set j k, g₀.fins⟩) ∧ (q ≠ p → g = g₀) := by
  unfold setKid at h
  split at h
  · next g₁ hg₁ =>
    by_cases hq : p = q
    · subst hq
      rw [setRgn, List.getElem?_set_self (getElem?_lt hg₁)] at h; cases h
      exact ⟨g₁, hg₁, fun _ => rfl, fun h => (h rfl).elim⟩
    · rw [setRgn, List.getElem?_set_ne hq] at h
      exact ⟨g, h, fun h' => (hq h'.symm).elim, fun _ => rfl⟩
  · exact ⟨g, h, fun h' => by subst h'; simp_all, fun _ => rfl⟩

theorem RgsTy.setKid {Ψ Λ p j k} (h : RgsTy S Ψ Λ) (hk : ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ)) :
    RgsTy S Ψ (setKid Λ p j k) := by
  intro q g hq
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  obtain ⟨hk₀, hf₀⟩ := h q g₀ hg₀
  by_cases hqp : q = p
  · rw [h₁ hqp]
    refine ⟨fun k' hk' => ?_, hf₀⟩
    obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
    rw [List.getElem_set] at *
    split
    · exact hk
    · exact hk₀ _ (List.getElem_mem _)
  · rw [h₂ hqp]; exact ⟨hk₀, hf₀⟩

theorem setKid_length (Λ : List Rgn) (p j : Nat) (k : Tm) : (setKid Λ p j k).length = Λ.length := by
  unfold setKid; split <;> simp [setRgn]

theorem CfgCaught.setKid {hs : Nat → Nat → Prop} {cell : Nat → Nat} {n m : Nat} {Λ : List Rgn} {e k : Tm} {p j : Nat}
    (h : CfgCaught hs cell n Λ e) (hm : m ≤ Λ.length) (hk : k.Caught hs cell (· < m)) {n' : Nat} {e' : Tm}
    (he : e'.Caught hs cell (· < n')) : CfgCaught hs cell n' (setKid Λ p j k) e' := by
  have hl := setKid_length Λ p j k
  refine ⟨he, fun q g hq k' hk' => ?_⟩
  rw [hl]
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  by_cases hqp : q = p
  · rw [h₁ hqp] at hk'
    rcases List.mem_append.mp hk' with hk' | hk'
    · obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
      rw [List.getElem_set]
      split
      · exact ⟨m, hm, hk⟩
      · exact h.rgs q g₀ hg₀ _ (List.mem_append_left _ (List.getElem_mem _))
    · exact h.rgs q g₀ hg₀ _ (List.mem_append_right _ hk')
  · rw [h₂ hqp] at hk'; exact h.rgs q g₀ hg₀ k' hk'

theorem CfgAsc.setKid {Λ μ e p j k} (h : CfgAsc Λ μ e) (hk : k.Asc 0) :
    CfgAsc (setKid Λ p j k) μ e := by
  refine ⟨h.main, fun q g hq k' hk' => ?_, h.mem⟩
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  by_cases hqp : q = p
  · rw [h₁ hqp] at hk'
    rcases List.mem_append.mp hk' with hk' | hk'
    · obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
      rw [List.getElem_set]
      split
      · exact hk
      · exact h.rgs q g₀ hg₀ _ (List.mem_append_left _ (List.getElem_mem _))
    · exact h.rgs q g₀ hg₀ _ (List.mem_append_right _ hk')
  · rw [h₂ hqp] at hk'; exact h.rgs q g₀ hg₀ k' hk'

theorem CfgBelow.kid {Λ μ e p j k} (h : CfgBelow Λ μ e) (hk : k.Below Λ.length) :
    CfgBelow (setKid Λ p j k) μ e := by
  have hl := setKid_length Λ p j k
  refine ⟨hl ▸ h.main, fun l v hv => hl ▸ h.mem l v hv, fun q g hq => ?_⟩
  rw [hl]
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  obtain ⟨hk₀, hf₀⟩ := h.rgs q g₀ hg₀
  by_cases hqp : q = p
  · rw [h₁ hqp]
    refine ⟨fun k' hk' => ?_, hf₀⟩
    obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
    rw [List.getElem_set]
    split
    · exact hk
    · exact hk₀ _ (List.getElem_mem _)
  · rw [h₂ hqp]; exact ⟨hk₀, hf₀⟩

end Flick
