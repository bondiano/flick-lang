import Formal.Equiv.Slot

/-!
# Contextual equivalence: runs of configurations

Two configurations are related (`CfgR`) when their main threads, their stores and their regions
are. The right side answers a step of the main thread, and of a child in its slot
(`Step.reslot`), by steps of its own (`Formal/Equiv/CfgStep.lean`).

The threads of both runs are closed and have no lazy cell where a pattern reads, wherever the
runs go (`Reach`). Typing gives both (`Formal/Equiv/Context.lean`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- Related configurations. -/
def CfgR (W : World) (c₁ c₂ : Cfg) : Prop :=
  ThR W c₁.mem none c₁.main c₂.main ∧ StR W c₁.mem c₂.mem ∧ RgsR W c₁.mem none c₁.rgs c₂.rgs

/-- The threads of a configuration are closed, and have no lazy cell where a pattern reads. -/
def CfgClosed (c : Cfg) : Prop :=
  c.main.Closed ∧ ¬ c.main.LzTop .any ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → ∀ k ∈ g.kids, k.Closed ∧ ¬ k.LzTop .any

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

/-- The answer of the right main thread, as a run. -/
theorem TAns.main {root : Roots} {Λ₂ Λ' : List Rgn} {μ₂ μ'' : List Tm} {e₂ t'' : Tm}
    (h : TAns root Λ₂ μ₂ e₂ Λ' μ'' t'') : CSteps root ⟨Λ₂, μ₂, e₂⟩ ⟨Λ', μ'', t''⟩ := by
  obtain ⟨μ₃, t₃, hk, hlast⟩ := h
  refine (hk.main Λ₂).trans ?_
  rcases hlast with ⟨rfl, rfl, rfl⟩ | ⟨l', s⟩
  · exact .refl
  · exact .step (.main s) .refl

theorem slot_mem {Λ : List Rgn} {p j : Nat} {g : Rgn} (hg : Λ[p]? = some g) (hj : j < g.kids.length)
    (k : Tm) : ∃ g', (setKid Λ p j k)[p]? = some g' ∧ k ∈ g'.kids :=
  ⟨_, Flick.Elab.setKid_get hg, List.mem_of_getElem? (List.getElem?_set_self hj)⟩

/-- The answer of a right child, as a run with the child in its slot. -/
theorem TAns.kid {root : Roots} {Λ₂ Λ' : List Rgn} {μ₂ μ'' : List Tm} {k₂ t'' : Tm}
    (h : TAns root Λ₂ μ₂ k₂ Λ' μ'' t'') {p j : Nat} {g₂ : Rgn} (e₂ : Tm)
    (hg₂ : Λ₂[p]? = some g₂) (hl₂ : g₂.live = true) (hk₂ : g₂.kids[j]? = some k₂) :
    CSteps root ⟨Λ₂, μ₂, e₂⟩ ⟨setKid Λ' p j t'', μ'', e₂⟩ ∧
      ∃ g', (setKid Λ' p j t'')[p]? = some g' ∧ t'' ∈ g'.kids := by
  obtain ⟨μ₃, t₃, hks, hlast⟩ := h
  have hj := Flick.Elab.lt_of_getElem? hk₂
  rcases hlast with ⟨rfl, rfl, rfl⟩ | ⟨l', s⟩
  · exact ⟨hks.kid e₂ hg₂ hl₂ hk₂, slot_mem hg₂ hj _⟩
  · obtain ⟨g₀, hg₀, hle⟩ := s.kids hg₂
    refine ⟨(hks.kid e₂ hg₂ hl₂ hk₂).trans ?_, slot_mem hg₀ (Nat.lt_of_lt_of_le hj hle) _⟩
    obtain ⟨hg', hk'⟩ := slot_kid hg₂ hk₂ t₃
    have s' := s.reslot hg₂ hk₂ (hks.not_final s) t₃
    have := CStep.kid (e := e₂) hg' hl₂ hk' s'
    rw [setKid_setKid'] at this
    exact .step this .refl

end Flick.Equiv
