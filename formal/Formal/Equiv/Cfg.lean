import Formal.Equiv.Slot

/-!
# Contextual equivalence: runs of configurations

Two configurations are related (`CfgR`) when their main threads, their stores and their regions
are. A step of the left one is answered by steps of the right one (`cstep_sim`): a step of the
main thread by `thread_sim`, a step of a child by `thread_sim` in its slot (`Step.reslot`), and
a child that finishes on the left is caught up on the right (`Sim.fcu`), as a region ends only
once its children are finished. So is a run (`csteps_sim`), and a left main thread that ends with
a literal is answered by a right one that ends with it (`lit_sim`).

The threads of both runs are closed; the right one's are, wherever its run goes (`Reach`).
Typing gives both (`Formal/Equiv/Context.lean`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- Related configurations. -/
def CfgR (W : World) (c₁ c₂ : Cfg) : Prop :=
  ThR W c₁.mem none c₁.main c₂.main ∧ StR W c₁.mem c₂.mem ∧ RgsR W c₁.mem none c₁.rgs c₂.rgs

/-- The threads of a configuration are closed. -/
def CfgClosed (c : Cfg) : Prop :=
  c.main.Closed ∧ ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → ∀ k ∈ g.kids, k.Closed

/-- The threads of every configuration a run from `c` reaches are closed. -/
def Reach (root : Roots) (c : Cfg) : Prop := ∀ c', CSteps root c c' → CfgClosed c'

theorem _root_.Flick.CSteps.trans {root : Roots} {c c₁ c' : Cfg} (h₁ : CSteps root c c₁)
    (h₂ : CSteps root c₁ c') : CSteps root c c' := by
  induction h₁ with
  | refl => exact h₂
  | step s _ ih => exact .step s (ih h₂)

theorem Reach.step {root : Roots} {c c' : Cfg} (h : Reach root c) (hs : CSteps root c c') :
    Reach root c' := fun _ h' => h _ (hs.trans h')

theorem setKid_setKid' (Λ : List Rgn) (p j : Nat) (k k' : Tm) :
    setKid (setKid Λ p j k) p j k' = setKid Λ p j k' := by
  apply List.ext_getElem?
  intro q
  rw [setKid_getElem, setKid_getElem, setKid_getElem, Option.map_map]
  congr 1
  funext g
  unfold Rgn.slot
  split <;> simp

theorem slot_kid {Λ : List Rgn} {p j : Nat} {g : Rgn} {k₀ : Tm} (hg : Λ[p]? = some g)
    (hk : g.kids[j]? = some k₀) (k : Tm) :
    (setKid Λ p j k)[p]? = some ⟨g.live, g.kids.set j k, g.fins⟩ ∧
      (g.kids.set j k)[j]? = some k := by
  refine ⟨Flick.Elab.setKid_get hg, ?_⟩
  rw [List.getElem?_set_self (Flick.Elab.lt_of_getElem? hk)]

/-- Steps of the main thread that read no region. -/
theorem KSteps.main {root : Roots} {μ μ' : List Tm} {t t' : Tm} (h : KSteps root μ t μ' t')
    (Λ : List Rgn) : CSteps root ⟨Λ, μ, t⟩ ⟨Λ, μ', t'⟩ := by
  induction h with
  | refl => exact .refl
  | step s _ ih => obtain ⟨l, s⟩ := s Λ; exact .step (.main s) ih

/-- Steps of a child that read no region. -/
theorem KSteps.kid {root : Roots} {μ μ' : List Tm} {t t' : Tm} (h : KSteps root μ t μ' t') :
    ∀ {Λ : List Rgn} {p j : Nat} {g : Rgn} (e : Tm), Λ[p]? = some g → g.live = true →
      g.kids[j]? = some t → CSteps root ⟨Λ, μ, e⟩ ⟨setKid Λ p j t', μ', e⟩ := by
  induction h with
  | refl => intro Λ p j g e hg _ hk; rw [Flick.Elab.setKid_same hg hk]; exact .refl
  | step s _ ih =>
    intro Λ p j g e hg hl hk
    obtain ⟨l, s⟩ := s Λ
    obtain ⟨hg', hk'⟩ := slot_kid hg hk _
    have := ih e hg' hl hk'
    rw [setKid_setKid'] at this
    exact .step (.kid hg hl hk s) this

theorem KSteps.not_final {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ'' : List Tm}
    {t t' t'' : Tm} (h : KSteps root μ t μ' t') (hs : Step root l Λ μ' t' (.cfg Λ' μ'' t'')) :
    ¬ Final t := by
  cases h with
  | refl => exact hs.not_final
  | step s _ => obtain ⟨_, s⟩ := s []; exact s.not_final

theorem RgsR.any {μ : List Tm} {Λ Λ₂ : List Rgn} (h : RgsR W μ none Λ Λ₂) (ex : Option (Nat × Nat)) :
    RgsR W μ ex Λ Λ₂ := by
  refine ⟨h.1, fun p g g' hg hg' => ?_⟩
  obtain ⟨h₁, h₂, h₃, h₄, h₅⟩ := h.2 p g g' hg hg'
  exact ⟨h₁, h₂, fun j k k' _ => h₃ j k k' (by simp), fun j k k' _ => h₄ j k k' (by simp), h₅⟩

/-- The stepped child back in its slot. -/
theorem RgsR.reslot {μ : List Tm} {Λ Λ₂ : List Rgn} {p j : Nat} {k k₂ : Tm}
    (h : RgsR W μ (some (p, j)) Λ Λ₂) (hk : ThR W μ (some (p, j)) k k₂)
    (hf : ∀ g, Λ[p]? = some g → g.live = true → j < g.kids.length → Final k → Final k₂) :
    RgsR W μ none (setKid Λ p j k) (setKid Λ₂ p j k₂) := by
  refine ⟨by rw [setKid_length, setKid_length, h.1], fun q g g' hg hg' => ?_⟩
  rw [setKid_getElem] at hg hg'
  obtain ⟨g₀, hg₀, rfl⟩ := Option.map_eq_some_iff.mp hg
  obtain ⟨g₀', hg₀', rfl⟩ := Option.map_eq_some_iff.mp hg'
  obtain ⟨h₁, h₂, h₃, h₄, h₅⟩ := h.2 q g₀ g₀' hg₀ hg₀'
  by_cases hq : q = p
  · subst hq
    simp only [Rgn.slot, if_true]
    refine ⟨h₁, by simp [h₂], fun i a a' _ ha ha' => ?_, fun i a a' _ hl ha ha' hfa => ?_, h₅⟩
    · by_cases hi : i = j
      · subst hi
        have hj : i < g₀.kids.length := by simpa using Flick.Elab.lt_of_getElem? ha
        rw [List.getElem?_set_self hj] at ha
        rw [List.getElem?_set_self (h₂ ▸ hj)] at ha'
        cases ha; cases ha'; exact hk
      · rw [List.getElem?_set_ne (Ne.symm hi)] at ha ha'
        exact h₃ i a a' (by simp [hi]) ha ha'
    · by_cases hi : i = j
      · subst hi
        have hj : i < g₀.kids.length := by simpa using Flick.Elab.lt_of_getElem? ha
        rw [List.getElem?_set_self hj] at ha
        rw [List.getElem?_set_self (h₂ ▸ hj)] at ha'
        cases ha; cases ha'; exact hf g₀ hg₀ hl hj hfa
      · rw [List.getElem?_set_ne (Ne.symm hi)] at ha ha'
        exact h₄ i a a' (by simp [hi]) hl ha ha' hfa
  · rw [Rgn.slot_ne _ hq, Rgn.slot_ne _ hq]
    exact ⟨h₁, h₂, fun i a a' _ => h₃ i a a' (by simp [hq]),
      fun i a a' _ => h₄ i a a' (by simp [hq]), h₅⟩

/-- A one-shot thread is not finished. -/
theorem OneShot.not_final {μ : List Tm} {τ : Option (Nat × Nat)} {t t' : Tm}
    (h : OneShot W μ τ t t') : ¬ Final t := by
  obtain ⟨E, -, n, -, -, rfl, -⟩ := h
  intro hf
  obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug (fun h => by cases h) hf
  · cases hf
  all_goals cases h

/-- A child that the left side finishes is caught up on the right. -/
theorem ThR.fcu {μ μ₂ : List Tm} {τ : Option (Nat × Nat)} {k k₂ : Tm} (h : ThR W μ τ k k₂)
    (hf : Final k) (hst : StR W μ μ₂) :
    ∃ W' μ₂' k₂', RJ W μ₂ k₂ W' μ₂' k₂' ∧ Final k₂' ∧ ThR W' μ τ k k₂' ∧ StR W' μ μ₂' := by
  rcases h with h | h
  · obtain ⟨W', μ₂', k₂', r, hk, s⟩ := h.fcu rfl hf μ₂
    exact ⟨W', μ₂', k₂', r, hk, .inl s, r.st _ hst⟩
  · exact (h.not_final hf).elim

theorem ThR.mono {W' : World} {μ : List Tm} {τ : Option (Nat × Nat)} {t t' : Tm}
    (h : ThR W μ τ t t') (hW : W.le W') : ThR W' μ τ t t' :=
  h.keep hW fun _ _ _ h => h

/-- **A step of a configuration** of the left side is answered by the right side. -/
theorem cstep_sim {root : Roots} (hroot : root.Src) {l : Lbl} {c₁ c₁' c₂ : Cfg}
    (hr : CfgR W c₁ c₂) (hcl₁ : CfgClosed c₁) (hcl₂ : Reach root c₂)
    (hs : CStep root l c₁ c₁') : ∃ W' c₂', W.le W' ∧ CSteps root c₂ c₂' ∧ CfgR W' c₁' c₂' := by
  obtain ⟨Λ₂, μ₂, e₂⟩ := c₂
  obtain ⟨ht, hst, hrg⟩ := hr
  cases hs with
  | main hs =>
    obtain ⟨W', Λ₂', μ₂', t₂', hW, ⟨μ₃, t₃, hk, hlast⟩, ht', hst', -, hrg'⟩ :=
      thread_sim hroot ht hst hcl₁.1 (fun _ _ h => (hcl₂ _ (h.main Λ₂)).1) hrg
        (fun _ _ h => nomatch h) hs
    refine ⟨W', ⟨Λ₂', μ₂', t₂'⟩, hW, (hk.main Λ₂).trans ?_, ht', hst', hrg'⟩
    rcases hlast with ⟨rfl, rfl, rfl⟩ | ⟨l', s⟩
    · exact .refl
    · exact .step (.main s) .refl
  | @kid Λ p g j k μ l Λ' μ' k' e hg hl hk hs =>
    obtain ⟨g₂, hg₂, hR⟩ := hrg.get hg
    obtain ⟨k₂, hk₂⟩ : ∃ k₂, g₂.kids[j]? = some k₂ := by
      have : j < g₂.kids.length := hR.len ▸ Flick.Elab.lt_of_getElem? hk
      exact ⟨_, List.getElem?_eq_getElem this⟩
    have hl₂ : g₂.live = true := hR.live ▸ hl
    have hcl₂' : ∀ μ'' t'', KSteps root μ₂ k₂ μ'' t'' → t''.Closed := fun μ'' t'' h => by
      obtain ⟨hg', hk'⟩ := slot_kid hg₂ hk₂ t''
      exact (hcl₂ _ (h.kid e₂ hg₂ hl₂ hk₂)).2 p _ hg' t'' (List.mem_of_getElem? hk')
    obtain ⟨W', Λ₂', μ₂', t₂', hW, ⟨μ₃, t₃, hks, hlast⟩, ht', hst', hko, hrg'⟩ :=
      thread_sim hroot (hR.kids j k k₂ (by simp) hk hk₂) hst
        (hcl₁.2 p g hg k (List.mem_of_getElem? hk)) hcl₂' (hrg.any _)
        (fun _ _ h => by cases h; exact ⟨g, hg, hk⟩) hs
    -- the right child's steps, back in its slot
    have run : CSteps root ⟨Λ₂, μ₂, e₂⟩ ⟨setKid Λ₂' p j t₂', μ₂', e₂⟩ := by
      refine (hks.kid e₂ hg₂ hl₂ hk₂).trans ?_
      rcases hlast with ⟨rfl, rfl, rfl⟩ | ⟨l', s⟩
      · exact .refl
      · obtain ⟨hg', hk'⟩ := slot_kid hg₂ hk₂ t₃
        have s' := s.reslot hg₂ hk₂ (hks.not_final s) t₃
        have := CStep.kid (e := e₂) hg' hl₂ hk' s'
        rw [setKid_setKid'] at this
        exact .step this .refl
    have hmain : ThR W' μ' none e e₂ :=
      ht.keep hW fun n a ho ha => hko n none a (by simp) ho ha
    -- a child the left side finishes in a live region is caught up on the right
    by_cases hfin : Final k' ∧ ∃ g', Λ'[p]? = some g' ∧ g'.live = true ∧ j < g'.kids.length
    · obtain ⟨hf, g', hg', hl', hj⟩ := hfin
      obtain ⟨g₂', hg₂', hR'⟩ := hrg'.get hg'
      have hj₂ : j < g₂'.kids.length := hR'.len ▸ hj
      obtain ⟨W'', μ₂'', k₂', r, hf₂, ht'', hst''⟩ := ht'.fcu hf hst'
      have hcu := r.ksteps.kid (root := root) e₂ (Flick.Elab.setKid_get hg₂')
        (by simp only; rw [← hR'.live, hl']) (List.getElem?_set_self hj₂)
      rw [setKid_setKid'] at hcu
      exact ⟨W'', ⟨setKid Λ₂' p j k₂', μ₂'', e₂⟩, hW.trans r.le, run.trans hcu, hmain.mono r.le,
        hst'', (hrg'.keep r.le keepO_refl).reslot ht'' fun _ _ _ _ _ => hf₂⟩
    · refine ⟨W', ⟨setKid Λ₂' p j t₂', μ₂', e₂⟩, hW, run, hmain, hst', hrg'.reslot ht' ?_⟩
      intro g' hg' hl' hj hf
      exact (hfin ⟨hf, g', hg', hl', hj⟩).elim

/-- **A run of configurations** of the left side is answered by a run of the right side. -/
theorem csteps_sim {root : Roots} (hroot : root.Src) {c₁ c₁' : Cfg} (hs : CSteps root c₁ c₁') :
    ∀ {W c₂}, CfgR W c₁ c₂ → Reach root c₁ → Reach root c₂ →
      ∃ W' c₂', W.le W' ∧ CSteps root c₂ c₂' ∧ CfgR W' c₁' c₂' := by
  induction hs with
  | refl => exact fun hr _ _ => ⟨_, _, World.le.refl _, .refl, hr⟩
  | step s hs ih =>
    intro W c₂ hr h₁ h₂
    obtain ⟨W₁, c₂₁, hW, hs₂, hr₁⟩ := cstep_sim hroot hr (h₁ _ .refl) h₂ s
    obtain ⟨W', c₂', hW', hs₂', hr'⟩ := ih hr₁ (h₁.step (.step s .refl)) (h₂.step hs₂)
    exact ⟨W', c₂', hW.trans hW', hs₂.trans hs₂', hr'⟩

/-- **A main thread that ends with a literal** on the left ends with it on the right. -/
theorem lit_sim {root : Roots} {Λ : List Rgn} {μ : List Tm} {o : Lit} {c₂ : Cfg}
    (hr : CfgR W ⟨Λ, μ, .lit o⟩ c₂) : ∃ Λ₂ μ₂, CSteps root c₂ ⟨Λ₂, μ₂, .lit o⟩ := by
  obtain ⟨Λ₂, μ₂, e₂⟩ := c₂
  obtain ⟨ht, -, -⟩ := hr
  rcases ht with ht | ht
  · obtain ⟨W', μ₂', v, r, hv, s⟩ := ht.vcu rfl .lit W μ₂ (World.le.refl _)
    have := (s.value_any .lit hv .top).inv_lit
    subst this
    exact ⟨Λ₂, μ₂', r.ksteps.main Λ₂⟩
  · exact (ht.not_final (.inl .lit)).elim

end Flick.Equiv
