import Formal.Preservation

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
  ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → (∀ k ∈ g.kids, ∃ t φ, Typ S Ψ Facts.all [] k (.ex t φ)) ∧
    ∀ f ∈ g.fins, ∃ t φ, Typ S Ψ Facts.all [] f (.ex t φ)

theorem RgsTy.weaken {Ψ Λ} (h : RgsTy S Ψ Λ) (X : List Ty) : RgsTy S (Ψ ++ X) Λ := by
  intro p g hg
  obtain ⟨hk, hf⟩ := h p g hg
  exact ⟨fun k hk' => let ⟨t, φ, h⟩ := hk k hk'; ⟨t, φ, h.weakenStore X⟩,
    fun f hf' => let ⟨t, φ, h⟩ := hf f hf'; ⟨t, φ, h.weakenStore X⟩⟩

theorem RgsTy.set {Ψ Λ p g g'} (h : RgsTy S Ψ Λ) (hg : Λ[p]? = some g)
    (hk : ∀ k ∈ g'.kids, k ∈ g.kids ∨ ∃ t φ, Typ S Ψ Facts.all [] k (.ex t φ))
    (hf : ∀ f ∈ g'.fins, f ∈ g.fins ∨ ∃ t φ, Typ S Ψ Facts.all [] f (.ex t φ)) :
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
theorem await_arg {Ψ c w t₀ T φ} (h : Typ S Ψ Facts.all [] (.app (.app (.const c) w) t₀) (.ex T φ))
    (hv : Value t₀) (hc : ∀ ι, ∃ a b r e τ C₁ C₂ ψ, c.ty S ι = .arrow a .pure (.arrow (.eff r e τ C₁) ψ b C₂) Caps.none) :
    ∃ t φ, Typ S Ψ Facts.all [] (.await t₀) (.ex t φ) := by
  cases h with
  | app hf ht _ _ =>
    cases hf with
    | app hc' _ _ hT₁ =>
      cases hc' with
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
theorem rgs_preservation {Ψ Λ μ r t φ Λ' μ' r'} (hΛ : RgsTy S Ψ Λ)
    (h : Typ S Ψ Facts.all [] r (.ex t φ)) (hh : RHead Λ μ r Λ' μ' r') : RgsTy S Ψ Λ' := by
  cases hh with
  | scope => exact hΛ.append
  | open_ hg =>
    refine hΛ.set hg (fun k hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf
      exact .inr ⟨_, .rt Row.none Row.none, .app (.const (ι := { (default : CArg) with s := .rg _ })
        trivial .refl) .cap (Stage.le_refl _) .refl⟩
  | addFin hg _ hv =>
    refine hΛ.set hg (fun k hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf
      exact .inr (await_arg h hv fun _ => ⟨_, _, _, _, _, _, _, _, rfl⟩)
  | spawn hg _ hv =>
    refine hΛ.set hg (fun k hk => ?_) fun f hf => .inl hf
    rcases List.mem_append.mp hk with hk | hk
    · exact .inl hk
    · simp at hk; subst hk
      exact .inr (await_arg h hv fun _ => ⟨_, _, _, _, _, _, _, _, rfl⟩)
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
    (h : Typ S Ψ Facts.all [] e (.ex t φ)) (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    RgsTy S Ψ Λ' := by
  cases hs with
  | rt hE hh =>
    obtain ⟨K, Θ', _, _, hr, _, hK, _⟩ := ctx_typing _ hE hh.not_value h
    obtain ⟨rfl, rfl, -⟩ := hK.resolve_left fun hp => by subst hp; exact hh.not_pure hr
    simp only [List.map_up_zero] at hr
    exact rgs_preservation hΛ hr hh
  | _ => exact hΛ

/-- A typed configuration at the program's type. -/
def CfgTy (S : Sig) (Ψ : List Ty) (c : Cfg) (t : Ty) (φ : Stage) : Prop :=
  StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.all [] c.main (.ex t φ) ∧ RgsTy S Ψ c.rgs

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

theorem RgsTy.setKid {Ψ Λ p j k} (h : RgsTy S Ψ Λ) (hk : ∃ t φ, Typ S Ψ Facts.all [] k (.ex t φ)) :
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

/-- A configuration step keeps a typed configuration at the same type and stage,
with a store typing that extends the first one. -/
theorem cstep_typed {root Ψ c c' t φ l} (hroot : RootTy S root) (hc : CfgTy S Ψ c t φ)
    (hs : CStep root l c c') : ∃ X, CfgTy S (Ψ ++ X) c' t φ := by
  obtain ⟨hμ, h, hΛ⟩ := hc
  cases hs with
  | main hs =>
    obtain ⟨X, hμ', h'⟩ := preservation hroot h hμ hs
    exact ⟨X, hμ', h', (step_rgs hΛ h hs).weaken X⟩
  | kid hg _ hj hs =>
    obtain ⟨t', φ', hk⟩ := (hΛ _ _ hg).1 _ (List.mem_of_getElem? hj)
    obtain ⟨X, hμ', hk'⟩ := preservation hroot hk hμ hs
    exact ⟨X, hμ', h.weakenStore X, ((step_rgs hΛ hk hs).weaken X).setKid ⟨_, _, hk'⟩⟩

theorem steps_typed {root c c'} (hroot : RootTy S root) (hs : CSteps root c c') :
    ∀ {Ψ t φ}, CfgTy S Ψ c t φ → ∃ X, CfgTy S (Ψ ++ X) c' t φ := by
  induction hs with
  | refl => intro Ψ _ _ h; exact ⟨[], by simpa using h⟩
  | step h₁ _ ih =>
    intro _ _ _ h
    obtain ⟨X₁, h'⟩ := cstep_typed hroot h h₁
    obtain ⟨X₂, h''⟩ := ih h'
    exact ⟨X₁ ++ X₂, by simpa using h''⟩

/-- A raised value has a type of errors of the stage. -/
theorem raise_errors {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    Typ S Ψ Θ [] v (.ex (.errors φ.err) .pure) := by
  obtain ⟨ε, hv', hε⟩ := raise_inv hv h
  obtain ⟨_, _, _, _, U, _, _, _, hU, ht, hsub⟩ := canonical_errors hv hv'
  exact .errVal hv ht hU (hsub.trans (Stage.le_err hε))

/-- The initial configuration of a program. -/
theorem CfgTy.init {e₀ t φ₀} (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀)) :
    CfgTy S [] ⟨[], [], e₀⟩ t φ₀ :=
  ⟨⟨rfl, by simp⟩, h₀.cut (Entails.all _), fun _ _ h => by simp at h⟩

/-- **T1.** A program `· ⊢ e₀ : τ ! φ₀` whose services `Σ₀` provides (Г3) reaches
only configurations whose main thread is a value, `raise v` with `v : Errors ε₀`,
makes a step, or is blocked (waits for a child of a region it closes, or uses a
region that is not live); every child of a live region is finished, makes a step,
asks for a service `Σ₀` lacks, or is blocked. -/
theorem T1 {root e₀ t φ₀ c} (hroot : RootTy S root)
    (hcov : ∀ N, φ₀.svc (.nm N) → (root N).isSome) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ((Value c.main ∧ ∃ Ψ, StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.all [] c.main (.ex t .pure)) ∨
      (∃ v, c.main = .raise v ∧ ∃ Ψ, Typ S Ψ Facts.all [] v (.ex (.errors φ₀.err) .pure)) ∨
      (∃ l o, Step root l c.rgs c.mem c.main o) ∨ Blocked c.rgs c.main) ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step root l c.rgs c.mem k o) ∨ (∃ D, AskStuck root k D) ∨
        Blocked c.rgs k := by
  obtain ⟨Ψ, hμ, h, hΛ⟩ := steps_typed hroot hs (CfgTy.init h₀)
  simp only [List.nil_append] at hμ h hΛ
  refine ⟨?_, fun p g hg _ k hk => ?_⟩
  · rcases progress (Λ := c.rgs) (root := root) h hμ.1 rfl rfl with
      hv | ⟨v, hv, he⟩ | hstep | ⟨_, N, _, _, _, hN, hD⟩ | hb
    · exact .inl ⟨hv, Ψ, hμ, h.valueAny hv _⟩
    · exact .inr (.inl ⟨v, he, Ψ, raise_errors hv (he ▸ h)⟩)
    · exact .inr (.inr (.inl hstep))
    · have := hcov N hD; rw [hN] at this; cases this
    · exact .inr (.inr (.inr hb))
  · obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
    rcases progress (Λ := c.rgs) (root := root) hk' hμ.1 rfl rfl with
      hv | ⟨v, hv, he⟩ | hstep | ha | hb
    · exact .inl (.inl hv)
    · exact .inl (.inr ⟨v, hv, he⟩)
    · exact .inr (.inl hstep)
    · exact .inr (.inr (.inl ⟨_, ha⟩))
    · exact .inr (.inr (.inr hb))

end Flick
