import Formal.AscStep

/-!
# T1: type safety (core.md §4) with regions

A configuration is typed when its store is, its main thread has the program's
type, and every child and finalizer of every region has some type. A step of the
main thread or of a child keeps this, and the main thread is a value, `raise v`,
steps, or is blocked: it waits for a child of a region it closes, or uses a
region that is not live. T4 shows the second never happens (`T1_regions`).
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
  cases h with
  | app hf ht _ _ =>
    cases hf with
    | app hc' _ _ hT₁ =>
      cases hc' with
      | perform =>
        obtain ⟨_, _, _, _, _, _, _, _, hx⟩ := hc default
        simp only [Const.ty] at hx; split at hx <;> cases hx
      | const _ hT' =>
        obtain ⟨_, _, _, _, _, _, _, _, hx⟩ := hc ‹_›
        rw [hx] at hT'
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h
        obtain ⟨_, _, _, _, h, la, _, _, -⟩ := (lt.trans hT₁).arrow_left
        cases h
        obtain ⟨_, _, _, _, rfl, -⟩ := la.eff_right
        exact ⟨_, _, .await (ht.valueAny hv _) (Stage.le_refl _) .refl⟩
  | errVal hv' => exact absurd hv' (by
      intro hv''; cases hv'' with
      | ctorApp _ hK _ => simp [Tm.head] at hK
      | constApp2 _ hl => cases c <;> simp_all [Const.layer, Const.ty] <;> exact (hc default).elim)

