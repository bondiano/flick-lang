import Formal.Elab.CoreStep

/-!
# λFlick⁺ as a `Target`: `Rel` is a weak simulation

A step of the main thread or of a child of a live region is matched from a related
configuration (`cstep_sim`); pure steps of a child are steps of the configuration
(`kid_star`). The result of a configuration is the literal of its main thread, and a literal
is related only to itself (`final_sim`).
-/

namespace Flick.Elab

variable {S : Sig} {root : Roots}

theorem List.set_of_getElem? {α : Type} : ∀ {l : List α} {i : Nat} {a : α}, l[i]? = some a →
    l.set i a = l
  | [], _, _, h => by simp at h
  | x :: l, 0, a, h => by simp at h; simp [h]
  | x :: l, i + 1, a, h => by simp only [List.set_cons_succ]; rw [List.set_of_getElem? (by simpa using h)]

theorem lt_of_getElem? {α : Type} {l : List α} {i : Nat} {a : α} (h : l[i]? = some a) :
    i < l.length := (List.getElem?_eq_some_iff.mp h).1

theorem setKid_same {Λ : List Rgn} {p j : Nat} {g : Rgn} {k : Flick.Tm} (hg : Λ[p]? = some g)
    (hk : g.kids[j]? = some k) : Flick.setKid Λ p j k = Λ := by
  unfold Flick.setKid setRgn
  rw [hg]
  dsimp only
  rw [List.set_of_getElem? hk]
  exact List.set_of_getElem? hg

theorem setKid_get {Λ : List Rgn} {p j : Nat} {g : Rgn} {k : Flick.Tm} (hg : Λ[p]? = some g) :
    (Flick.setKid Λ p j k)[p]? = some ⟨g.live, g.kids.set j k, g.fins⟩ := by
  unfold Flick.setKid setRgn
  rw [hg]
  simp [lt_of_getElem? hg]

theorem setKid_setKid {Λ : List Rgn} {p j : Nat} {g : Rgn} {k k' : Flick.Tm}
    (hg : Λ[p]? = some g) :
    Flick.setKid (Flick.setKid Λ p j k) p j k' = Flick.setKid Λ p j k' := by
  rw [Flick.setKid.eq_def (Flick.setKid Λ p j k), setKid_get hg]
  unfold Flick.setKid setRgn
  rw [hg]
  simp

theorem main_star {Λ : List Rgn} {μ : List Flick.Tm} {t t' : Flick.Tm}
    (h : Star (PStep root) t t') : Star (CS root) ⟨Λ, μ, t⟩ ⟨Λ, μ, t'⟩ := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step ⟨_, .main (s.step Λ μ)⟩ ih

/-- Pure steps of a child of a live region are steps of the configuration. -/
theorem kid_star {μ : List Flick.Tm} {e : Flick.Tm} {p j : Nat} {k k' : Flick.Tm}
    (h : Star (PStep root) k k') : ∀ {Λ : List Rgn} {g : Rgn}, Λ[p]? = some g → g.live = true →
    g.kids[j]? = some k → Star (CS root) ⟨Λ, μ, e⟩ ⟨Flick.setKid Λ p j k', μ, e⟩ := by
  induction h with
  | refl => intro Λ g hg _ hk; rw [setKid_same hg hk]; exact .refl
  | @step a b c s _ ih =>
    intro Λ g hg hl hk
    have hj : j < g.kids.length := lt_of_getElem? hk
    have := ih (setKid_get (k := b) (j := j) hg) hl (by simp [hj])
    rw [setKid_setKid hg] at this
    exact .step ⟨_, .kid hg hl hk (s.step Λ μ)⟩ this

/-- **`Rel` is a weak simulation** for the steps of the core's configurations. -/
theorem cstep_sim (hroot : RootTy S root) {c₁ c₂ c₁' : Cfg} {l : Lbl}
    (hr : CfgR (DP S root) c₁ c₂) (hs : CStep root l c₁ c₁') :
    ∃ c₂', Star (CS root) c₂ c₂' ∧ CfgR (DP S root) c₁' c₂' := by
  obtain ⟨Λ₂, μ₂, t₂⟩ := c₂
  obtain ⟨hm, hμ, hΛ⟩ := hr
  cases hs with
  | main hs' =>
    rcases thread_sim hroot hm ⟨hΛ, hμ⟩ hs' with ⟨rfl, rfl, t₂', hst, hc⟩ |
      ⟨_, _, _, _, hs₂, hc, hΛ', hμ'⟩
    · exact ⟨_, main_star hst, ⟨hc, hμ, hΛ⟩⟩
    · exact ⟨_, .step ⟨_, .main hs₂⟩ .refl, ⟨hc, hμ', hΛ'⟩⟩
  | kid hg hl hk hs' =>
    obtain ⟨g₂, hg₂, hl₂, r⟩ := live_rel hΛ hg hl
    obtain ⟨k₂, hk₂, rk⟩ := r.2.1.getElem? hk
    rcases thread_sim hroot rk ⟨hΛ, hμ⟩ hs' with ⟨rfl, rfl, k₂', hst, hc⟩ |
      ⟨_, _, _, _, hs₂, hc, hΛ', hμ'⟩
    · exact ⟨_, kid_star hst hg₂ hl₂ hk₂, ⟨hm, hμ, RgR.setKid hΛ _ _ hc⟩⟩
    · exact ⟨_, .step ⟨_, .kid hg₂ hl₂ hk₂ hs₂⟩ .refl, ⟨hm, hμ', RgR.setKid hΛ' _ _ hc⟩⟩

/-- The result of a configuration: the literal of its main thread. -/
def obsC (c : Cfg) : Option Lit :=
  match c.main with
  | .lit l => some l
  | _ => none

/-- **A result is matched**: a literal is related only to itself. -/
theorem final_sim {c₁ c₂ : Cfg} (hr : CfgR (DP S root) c₁ c₂) {o : Lit} (ho : obsC c₁ = some o) :
    obsC c₂ = some o := by
  obtain ⟨Λ, μ, t⟩ := c₁
  obtain ⟨Λ₂, μ₂, t₂⟩ := c₂
  have hm := hr.main
  simp only [obsC] at ho ⊢
  cases t <;> simp at ho
  subst ho
  cases hm with
  | lit => rfl
  | base hb => exact (DP.noVal _ _ hb .lit).elim

end Flick.Elab
