import Formal.Preservation

/-!
# Steps keep the terms below the number of regions
-/

namespace Flick

variable {S : Sig}

theorem subst_inst_has {e : Tm} {δ : List Tm} {s : Site} (h : (e.subst (inst δ)).Has s) :
    e.Has s ∨ ∃ w ∈ δ, w.Has s := by
  rcases Tm.has_subst e _ _ h with h | ⟨i, _, h⟩
  · exact .inl h
  · unfold inst at h
    split at h
    · exact .inr ⟨_, List.getElem_mem _, h⟩
    · simp [Tm.Has] at h

theorem field_has {v x : Tm} {l} {s : Site} (h : v.field l = some x) (hx : x.Has s) : v.Has s :=
  matchP_below.field_has h hx

theorem setField_has {v w v' : Tm} {l} {s : Site} (h : v.setField l w = some v') (hs : v'.Has s) :
    v.Has s ∨ w.Has s := by
  induction v generalizing v' with
  | rcons l' e r _ ih =>
    simp only [Tm.setField] at h
    split at h
    · cases h; rcases hs with hs | hs
      · exact .inr hs
      · exact .inl (.inr hs)
    · obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp h
      rcases hs with hs | hs
      · exact .inl (.inl hs)
      · exact (ih hr' hs).imp .inr id
  | _ => simp [Tm.setField] at h

theorem envAppend_has {θ₁ θ₂ : Tm} {s : Site} (h : (θ₁.envAppend θ₂).Has s) : θ₁.Has s ∨ θ₂.Has s := by
  induction θ₁ with
  | econs N v θ _ ih =>
    rcases h with h | h
    · exact .inl (.inl h)
    · exact (ih h).imp .inr id
  | _ => exact .inr h

theorem matchP_has {s : Site} : ∀ (p : Pat) {v : Tm} {δ : List Tm}, matchP p v = some δ →
    ∀ w ∈ δ, w.Has s → v.Has s
  | .wild, _, _, h => by simp [matchP] at h; subst h; simp
  | .var, _, _, h => by simp [matchP] at h; subst h; simp
  | .lit _, v, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .ctor _, v, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .app p q, v, _, h => by
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp h
      intro w hw hs
      rcases List.mem_append.mp hw with hw | hw
      · exact .inl (matchP_has p h1 w hw hs)
      · exact .inr (matchP_has q h2 w hw hs)
    | _ => simp [matchP] at h
  | .rany, v, _, h => by cases v <;> simp [matchP] at h <;> subst h <;> simp
  | .rfield l p r, v, _, h => by
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp h
    intro w hw hs
    rcases List.mem_append.mp hw with hw | hw
    · exact field_has hx (matchP_has p h1 w hw hs)
    · exact matchP_has r h2 w hw hs
  | .or p q, v, _, h => by
    rcases matchP_or.mp h with h | ⟨_, h⟩
    · exact matchP_has p h
    · exact matchP_has q h

theorem build_has {s : Site} (v : Tm) : ∀ {b : Tm}, v.build = some b → b.Has s → v.Has s := by
  intro b h hb
  rw [Tm.build.eq_def] at h
  split at h
  · cases h; rcases hb with hb | hb
    · exact .inr hb
    · simp [Tm.Has] at hb
  · cases h; exact .inr hb
  · rename_i v₁ v₂
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at h
    obtain ⟨b₁, h₁, b₂, h₂, rfl⟩ := h
    rcases hb with hb | hb
    · exact .inl (.inr (build_has v₁ h₁ hb))
    · exact .inr (build_has v₂ h₂ hb)
  · cases h

theorem subst_match_has {p : Pat} {v x : Tm} {δ : List Tm} {s : Site} (hm : matchP p v = some δ)
    (h : (x.subst (inst δ)).Has s) : x.Has s ∨ v.Has s :=
  (subst_inst_has h).imp_right fun ⟨w, hw, h⟩ => matchP_has _ hm w hw h

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
  | tryVal => exact .inl hs
  | withVal => exact .inr (.inr (.inr hs))
  | bind => rcases hs with h | h; exact h; cases h
  | join => exact envAppend_has hs

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

theorem Tm.Below.empty {n : Nat} {x : Tm} (h : ∀ p, ¬ x.Has (.rgn p) ∧ ¬ x.Has (.prov p)) : x.Below n :=
  fun p hp => (hp.elim (h p).1 (h p).2).elim

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
  | build hv hb => exact same (sub fun _ h => h.elim (fun h => .inl (build_has _ hb h)) .inr)
  | provideEnv =>
    refine ⟨h.append _ fun p hp => ?_, by simp⟩
    rcases hp with hp | hp
    · rcases hp with hp | hp | hp | hp
      · cases hp; omega
      · cases hp
      · cases hp
      · rcases hp with hp | hp | hp | hp
        · cases hp
        · cases hp
        · exact Nat.lt_succ_of_lt (h.main p (.inl (.inl hp)))
        · exact Nat.lt_succ_of_lt (h.main p (.inl (.inr hp)))
    · rcases hp with hp | hp | hp | hp
      · cases hp
      · cases hp
      · cases hp
      · rcases hp with hp | hp | hp | hp
        · cases hp
        · cases hp; omega
        · exact Nat.lt_succ_of_lt (h.main p (.inr (.inl hp)))
        · exact Nat.lt_succ_of_lt (h.main p (.inr (.inr hp)))
  | tryRaise => exact same (sub fun _ h => h.elim .inl (.inr ·))
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
    rcases hp with hp | hp
    · rcases hp with hp | hp | hp | hp
      · cases hp; omega
      · cases hp
      · cases hp
      · rcases subst_inst_has hp with hp | ⟨w, hw, hp⟩
        · exact Nat.lt_succ_of_lt (h.main _ (.inl hp))
        · simp at hw; subst hw; cases hp; omega
    · rcases hp with hp | hp | hp | hp
      · cases hp
      · cases hp
      · cases hp
      · rcases subst_inst_has hp with hp | ⟨w, hw, hp⟩
        · exact Nat.lt_succ_of_lt (h.main _ (.inr hp))
        · simp at hw; subst hw; cases hp
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

theorem root_below {root : Name → Option Tm} (hroot : RootTy S root) {N v} (h : root N = some v)
    (n : Nat) : v.Below n := by
  intro p hp
  rcases hp with hp | hp
  · have := ((hroot N v h).2.holds (hroot N v h).1 rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩))
    exact (S.impl_reach N _ this).elim
  · exact ((hroot N v h).2.noIns (hroot N v h).1 (Tm.has_prov_ins hp)).elim

theorem plug_below {E : List Frame} {r r' : Tm} {n : Nat} (h : (plug E r).Below n) (h' : r'.Below n) :
    (plug E r').Below n := fun p hp => by
  rcases hp with hp | hp
  · exact (plug_has_inv E hp).elim (fun h₁ => h p (.inl h₁)) (fun h₁ => h' p (.inl h₁))
  · exact (plug_has_inv E hp).elim (fun h₁ => h p (.inr h₁)) (fun h₁ => h' p (.inr h₁))

/-- What the `with` frames of a context hold, the context holds. -/
theorem withs_has {r x : Tm} {s : Site} :
    ∀ (E : List Frame), (withs E x).Has s → (plug E r).Has s ∨ x.Has s
  | [], h => .inr h
  | F :: E, h => by
    rcases withs_has (x := F.wrap x) (r := F.plug r) E h with h | h
    · exact .inl h
    · cases F with
      | with_ q θ =>
        rcases h with h | h | h | h
        · exact .inl (plug_has E (.inl h))
        · exact .inl (plug_has E (.inr (.inl h)))
        · exact .inl (plug_has E (.inr (.inr (.inl h))))
        · exact .inr h
      | _ => exact .inr h

theorem withs_below {E : List Frame} {r x : Tm} {n : Nat} (h : (plug E r).Below n) (hx : x.Below n) :
    (withs E x).Below n := fun p hp => by
  rcases hp with hp | hp
  · exact (withs_has E hp).elim (fun h₁ => h p (.inl h₁)) (fun h₁ => hx p (.inl h₁))
  · exact (withs_has E hp).elim (fun h₁ => h p (.inr h₁)) (fun h₁ => hx p (.inr h₁))

/-- A step writes only regions that exist after it. -/
theorem Step.below {root l Λ μ e Λ' μ' e'} (hroot : RootTy S root)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) (h : CfgBelow Λ μ e) :
    CfgBelow Λ' μ' e' ∧ Λ.length ≤ Λ'.length := by
  cases hs with
  | pure hE hh =>
    exact ⟨⟨plug_below h.main (h.main.sub fun _ hp => plug_has _ (hh.has hp)), h.mem, h.rgs⟩,
      Nat.le_refl _⟩
  | rt hE hh =>
    obtain ⟨h', hl⟩ := hh.below ⟨h.main.sub fun _ hp => plug_has _ hp, h.mem, h.rgs⟩
    exact ⟨⟨plug_below (h.main.mono hl) h'.main, h'.mem, h'.rgs⟩, hl⟩
  | askWith hE hv =>
    exact ⟨⟨plug_below h.main (h.main.sub fun _ hp => lookupE_has _ hv hp), h.mem, h.rgs⟩,
      Nat.le_refl _⟩
  | askRoot hE _ hr => exact ⟨⟨plug_below h.main (root_below hroot hr _), h.mem, h.rgs⟩, Nat.le_refl _⟩
  | raise hE =>
    exact ⟨⟨plug_below h.main (h.main.sub fun _ hp => plug_has _ (Frame.plug_has _ hp)), h.mem, h.rgs⟩,
      Nat.le_refl _⟩
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
