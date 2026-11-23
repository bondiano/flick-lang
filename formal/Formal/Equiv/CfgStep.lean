import Formal.Equiv.Cfg
import Formal.Equiv.CPot

/-!
# Contextual equivalence: a step of a configuration

A step of the left configuration is answered by steps of the right one (`cstep_every`): a step of
the main thread by `thread_sim`, a step of a child by `thread_sim` in its slot (`Step.reslot`),
and a child that finishes on the left is caught up on the right (`Sim.fcu`), as a region ends
only once its children are finished. The answer has a step, or none, and then, without the pairs
of O1b, the potential of the left configuration drops (`cpot`). So is a run answered
(`csteps_sim`), and a left main thread that ends with a literal is answered by a right one that
ends with it (`lit_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem _root_.Flick.CSteps.plus {root : Roots} {l : Lbl} {a b c d : Cfg} (h : CSteps root a b)
    (s : CStep root l b c) (h' : CSteps root c d) : ∃ l' c', CStep root l' a c' ∧ CSteps root c' d := by
  cases h with
  | refl => exact ⟨l, c, s, h'⟩
  | step s₀ r => exact ⟨_, _, s₀, r.trans (.step s h')⟩

/-- An answer of at least one step of the main thread. -/
theorem TPos.main {root : Roots} {Λ₂ Λ₂' : List Rgn} {μ₂ μ₂' : List Tm} {e₂ t₂' : Tm}
    (h : TPos root Λ₂ μ₂ e₂ Λ₂' μ₂' t₂') :
    ∃ l c, CStep root l ⟨Λ₂, μ₂, e₂⟩ c ∧ CSteps root c ⟨Λ₂', μ₂', t₂'⟩ := by
  rcases h with ⟨μ₁, t₁, l, hk, hs⟩ | ⟨hΛ, μ₁, t₁, s, hk⟩
  · exact (hk.main Λ₂).plus (.main hs) .refl
  · subst Λ₂'
    obtain ⟨l, s⟩ := s Λ₂
    exact ⟨l, _, .main s, hk.main Λ₂⟩

/-- An answer of at least one step of a child, in its slot. -/
theorem TPos.kid {root : Roots} {Λ₂ Λ₂' : List Rgn} {μ₂ μ₂' : List Tm} {k₂ t₂' : Tm}
    (h : TPos root Λ₂ μ₂ k₂ Λ₂' μ₂' t₂') {p j : Nat} {g₂ : Rgn} (e₂ : Tm)
    (hg₂ : Λ₂[p]? = some g₂) (hl₂ : g₂.live = true) (hk₂ : g₂.kids[j]? = some k₂) :
    ∃ l c, CStep root l ⟨Λ₂, μ₂, e₂⟩ c ∧ CSteps root c ⟨setKid Λ₂' p j t₂', μ₂', e₂⟩ := by
  rcases h with ⟨μ₁, t₁, l, hk, hs⟩ | ⟨hΛ, μ₁, t₁, s, hk⟩
  · obtain ⟨hg', hk'⟩ := slot_kid hg₂ hk₂ t₁
    have := CStep.kid (e := e₂) hg' hl₂ hk' (hs.reslot hg₂ hk₂ (hk.not_final hs) t₁)
    rw [setKid_setKid'] at this
    exact (hk.kid e₂ hg₂ hl₂ hk₂).plus this .refl
  · subst Λ₂'
    obtain ⟨l, s⟩ := s Λ₂
    obtain ⟨hg', hk'⟩ := slot_kid hg₂ hk₂ t₁
    have := hk.kid e₂ hg' hl₂ hk'
    rw [setKid_setKid'] at this
    exact ⟨l, _, .kid hg₂ hl₂ hk₂ s, this⟩

theorem RgsR.kid {μ : List Tm} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn} (h : RgsR W μ τ Λ Λ₂)
    {p j : Nat} {g : Rgn} {k : Tm} (hg : Λ[p]? = some g) (hk : g.kids[j]? = some k)
    (hne : some (p, j) ≠ τ) : ∃ k₂, ThR W μ (some (p, j)) k k₂ := by
  obtain ⟨g₂, hg₂, hR⟩ := h.get hg
  have : j < g₂.kids.length := hR.len ▸ Flick.Elab.lt_of_getElem? hk
  exact ⟨_, hR.kids j k _ hne hk (List.getElem?_eq_getElem this)⟩

/-- The potential of a thread does not grow at a step of another one answered by none. -/
theorem pot_other {W' : World} {μ μ' μ₂ μ₂' : List Tm} {τ τ₀ : Option (Nat × Nat)} {t t₂ t₂' : Tm}
    (h : ThR W μ τ₀ t t₂) (h' : ThR W' μ' τ₀ t t₂') (hst : StR W μ μ₂) (hst' : StR W' μ' μ₂')
    (hB : ¬ W.B) (hW : W.le W') (hko : KeepO W τ μ μ') (hτ : τ₀ ≠ τ)
    (ho : ∀ n τ', τ' ≠ τ → W'.O n τ' → W.O n τ') : pot W' μ' t ≤ pot W μ t := by
  have := h.xpot_le h' hst hst' hB hW hko hτ ho
  simp only [pot]; omega

theorem pot_congr {W' : World} {μ : List Tm} (h : ∀ n, W'.J₁ n ↔ W.J₁ n) (t : Tm) :
    pot W' μ t = pot W μ t := by
  simp only [pot, xpot_congr h]

/-- **A step of a configuration** of the left side is answered by the right side: by at least
one step, or by none, and then the potential of the left configuration drops. -/
theorem cstep_every {root : Roots} (hroot : root.Src) {l : Lbl} {c₁ c₁' c₂ : Cfg}
    (hr : CfgR W c₁ c₂) (hR : W.R = root) (hcl₁ : Reach root c₁) (hcl₂ : Reach root c₂)
    (hs : CStep root l c₁ c₁') : ∃ W' c₂', W.le W' ∧ CfgR W' c₁' c₂' ∧
      ((∃ l' c, CStep root l' c₂ c ∧ CSteps root c c₂') ∨
        (c₂' = c₂ ∧ (¬ W.B → cpot W' c₁' < cpot W c₁))) := by
  obtain ⟨Λ₂, μ₂, e₂⟩ := c₂
  obtain ⟨ht, hst, hrg⟩ := hr
  have h₁ := (hcl₁ _ .refl).1
  have hcell := (hcl₁ _ .refl).2 l c₁' hs
  have h₁' := (hcl₁ _ (.step hs .refl)).1
  cases hs with
  | @main lb Λ μ e Λ' μ' e' hs =>
    obtain ⟨W', Λ₂', μ₂', t₂', hW, -, ht', hst', hko, hrg', hans⟩ := thread_ans
      (thread_sim hroot ht hst hR h₁.main_closed h₁'.main_closed
        (fun _ _ h => (hcl₂ _ (h.main Λ₂)).1.main_closed)
        h₁.main_matchVals (fun _ _ h => (hcl₂ _ (h.main Λ₂)).1.main_matchVals) hcell
        (fun m hm => h₁.settle.running_of_main hm) h₁.done
        (fun _ _ h => (hcl₂ _ (h.main Λ₂)).1.done) hrg
        (fun _ _ h => nomatch h) hs h₁'.main_noLz (fun _ _ _ h => (hcl₂ _ h.main).1.main_noLz)) hrg
    refine ⟨W', ⟨Λ₂', μ₂', t₂'⟩, hW, ⟨ht', hst', hrg'⟩, ?_⟩
    rcases hans with hp | ⟨e₁, e₂', e₃, e₄, hz⟩
    · exact .inl hp.main
    · subst Λ₂' μ₂' t₂' Λ'
      refine .inr ⟨rfl, fun hB => ?_⟩
      obtain ⟨hlt, ho⟩ := hz hB
      have hk : ksum (pot W' μ') _ ≤ ksum (pot W _) _ := ksum_le fun p g j k hg hk => by
        obtain ⟨k₂, h₂⟩ := hrg.kid hg hk (by simp)
        obtain ⟨k₂', h₂'⟩ := hrg'.kid hg hk (by simp)
        exact pot_other h₂ h₂' hst hst' hB hW hko (by simp) ho
      simp only [cpot]
      dsimp only at hlt hk ⊢
      omega
  | @kid Λ p g j k lb μ Λ' μ' k' e hg hl hk hs =>
    obtain ⟨g₂, hg₂, hR'⟩ := hrg.get hg
    obtain ⟨k₂, hk₂⟩ : ∃ k₂, g₂.kids[j]? = some k₂ := by
      have : j < g₂.kids.length := hR'.len ▸ Flick.Elab.lt_of_getElem? hk
      exact ⟨_, List.getElem?_eq_getElem this⟩
    have hl₂ : g₂.live = true := hR'.live ▸ hl
    have right : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ k₂ Λ' μ'' t'' →
        t''.Closed ∧ ¬ t''.LzTop .any ∧ t''.MatchVals ∧
          (∀ (i : Nat) (o : Tm), μ''[i]? = some (.done o) → DoneCell o) :=
      fun _ _ _ h => by
        obtain ⟨hc, g', hg', hm⟩ := h.kid e₂ hg₂ hl₂ hk₂
        have hc' := (hcl₂ _ hc).1
        have hk' := hc'.kids p g' hg' _ hm
        exact ⟨hk'.1, hk'.2.1, hk'.2.2, hc'.done⟩
    have left : k'.Closed ∧ ¬ k'.LzTop .any ∧ k'.MatchVals := by
      obtain ⟨g₀, hg₀, hle⟩ := hs.kids hg
      obtain ⟨g', hg', hm⟩ := slot_mem hg₀ (Nat.lt_of_lt_of_le (Flick.Elab.lt_of_getElem? hk) hle) k'
      exact h₁'.kids p g' hg' k' hm
    obtain ⟨W', Λ₂', μ₂', t₂', hW, hT, ht', hst', hko, hrg', hans⟩ := thread_ans
      (thread_sim hroot (hR'.kids j k k₂ (by simp) hk hk₂) hst hR
        (h₁.kids p g hg k (List.mem_of_getElem? hk)).1 left.1
        (fun _ _ h => (right _ _ _ ⟨_, _, h, .inl ⟨rfl, rfl, rfl⟩⟩).1)
        (h₁.kids p g hg k (List.mem_of_getElem? hk)).2.2
        (fun _ _ h => (right _ _ _ ⟨_, _, h, .inl ⟨rfl, rfl, rfl⟩⟩).2.2.1)
        hcell (fun m hm => h₁.settle.running_of_kid hg hk hm) h₁.done
        (fun _ _ h => (right _ _ _ ⟨_, _, h, .inl ⟨rfl, rfl, rfl⟩⟩).2.2.2) (hrg.any _)
        (fun _ _ h => by cases h; exact ⟨g, hg, hk⟩) hs left.2.1 (fun _ _ _ h => (right _ _ _ h).2.1))
      (hrg.any _)
    -- the right child's steps, back in its slot
    have run : CSteps root ⟨Λ₂, μ₂, e₂⟩ ⟨setKid Λ₂' p j t₂', μ₂', e₂⟩ := (hT.kid e₂ hg₂ hl₂ hk₂).1
    have hmain : ThR W' μ' none e e₂ :=
      ht.keep hW fun n a ho ha => hko n none a (by simp) ho ha
    -- an answer of no step lowers the potential, in any world with the left cells of `W'`
    have zero : ∀ {W'' : World}, (∀ n, W''.J₁ n ↔ W'.J₁ n) → Λ' = Λ →
        Zero W W' (some (p, j)) μ μ' k k' → ¬ W.B →
        cpot W'' ⟨setKid Λ' p j k', μ', e⟩ < cpot W ⟨Λ, μ, e⟩ := by
      intro W'' hJ hΛ hz hB
      subst Λ'
      obtain ⟨hlt, ho⟩ := hz hB
      have he := pot_other ht hmain hst hst' hB hW hko (by simp) ho
      have hs₁ := ksum_set (pot W' μ') k' hg hk
      have hs₂ := ksum_set (pot W' μ') .todo hg hk
      have hs₃ := ksum_set (pot W μ) .todo hg hk
      have hle : ksum (pot W' μ') (setKid Λ p j .todo) ≤ ksum (pot W μ) (setKid Λ p j .todo) :=
        ksum_slot_le (by rw [pot_todo, pot_todo]; exact Nat.le_refl 0) fun q g₀ i k₀ hg₀ hk₀ hne => by
          obtain ⟨k₂, h₂⟩ := hrg.kid hg₀ hk₀ (by simp)
          obtain ⟨k₂', h₂'⟩ := hrg'.kid hg₀ hk₀ (by simpa using hne)
          exact pot_other h₂ h₂' hst hst' hB hW hko (by simpa using hne) ho
      have e₀ : pot W'' μ' = pot W' μ' := funext (pot_congr hJ)
      have e₁ : cpot W'' ⟨setKid Λ p j k', μ', e⟩ = cpot W' ⟨setKid Λ p j k', μ', e⟩ := by
        simp only [cpot, e₀]
      rw [e₁]
      simp only [cpot, pot_todo] at hs₂ hs₃ ⊢
      dsimp only at hlt he hs₁ hs₂ hs₃ hle ⊢
      omega
    -- a child the left side finishes in a live region is caught up on the right
    by_cases hfin : Final k' ∧ ∃ g', Λ'[p]? = some g' ∧ g'.live = true ∧ j < g'.kids.length
    · obtain ⟨hf, g', hg', hl', hj⟩ := hfin
      obtain ⟨g₂', hg₂', hR''⟩ := hrg'.get hg'
      have hj₂ : j < g₂'.kids.length := hR''.len ▸ hj
      have hl₂' : (⟨g₂'.live, g₂'.kids.set j t₂', g₂'.fins⟩ : Rgn).live = true := by
        simp only; rw [← hR''.live, hl']
      obtain ⟨W'', μ₂'', k₂', r, hf₂, ht'', hst''⟩ :=
        ht'.fcu hf hst' (fun _ => ⟨left.1, (right _ _ _ hT).1⟩)
      have hcu := r.ksteps.kid (root := root) e₂ (Flick.Elab.setKid_get hg₂') hl₂'
        (List.getElem?_set_self hj₂)
      rw [setKid_setKid'] at hcu
      refine ⟨W'', ⟨setKid Λ₂' p j k₂', μ₂'', e₂⟩, hW.trans r.le, ⟨hmain.mono r.le,
        hst'', (hrg'.keep r.le keepO_refl).reslot ht'' fun _ _ _ _ _ => hf₂⟩, ?_⟩
      rcases hans with hp | ⟨e₁, e₂', e₃, hΛ, hz⟩
      · obtain ⟨l', c, s, hc⟩ := hp.kid e₂ hg₂ hl₂ hk₂
        exact .inl ⟨l', c, s, hc.trans hcu⟩
      · subst Λ₂' μ₂' t₂'
        rcases r.ksteps.split (root := root) with ⟨e₄, e₅⟩ | hp
        · subst μ₂'' k₂'
          rw [Flick.Elab.setKid_same hg₂ hk₂]
          exact .inr ⟨rfl, zero (fun n => ⟨r.j₁ n, r.le.J₁ n⟩) hΛ hz⟩
        · exact .inl (TPos.kid (.inr ⟨rfl, hp⟩) e₂ hg₂ hl₂ hk₂)
    · refine ⟨W', ⟨setKid Λ₂' p j t₂', μ₂', e₂⟩, hW, ⟨hmain, hst', hrg'.reslot ht' ?_⟩, ?_⟩
      · intro g' hg' hl' hj hf
        exact (hfin ⟨hf, g', hg', hl', hj⟩).elim
      rcases hans with hp | ⟨e₁, e₂', e₃, hΛ, hz⟩
      · exact .inl (hp.kid e₂ hg₂ hl₂ hk₂)
      · subst Λ₂' μ₂' t₂'
        rw [Flick.Elab.setKid_same hg₂ hk₂]
        exact .inr ⟨rfl, zero (fun _ => Iff.rfl) hΛ hz⟩

/-- **A step of a configuration** of the left side is answered by steps of the right side. -/
theorem cstep_sim {root : Roots} (hroot : root.Src) {l : Lbl} {c₁ c₁' c₂ : Cfg}
    (hr : CfgR W c₁ c₂) (hR : W.R = root) (hcl₁ : Reach root c₁) (hcl₂ : Reach root c₂)
    (hs : CStep root l c₁ c₁') : ∃ W' c₂', W.le W' ∧ CSteps root c₂ c₂' ∧ CfgR W' c₁' c₂' := by
  obtain ⟨W', c₂', hW, hr', h | ⟨rfl, -⟩⟩ := cstep_every hroot hr hR hcl₁ hcl₂ hs
  · obtain ⟨l', c, s, hc⟩ := h
    exact ⟨W', c₂', hW, .step s hc, hr'⟩
  · exact ⟨W', _, hW, .refl, hr'⟩

/-- **A run of configurations** of the left side is answered by a run of the right side. -/
theorem csteps_sim {root : Roots} (hroot : root.Src) {c₁ c₁' : Cfg} (hs : CSteps root c₁ c₁') :
    ∀ {W c₂}, CfgR W c₁ c₂ → W.R = root → Reach root c₁ → Reach root c₂ →
      ∃ W' c₂', W.le W' ∧ CSteps root c₂ c₂' ∧ CfgR W' c₁' c₂' := by
  induction hs with
  | refl => exact fun hr _ _ _ => ⟨_, _, World.le.refl _, .refl, hr⟩
  | step s hs ih =>
    intro W c₂ hr hR h₁ h₂
    obtain ⟨W₁, c₂₁, hW, hs₂, hr₁⟩ := cstep_sim hroot hr hR h₁ h₂ s
    obtain ⟨W', c₂', hW', hs₂', hr'⟩ := ih hr₁ (hW.R.symm.trans hR) (h₁.step (.step s .refl)) (h₂.step hs₂)
    exact ⟨W', c₂', hW.trans hW', hs₂.trans hs₂', hr'⟩

/-- **A main thread that ends with a literal** on the left ends with it on the right. -/
theorem lit_sim {root : Roots} {Λ : List Rgn} {μ : List Tm} {o : Lit} {c₂ : Cfg}
    (hr : CfgR W ⟨Λ, μ, .lit o⟩ c₂) (hcl₂ : W.B → c₂.main.Closed) :
    ∃ Λ₂ μ₂, CSteps root c₂ ⟨Λ₂, μ₂, .lit o⟩ := by
  obtain ⟨Λ₂, μ₂, e₂⟩ := c₂
  obtain ⟨ht, hst, -⟩ := hr
  rcases ht with ht | ht
  · obtain ⟨W', μ₂', v, r, hv, s⟩ :=
      ht.vcu rfl .lit W μ₂ (World.le.refl _) hst (fun hB => ⟨fun _ hi => hi, hcl₂ hB⟩)
    have := (s.value_any .lit hv .top).inv_lit
    subst this
    exact ⟨Λ₂, μ₂', r.ksteps.main Λ₂⟩
  · exact (ht.not_final (.inl .lit)).elim

end Flick.Equiv