/-- What a step at the redex does to the regions is typed. -/
theorem rgs_preservation {Ψ Θ Λ μ r t φ Λ' μ' r'} (hΛ : RgsTy S Ψ Λ)
    (h : Typ S Ψ Θ [] r (.ex t φ)) (hout : Entails Θ Facts.age) (hh : RHead Λ μ r Λ' μ' r') :
    RgsTy S Ψ Λ' := by
  have cut : (∃ t φ, Typ S Ψ Θ [] r' (.ex t φ)) → ∃ t φ, Typ S Ψ Facts.age [] r' (.ex t φ) :=
    fun ⟨t, φ, h⟩ => ⟨t, φ, h.cut hout⟩
  cases hh with
  | scope => exact hΛ.append
  | provideEnv => exact hΛ.append
  | hdl => exact hΛ.append
  | open_ hg =>
    refine hΛ.set hg (fun k hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf
      exact .inr ⟨_, .rt Row.none Row.none, .app (.const (ι := { (default : CArg) with s := .rg _ })
        trivial .refl) .cap (Stage.le_refl _) .refl⟩
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
      (c = .addFin ∨ c = .spawn) → Value w → ∃ t φ, Typ S Ψ Facts.age [] (withs E (.await w)) (.ex t φ) := by
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
    obtain ⟨rfl, hΘ, -⟩ := hK.resolve_left fun hp => by subst hp; exact hh.not_pure hr
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
    CfgBelow c.rgs c.mem c.main ∧ (∀ u ∈ Ψ, u.YF c.rgs.length) ∧ CfgAsc c.rgs c.mem c.main

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

/-- A configuration step keeps a typed configuration at the same type and stage,
with a store typing that extends the first one. The type and the stage of the
program name no run-time region. -/
theorem cstep_typed {root Ψ c c' t φ l} (hroot : RootTy S root) (ht : t.YF 0)
    (hφ : φ.substT (kill 0) = φ) (hc : CfgTy S Ψ c t φ) (hs : CStep root l c c') :
    ∃ X, CfgTy S (Ψ ++ X) c' t φ := by
  obtain ⟨hμ, h, hΛ, hb, hΨ, ha⟩ := hc
  cases hs with
  | main hs =>
    obtain ⟨X, hμ', h', hX⟩ := preservation hroot h hμ hb.main hb.mem hΨ hs
    obtain ⟨hb', hl⟩ := hs.below hroot hb
    obtain ⟨ha', hka⟩ := hs.asc hroot ha hb.main
    rw [ht.mono (Nat.zero_le _), Stage.YF_mono hφ (Nat.zero_le _)] at h'
    refine ⟨X, hμ', h', (step_rgs hΛ h hb.main hΨ ha.main hs).weaken X, hb', fun u hu => ?_, hka _ ha'⟩
    rcases List.mem_append.mp hu with hu | hu
    · exact (hΨ u hu).mono hl
    · exact (hX u hu).mono hl
  | kid hg _ hj hs =>
    have hkm := List.mem_of_getElem? hj
    obtain ⟨t', φ', hk⟩ := (hΛ _ _ hg).1 _ hkm
    have hkb := (hb.rgs _ _ hg).1 _ hkm
    have hkA := ha.rgs _ _ hg _ (List.mem_append_left _ hkm)
    obtain ⟨X, hμ', hk', hX⟩ := preservation hroot hk hμ hkb hb.mem hΨ hs
    obtain ⟨hb', hl⟩ := hs.below hroot ⟨hkb, hb.mem, hb.rgs⟩
    obtain ⟨ha', hka⟩ := hs.asc hroot ⟨hkA, ha.rgs, ha.mem⟩ hkb
    refine ⟨X, hμ', h.weakenStore X, ((step_rgs hΛ hk hkb hΨ hkA hs).weaken X).setKid ⟨_, _, hk'⟩,
      CfgBelow.kid ⟨hb.main.mono hl, hb'.mem, hb'.rgs⟩ hb'.main, fun u hu => ?_,
      (hka _ ha.main).setKid ha'⟩
    rw [setKid_length]
    rcases List.mem_append.mp hu with hu | hu
    · exact (hΨ u hu).mono hl
    · exact (hX u hu).mono hl

theorem steps_typed {root c c'} (hroot : RootTy S root) (hs : CSteps root c c') :
    ∀ {Ψ t φ}, t.YF 0 → φ.substT (kill 0) = φ → CfgTy S Ψ c t φ →
      ∃ X, CfgTy S (Ψ ++ X) c' t φ := by
  induction hs with
  | refl => intro Ψ _ _ _ _ h; exact ⟨[], by simpa using h⟩
  | step h₁ _ ih =>
    intro _ _ _ ht hφ h
    obtain ⟨X₁, h'⟩ := cstep_typed hroot ht hφ h h₁
    obtain ⟨X₂, h''⟩ := ih ht hφ h'
    exact ⟨X₁ ++ X₂, by simpa using h''⟩

/-- A raised value has a type of errors of the stage. -/
theorem raise_errors {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    Typ S Ψ Θ [] v (.ex (.errors φ.err) .pure) := by
  obtain ⟨ε, hv', hε⟩ := raise_inv hv h
  obtain ⟨_, _, _, _, U, _, _, _, hU, ht, hsub⟩ := canonical_errors hv hv'
  exact .errVal hv ht hU (hsub.trans (Stage.le_err hε))

/-- The initial configuration of a program. -/
theorem CfgTy.init {e₀ t φ₀} (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀)) (hb : e₀.Below 0) :
    CfgTy S [] ⟨[], [], e₀⟩ t φ₀ :=
  ⟨⟨rfl, by simp⟩, h₀.cut (Entails.none _), fun _ _ h => by simp at h,
    ⟨hb, fun _ _ h => by simp at h, fun _ _ h => by simp at h⟩, by simp,
    ⟨Tm.Asc.of_noIns hb.noIns, fun _ _ h => by simp at h, fun _ _ h => by simp at h⟩⟩

/-- **T1.** A program `· ⊢ e₀ : τ ! φ₀` (a term without run-time regions, at a type and
a stage without them) whose services `Σ₀` provides (G3) reaches
only configurations whose main thread is a value, `raise v` with `v : Errors ε₀`,
makes a step, is blocked (waits for a child of a region it closes, or uses a
region that is not live), or unwinds to a handler (which T4 rules out at the top,
`T1_regions`); every child of a live region is finished, makes a step, asks for a
service `Σ₀` lacks, performs an operation no handler of its own handles, or is
blocked. An operation no handler handles is not an outcome of the main thread:
`Σ₀` has no value of an effect. -/
theorem T1 {root e₀ t φ₀ c} (hroot : RootTy S root)
    (hcov : ∀ N, φ₀.svc (.nm N) → (root.svc N).isSome) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ((Value c.main ∧ ∃ Ψ, StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.age [] c.main (.ex t .pure)) ∨
      (∃ v, c.main = .raise v ∧ ∃ Ψ, Typ S Ψ Facts.age [] v (.ex (.errors φ₀.err) .pure)) ∨
      (∃ l o, Step root l c.rgs c.mem c.main o) ∨ Blocked c.rgs c.main ∨
      ∃ p ℓ a, c.main = .unw p ℓ a) ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step root l c.rgs c.mem k o) ∨ (∃ D, AskStuck S root k D) ∨
        (∃ D, PerformStuck S k D) ∨ Blocked c.rgs k := by
  obtain ⟨Ψ, hμ, h, hΛ, -⟩ := steps_typed hroot hs ht hφ (CfgTy.init h₀ hp)
  simp only [List.nil_append] at hμ h hΛ
  refine ⟨?_, fun p g hg _ k hk => ?_⟩
  · rcases progress (Λ := c.rgs) (root := root) hroot h hμ.1 rfl rfl with hv | hn
    · exact .inl ⟨hv, Ψ, hμ, h.valueAny hv _⟩
    cases hn with
    | raise h' => obtain ⟨v, hv, he⟩ := h'; exact .inr (.inl ⟨v, he, Ψ, raise_errors hv (he ▸ h)⟩)
    | step hstep => exact .inr (.inr (.inl hstep))
    | ask h' =>
      obtain ⟨_, N, _, _, _, hN, _, hD⟩ := h'
      have := hcov N hD; rw [hN] at this; cases this
    | perform h' =>
      obtain ⟨_, Ef, o, _, _, _, _, _, _, _, hop, _, hD⟩ := h'
      obtain ⟨w, hw⟩ := Option.isSome_iff_exists.mp (hcov Ef hD)
      obtain ⟨hwv, hwt⟩ := hroot.svc _ _ hw
      rw [S.op_impl _ _ _ _ _ hop] at hwt
      obtain ⟨_, _, _, _, _, _, _, hN, _⟩ := canonical_errors hwv hwt
      exact hN.elim
    | blocked hb => exact .inr (.inr (.inr (.inl hb)))
    | unw h' => exact .inr (.inr (.inr (.inr h')))
  · obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
    rcases progress (Λ := c.rgs) (root := root) hroot hk' hμ.1 rfl rfl with hv | hn
    · exact .inl (.inl hv)
    cases hn with
    | raise h' => exact .inl (.inr (.inl h'))
    | step hstep => exact .inr (.inl hstep)
    | ask ha => exact .inr (.inr (.inl ⟨_, ha⟩))
    | perform hp => exact .inr (.inr (.inr (.inl ⟨_, hp⟩)))
    | blocked hb => exact .inr (.inr (.inr (.inr hb)))
    | unw h' => exact .inl (.inr (.inr h'))

end Flick
