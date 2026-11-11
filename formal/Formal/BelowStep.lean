import Formal.StepSites

/-!
# Steps keep the terms below the number of regions
-/

namespace Flick

variable {S : Sig}

theorem PHead.has {e e' : Tm} (h : PHead e e') {s : Site} (hs : e'.Has s) : e.Has s := by
  cases h with
  | beta =>
    rcases subst_inst_has hs with h | ⟨w, hw, h⟩
    · exact .inl h
    · simp at hw; subst hw; exact .inr h
  | fix =>
    rcases subst_inst_has hs with h | ⟨w, hw, h⟩
    · exact .inl h
    · simp at hw; rcases hw with rfl | rfl
      · exact .inr h
      · exact .inl h
  | let_ =>
    rcases subst_inst_has hs with h | ⟨w, hw, h⟩
    · exact .inr h
    · simp at hw; subst hw; exact .inl h
  | letrec =>
    rcases subst_inst_has hs with h | ⟨w, hw, h⟩
    · exact .inr h
    · simp at hw; subst hw; exact .inl h
  | proj _ hw => have := field_has hw hs; exact this
  | update _ _ hv' => exact setField_has hv' hs
  | matchYes _ hm =>
    rcases subst_inst_has hs with h | ⟨w, hw, h⟩
    · exact .inr (.inl h)
    · exact .inl (matchP_has _ hm w hw h)
  | matchNo => rcases hs with h | h; exact .inl h; exact .inr (.inr h)
  | guardYes _ hm =>
    simp only [Tm.Has] at hs ⊢
    rcases hs with h | h | h
    · rcases subst_match_has hm h with h | h
      · exact .inr (.inl h)
      · exact .inl h
    · rcases subst_match_has hm h with h | h
      · exact .inr (.inr (.inl h))
      · exact .inl h
    · simp at h; grind
  | guardNo => rcases hs with h | h; exact .inl h; exact .inr (.inr (.inr h))
  | hwithVal _ | hdoneVal _ => simp only [Tm.Has] at hs ⊢; grind
  | @hctlTail k p w ℓ κ E r al Ei _ hk _ =>
    subst hk
    have := plug_has_inv (r := .hole p) (r' := w) (s := s) Ei
    simp only [Tm.Has] at hs ⊢; grind
  | hkeepVal => exact .inr (.inr (.inr hs))
  | @hkarmVal k q w ℓ E al Ei _ hk _ =>
    subst hk
    have := plug_has_inv (r := .hole q) (r' := w) (s := s) Ei
    simp only [Tm.Has] at hs ⊢; grind
  | @hctlDisc k p w ℓ κ E r al Ei _ hk _ =>
    subst hk
    have := plug_has_inv (r := .hole p) (r' := .unw p ℓ w) (s := s) Ei
    simp only [Tm.Has] at hs this ⊢; grind
  | @hresVal a p w ℓ A _ ha _ =>
    subst ha
    have := plug_has_inv (r := .hole p) (r' := w) (s := s) A
    simp only [Tm.Has] at hs ⊢; grind
  | tryVal => exact .inl hs
  | intEq => cases hs

/-- The terms of a configuration write only regions that exist. -/
structure CfgBelow (Λ : List Rgn) (μ : List Tm) (e : Tm) : Prop where
  main : e.Below Λ.length
  mem : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length
  rgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g →
    (∀ k ∈ g.kids, k.Below Λ.length) ∧ ∀ f ∈ g.fins, f.Below Λ.length

theorem setRgn_get {Λ : List Rgn} {p q : Nat} {g g' : Rgn} (hg : Λ[p]? = some g)
    (h : (setRgn Λ p g')[q]? = some g'') : (q = p ∧ g'' = g') ∨ (q ≠ p ∧ Λ[q]? = some g'') := by
  by_cases hq : p = q
  · subst hq; rw [setRgn, List.getElem?_set_self (getElem?_lt hg)] at h; cases h; exact .inl ⟨rfl, rfl⟩
  · rw [setRgn, List.getElem?_set_ne hq] at h; exact .inr ⟨fun h' => hq h'.symm, h⟩

theorem CfgBelow.set {Λ μ e e' p g g'} (h : CfgBelow Λ μ e) (hg : Λ[p]? = some g)
    (hk : ∀ k ∈ g'.kids, k.Below Λ.length) (hf : ∀ f ∈ g'.fins, f.Below Λ.length)
    (he : e'.Below Λ.length) : CfgBelow (Flick.setRgn Λ p g') μ e' := by
  have hl : (Flick.setRgn Λ p g').length = Λ.length := by simp [Flick.setRgn]
  refine ⟨hl ▸ he, fun l v hv => hl ▸ h.mem l v hv, fun q g'' hq => ?_⟩
  rw [hl]
  rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
  · exact ⟨hk, hf⟩
  · exact h.rgs q g'' hq

theorem Tm.Below.empty {n : Nat} {x : Tm} (h : ∀ p, ¬ x.Has (.rgn p)) : x.Below n :=
  fun p hp => (h p hp).elim

theorem CfgBelow.alloc {Λ : List Rgn} {μ : List Tm} {e : Tm} (h : CfgBelow Λ μ e) {v : Tm}
    (hv : v.Below Λ.length) : CfgBelow Λ (μ ++ [v]) e := by
  refine ⟨h.main, fun l v' hv' => ?_, h.rgs⟩
  by_cases hl : l < μ.length
  · rw [List.getElem?_append_left hl] at hv'; exact h.mem l v' hv'
  · rw [List.getElem?_append_right (by omega)] at hv'
    cases hj : l - μ.length with
    | zero => simp [hj] at hv'; subst hv'; exact hv
    | succ => simp [hj] at hv'

theorem CfgBelow.append {Λ : List Rgn} {μ : List Tm} {e : Tm} (h : CfgBelow Λ μ e) (e' : Tm)
    (he : e'.Below (Λ.length + 1)) : CfgBelow (Λ ++ [Rgn.new]) μ e' := by
  refine ⟨by simpa using he, fun l v hv => (h.mem l v hv).mono (by simp), fun q g hq => ?_⟩
  by_cases hql : q < Λ.length
  · rw [List.getElem?_append_left hql] at hq
    obtain ⟨hk, hf⟩ := h.rgs q g hq
    exact ⟨fun k hk' => (hk k hk').mono (by simp), fun f hf' => (hf f hf').mono (by simp)⟩
  · rw [List.getElem?_append_right (by omega)] at hq
    cases hj : q - Λ.length with
    | zero => simp [hj] at hq; subst hq; simp [Rgn.new]
    | succ => simp [hj] at hq

/-- A step at the redex writes only regions that exist after it. -/
theorem RHead.below {Λ μ e Λ' μ' e'} (hh : RHead Λ μ e Λ' μ' e') (h : CfgBelow Λ μ e) :
    CfgBelow Λ' μ' e' ∧ Λ.length ≤ Λ'.length := by
  have sub : ∀ {x : Tm}, (∀ s, x.Has s → e.Has s) → x.Below Λ.length := fun hx => h.main.sub hx
  have same : ∀ {x}, x.Below Λ.length → CfgBelow Λ μ x ∧ Λ.length ≤ Λ.length :=
    fun hx => ⟨⟨hx, h.mem, h.rgs⟩, Nat.le_refl _⟩
  cases hh with
  | refMake hv =>
    refine ⟨⟨Tm.Below.empty fun p => by simp [Tm.Has], fun l v hv' => ?_, h.rgs⟩, Nat.le_refl _⟩
    by_cases hl : l < μ.length
    · rw [List.getElem?_append_left hl] at hv'; exact h.mem l v hv'
    · rw [List.getElem?_append_right (by omega)] at hv'
      cases hj : l - μ.length with
      | zero => simp [hj] at hv'; subst hv'; exact sub fun _ h => .inr h
      | succ => simp [hj] at hv'
  | refGet hl => exact same (h.mem _ _ hl)
  | @refSet v l₀ _ _ hv hlt =>
    refine ⟨⟨Tm.Below.empty fun p => by simp [Tm.Has], fun l v hv' => ?_, h.rgs⟩, Nat.le_refl _⟩
    by_cases hi : l₀ = l
    · subst hi; rw [List.getElem?_set_self hlt] at hv'; cases hv'; exact sub fun _ h => .inr h
    · rw [List.getElem?_set_ne hi] at hv'; exact h.mem l v hv'
  | await => exact same (sub fun _ h => h)
  | scopeRun => exact same (sub fun _ h => Tm.has_scopeRun h)
  | tryRaise => exact same (sub fun _ h => h.elim .inl (.inr ·))
  | hwithCatch => exact same (sub fun _ hs => by simp only [Tm.Has] at hs ⊢; grind)
  | @hctlRaise k p w _ _ ℓ κ E r al tl Ei _ hk _ =>
    subst hk
    refine same (sub fun s hs => ?_)
    have := plug_has_inv (r := .hole p) (r' := .unw p ℓ (.raise w)) (s := s) Ei
    simp only [Tm.Has] at hs this ⊢; grind
  | @hctlUnw k p _ _ ℓ κ E r q ℓ' a al tl Ei _ hk =>
    subst hk
    refine same (sub fun s hs => ?_)
    have := plug_has_inv (r := .hole p) (r' := .unw p ℓ (.unw q ℓ' a)) (s := s) Ei
    simp only [Tm.Has] at hs this ⊢; grind
  | @hkarmRaise k q w _ _ ℓ E al Ei _ hk _ =>
    subst hk
    refine same (sub fun s hs => ?_)
    have := plug_has_inv (r := .hole q) (r' := .unw q ℓ (.raise w)) (s := s) Ei
    simp only [Tm.Has] at hs this ⊢; grind
  | @hkarmUnw k q _ _ ℓ E q' ℓ' a al Ei _ hk =>
    subst hk
    refine same (sub fun s hs => ?_)
    have := plug_has_inv (r := .hole q) (r' := .unw q' ℓ' a) (s := s) Ei
    simp only [Tm.Has] at hs this ⊢; grind
  | @hresRaise a p w _ _ ℓ A _ ha _ =>
    subst ha
    refine same (sub fun s hs => ?_)
    have := plug_has_inv (r := .hole p) (r' := .raise w) (s := s) A
    simp only [Tm.Has] at hs this ⊢; grind
  | @hresUnw a p _ _ ℓ q ℓ' b A _ ha =>
    subst ha
    refine same (sub fun s hs => ?_)
    have := plug_has_inv (r := .hole p) (r' := .unw q ℓ' b) (s := s) A
    simp only [Tm.Has] at hs this ⊢; grind
  | hdl =>
    refine ⟨(h.append _ fun p hp => ?_).alloc (Tm.Below.empty fun p => by simp [Tm.Has]), by simp⟩
    have hm := h.main p
    simp only [Tm.Has] at hp hm
    grind
  | handleYes _ hm =>
    exact same (sub fun _ h => (subst_match_has hm h).elim (fun h => .inr (.inl h)) .inl)
  | handleNo => exact same (sub fun _ h => h.elim .inl (.inr ∘ .inr))
  | hguardYes _ hm =>
    refine same (sub fun _ h => ?_)
    simp only [Tm.Has] at h ⊢
    rcases h with h | h | h
    · rcases subst_match_has hm h with h | h
      · exact .inr (.inl h)
      · exact .inl h
    · rcases subst_match_has hm h with h | h
      · exact .inr (.inr (.inl h))
      · exact .inl h
    · simp at h; grind
  | hguardNo => exact same (sub fun _ h => h.elim .inl (.inr ∘ .inr ∘ .inr))
  | handleNil => exact same (sub fun _ h => .inl h)
  | scope =>
    refine ⟨h.append _ fun p hp => ?_, by simp⟩
    rcases hp with hp | hp | hp | hp
    · cases hp; omega
    · cases hp
    · cases hp
    · rcases subst_inst_has hp with hp | ⟨w, hw, hp⟩
      · exact Nat.lt_succ_of_lt (h.main _ hp)
      · simp at hw; subst hw; cases hp; omega
  | open_ hg =>
    obtain ⟨hk, hf⟩ := h.rgs _ _ hg
    refine ⟨h.set hg hk (fun f hf' => ?_) (sub fun _ h => by cases h; exact .inr rfl),
      by simp [setRgn]⟩
    rcases List.mem_append.mp hf' with hf' | hf'
    · exact hf f hf'
    · simp at hf'; subst hf'; exact sub fun _ h => by rcases h with h | h; cases h; cases h; exact .inr rfl
  | op => exact same (Tm.Below.empty fun p => by simp [Tm.Has])
  | finRun hg _ _ _ hfs =>
    obtain ⟨hk, hf⟩ := h.rgs _ _ hg
    refine ⟨h.set hg (fun k hk' => ?_) (fun f hf' => hf f (by rw [hfs]; simp [hf'])) h.main,
      by simp [setRgn]⟩
    rcases List.mem_append.mp hk' with hk' | hk'
    · exact hk k hk'
    · simp at hk'; subst hk'; exact hf _ (by rw [hfs]; simp)
  | close hg =>
    obtain ⟨hk, _⟩ := h.rgs _ _ hg
    exact ⟨h.set hg hk (fun f hf' => by simp at hf') (sub fun _ h => .inr (.inr (.inr h))),
      by simp [setRgn]⟩

/-- The roots name no run-time region. -/
structure RootBelow (root : Roots) : Prop where
  svc : ∀ N v, root.svc N = some v → ∀ p, ¬ v.Has (.rgn p)
  glob : ∀ g v, root.glob g = some v → ∀ x, ¬ v.Has x

theorem RootTy.below {root : Roots} (hroot : RootTy S root) :
    RootBelow root :=
  ⟨hroot.svcBelow, fun g v h => (hroot.glob g v h).1⟩

theorem root_below {root : Roots} (hroot : RootBelow root) {N v} (h : root.svc N = some v)
    (n : Nat) : v.Below n := fun p hp => (hroot.svc _ _ h p hp).elim

theorem plug_below {E : List Frame} {r r' : Tm} {n : Nat} (h : (plug E r).Below n) (h' : r'.Below n) :
    (plug E r').Below n := fun p hp =>
  (plug_has_inv E hp).elim (fun h₁ => h p h₁) (fun h₁ => h' p h₁)

/-- What the handlers of a context hold, the context holds. -/
theorem withs_has {r x : Tm} {p : Nat} {s : Site} :
    ∀ (E : List Frame), (withs p E x).Has s → (plug E r).Has s ∨ x.Has s
  | [], h => .inr h
  | F :: E, h => by
    rcases withs_has (x := F.wrap p x) (r := F.plug r) E h with h | h
    · exact .inl h
    · cases F with
      | hwith q ℓ E' al rf =>
        simp only [Frame.wrap] at h
        split at h
        · simp only [Tm.Has] at h
          rcases h with h | h | h | h
          · exact .inl (plug_has E (.inl h))
          · exact .inl (plug_has E (.inr (.inl h)))
          · exact .inl (plug_has E (.inr (.inr (.inl h))))
          · exact .inr h
        · exact .inr h
      | hkeep q ℓ E' al =>
        simp only [Frame.wrap] at h
        split at h
        · simp only [Tm.Has] at h
          rcases h with h | h | h | h
          · exact .inl (plug_has E (.inl h))
          · exact .inl (plug_has E (.inr (.inl h)))
          · exact .inl (plug_has E (.inr (.inr (.inl h))))
          · exact .inr h
        · exact .inr h
      | _ => exact .inr h

theorem withs_below {E : List Frame} {r x : Tm} {p n : Nat} (h : (plug E r).Below n) (hx : x.Below n) :
    (withs p E x).Below n := fun q hq =>
  (withs_has E hq).elim (fun h₁ => h q h₁) (fun h₁ => hx q h₁)

/-- A step writes only regions that exist after it. -/
theorem Step.below {root l Λ μ e Λ' μ' e'} (hroot : RootBelow root)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) (h : CfgBelow Λ μ e) :
    CfgBelow Λ' μ' e' ∧ Λ.length ≤ Λ'.length := by
  cases hs with
  | pure hE hh =>
    exact ⟨⟨plug_below h.main (h.main.sub fun _ hp => plug_has _ (hh.has hp)), h.mem, h.rgs⟩,
      Nat.le_refl _⟩
  | rt hE hh =>
    obtain ⟨h', hl⟩ := hh.below ⟨h.main.sub fun _ hp => plug_has _ hp, h.mem, h.rgs⟩
    exact ⟨⟨plug_below (h.main.mono hl) h'.main, h'.mem, h'.rgs⟩, hl⟩
  | glob hE hg =>
    exact ⟨⟨plug_below h.main fun _ hp => (hroot.glob _ _ hg _ hp).elim, h.mem, h.rgs⟩, Nat.le_refl _⟩
  | performRoot hE _ hr hf _ =>
    refine ⟨⟨plug_below h.main fun p hp => ?_, h.mem, h.rgs⟩, Nat.le_refl _⟩
    rcases hp with hp | hp
    · exact root_below hroot hr _ p (field_has hf hp)
    · exact h.main p (plug_has _ (.inr hp))
  | raise hE =>
    exact ⟨⟨plug_below h.main (h.main.sub fun _ hp => plug_has _ (Frame.plug_has _ hp)), h.mem, h.rgs⟩,
      Nat.le_refl _⟩
  | unw hE =>
    exact ⟨⟨plug_below h.main (h.main.sub fun _ hp => plug_has _ (Frame.plug_has _ hp)), h.mem, h.rgs⟩,
      Nat.le_refl _⟩
  | @performAbort E o b v Λ μ p ℓ r Ei Ef al _ _ _ ha =>
    refine ⟨⟨plug_below h.main (h.main.sub fun s hs => plug_has _ ?_), h.mem, h.rgs⟩, Nat.le_refl _⟩
    obtain ⟨h₁, h₂, h₃, h₄, -, h₆⟩ := perform_has (ℓ := ℓ) (c := .abort) (r := r)
      (x := .unw p ℓ (b.subst (inst [v]))) (δ := [v]) (s := s) ha
      (fun w hw hh => by simp at hw; rw [hw] at hh; exact .inr hh) (fun h => h)
    simp only [Tm.Has] at hs
    rcases hs with h | h | h | h | h
    · exact h₁ h
    · exact h₂ h
    · exact h₃ h
    · exact h₄ h
    · exact h₆ h
  | performKept hE hEi _ ha _ _ =>
    exact ⟨⟨plug_below h.main (h.main.sub fun s hs => plug_has _ (kept_has ha hs)), h.mem, h.rgs⟩, Nat.le_refl _⟩
  | @performTail E o b v Λ μ p ℓ r Ei Ef al _ _ _ ha =>
    refine ⟨⟨plug_below h.main (h.main.sub fun s hs => plug_has _ ?_), h.mem, h.rgs⟩, Nat.le_refl _⟩
    obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆⟩ :=
      perform_has (p := p) (ℓ := ℓ) (c := .tail) (r := r) (Ei := Ei) (x := .hole p) (δ := [v]) (s := s) ha
      (fun w hw hh => by simp at hw; rw [hw] at hh; exact .inr hh) (fun h => h.elim .inl (.inr ∘ .inl))
    simp only [Tm.Has] at hs
    rcases hs with h | h | h | h | h | h
    · exact h₁ h
    · exact h₂ h
    · exact h₃ h
    · exact h₄ h
    · exact h₅ h
    · exact h₆ h
  | @performCtl E o b v Λ μ p ℓ r Ei Ef al _ _ _ ha =>
    have hm : ∀ (l : Nat) (v' : Tm), (μ ++ [.lam .todo])[l]? = some v' → v'.Below Λ.length := by
      intro l v' hv'
      by_cases hl : l < μ.length
      · rw [List.getElem?_append_left hl] at hv'; exact h.mem l v' hv'
      · rw [List.getElem?_append_right (by omega)] at hv'
        cases hj : l - μ.length with
        | zero => simp [hj] at hv'; subst hv'; exact Tm.Below.empty fun p => by simp [Tm.Has]
        | succ => simp [hj] at hv'
    refine ⟨⟨plug_below h.main (h.main.sub fun s hs => plug_has _ ?_), hm, h.rgs⟩, Nat.le_refl _⟩
    obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆⟩ :=
      perform_has (p := p) (ℓ := ℓ) (c := .control) (r := r) (Ei := Ei) (x := .hole p) (δ := [v, .kont p μ.length])
        (s := s) ha
      (fun w hw hh => by
        simp at hw
        rcases hw with hw | hw <;> rw [hw] at hh
        · exact .inr hh
        · exact .inl (by simpa [Tm.Has] using hh))
      (fun h => h.elim .inl (.inr ∘ .inl))
    simp only [Tm.Has] at hs
    rcases hs with h | h | h | h | h | h
    · exact h₁ h
    · exact h₂ h
    · exact h₃ h
    · exact h₄ h
    · exact h₅ h
    · exact h₆ h
  | @resume E A k p u Λ μ ℓ r Ef al Ei ℓh _ _ _ hk _ _ =>
    subst hk
    refine ⟨⟨plug_below h.main (h.main.sub fun s hs => plug_has _ ?_), h.mem, h.rgs⟩, Nat.le_refl _⟩
    simp only [Tm.Has] at hs ⊢
    rcases hs with h | h | h | h | h | h | h | h
    · exact .inl h
    · exact .inr (.inl h)
    · rcases plug_has_inv (r := .app (.kont p ℓ) u) A h with h | h
      · exact .inr (.inr (.inr (.inr (.inl h))))
      · simp only [Tm.Has] at h
        rcases h with h | h
        · exact .inl h
        · exact .inr (.inl h)
    · exact .inl h
    · exact .inr (.inl h)
    · exact .inr (.inr (.inl h))
    · exact .inr (.inr (.inr (.inl h)))
    · rcases plug_has_inv (r := .hole p) Ei h with h | h
      · exact .inr (.inr (.inr (.inr (.inr h))))
      · exact .inr (.inr (.inr (.inr (.inl (plug_has A (by simp [Tm.Has, h]))))))
  | addFin hE hg _ _ =>
    obtain ⟨hk, hf⟩ := h.rgs _ _ hg
    refine ⟨h.set hg hk (fun f hf' => ?_) (plug_below h.main (Tm.Below.empty fun p => by simp [Tm.Has])),
      by simp [setRgn]⟩
    rcases List.mem_append.mp hf' with hf' | hf'
    · exact hf f hf'
    · simp at hf'; subst hf'
      exact withs_below h.main (h.main.sub fun _ hp => plug_has _ (.inr hp))
  | spawn hE hg _ _ =>
    obtain ⟨hk, hf⟩ := h.rgs _ _ hg
    refine ⟨h.set hg (fun k hk' => ?_) hf (plug_below h.main (Tm.Below.empty fun p => by simp [Tm.Has])),
      by simp [setRgn]⟩
    rcases List.mem_append.mp hk' with hk' | hk'
    · exact hk k hk'
    · simp at hk'; subst hk'
      exact withs_below h.main (h.main.sub fun _ hp => plug_has _ (.inr hp))

end Flick
