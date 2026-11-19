import Formal.Equiv.ThreadB

/-!
# Contextual equivalence: registrations, roots, globals, errors and signals

`addFin_sim`, `spawn_sim`, `root_sim`, `glob_sim`, `raise_sim`, `unw_sim`: the right side takes
the same step once it has caught up the focus and left the frames of its own cells.
-/

namespace Flick.Equiv

open Flick.Elab (All2 fin_redex raise_redex unw_redex perform_redex)

variable {W : World}

theorem not_final_wrap {p : Nat} {x : Tm} (hx : ¬ Final x) : ∀ F : Frame, ¬ Final (F.wrap p x) := by
  intro F hF
  cases F <;> simp only [Frame.wrap] at hF <;> first
    | exact hx hF
    | (split at hF
       · rcases hF with h | ⟨_, _, h⟩ | ⟨_, _, _, h⟩ <;> cases h
       · exact hx hF)

theorem not_final_withs {p : Nat} : ∀ {E : List Frame} {x : Tm}, ¬ Final x → ¬ Final (withs p E x)
  | [], _, hx => hx
  | F :: E, _, hx => not_final_withs (E := E) (not_final_wrap hx F)

theorem not_final_await {t : Tm} : ¬ Final (.await t) := by
  rintro (h | ⟨_, _, h⟩ | ⟨_, _, _, h⟩) <;> cases h

theorem RgsR.set {μ : List Tm} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn} (h : RgsR W μ τ Λ Λ₂)
    {p : Nat} {g g₂ g' g₂' : Rgn} (hg : Λ[p]? = some g) (hg₂ : Λ₂[p]? = some g₂)
    (hr : RgR W μ τ p g' g₂') : RgsR W μ τ (setRgn Λ p g') (setRgn Λ₂ p g₂') := by
  have hp := (List.getElem?_eq_some_iff.mp hg).1
  have hp₂ := (List.getElem?_eq_some_iff.mp hg₂).1
  refine ⟨by simp [setRgn, h.1], fun q a b ha hb => ?_⟩
  by_cases hq : q = p
  · subst hq
    rw [setRgn, List.getElem?_set_self hp] at ha
    rw [setRgn, List.getElem?_set_self hp₂] at hb
    cases ha; cases hb; exact hr
  · rw [setRgn, List.getElem?_set_ne (Ne.symm hq)] at ha
    rw [setRgn, List.getElem?_set_ne (Ne.symm hq)] at hb
    exact h.2 q a b ha hb

theorem RgR.keep {W' : World} {μ μ' : List Tm} {τ : Option (Nat × Nat)} {p : Nat} {g g₂ : Rgn}
    (h : RgR W μ τ p g g₂) (hW : W.le W') (hk : KeepO W τ μ μ') : RgR W' μ' τ p g g₂ :=
  ⟨h.live, h.len, fun j k k' hj hk' hk'' => (h.kids j k k' hj hk' hk'').keep hW
      (fun n e ho he => hk n _ e hj ho he), h.fin,
    Flick.Elab.All2.mono h.fins fun _ _ r => ⟨r.1.mono hW, r.2⟩⟩

theorem keepO_refl {τ : Option (Nat × Nat)} {μ : List Tm} : KeepO W τ μ μ := fun _ _ _ _ _ h => h

/-- A new finalizer of a related region. -/
theorem RgR.addFin {μ : List Tm} {τ : Option (Nat × Nat)} {p : Nat} {g g₂ : Rgn} {f f₂ : Tm}
    (h : RgR W μ τ p g g₂) (hf : Sim W .any f f₂) (hnf : ¬ Final f) :
    RgR W μ τ p ⟨g.live, g.kids, g.fins ++ [f]⟩ ⟨g₂.live, g₂.kids, g₂.fins ++ [f₂]⟩ :=
  ⟨h.live, h.len, h.kids, h.fin, Flick.Elab.All2.app h.fins (.cons ⟨hf, fun h' => (hnf h').elim⟩ .nil)⟩

/-- A new child of a related region. -/
theorem RgR.addKid {μ : List Tm} {τ : Option (Nat × Nat)} {p : Nat} {g g₂ : Rgn} {k k₂ : Tm}
    (h : RgR W μ τ p g g₂) (hk : Sim W .any k k₂) (hnf : ¬ Final k) :
    RgR W μ τ p ⟨g.live, g.kids ++ [k], g.fins⟩ ⟨g₂.live, g₂.kids ++ [k₂], g₂.fins⟩ := by
  refine ⟨h.live, by simp [h.len], fun j a a' hj ha ha' => ?_, fun j a a' hj hl ha ha' hfa => ?_,
    h.fins⟩
  · simp only [List.getElem?_append] at ha ha'
    by_cases hj' : j < g.kids.length
    · rw [if_pos hj'] at ha; rw [if_pos (h.len ▸ hj')] at ha'
      exact h.kids j a a' hj (by simpa using ha) (by simpa using ha')
    · rw [if_neg hj'] at ha; rw [if_neg (h.len ▸ hj')] at ha'
      simp [List.getElem?_cons, h.len] at ha ha'
      obtain ⟨-, rfl⟩ := ha; obtain ⟨-, rfl⟩ := ha'; exact .inl hk
  · simp only [List.getElem?_append] at ha ha'
    by_cases hj' : j < g.kids.length
    · rw [if_pos hj'] at ha; rw [if_pos (h.len ▸ hj')] at ha'
      exact h.fin j a a' hj hl (by simpa using ha) (by simpa using ha') hfa
    · rw [if_neg hj'] at ha
      simp [List.getElem?_cons] at ha
      obtain ⟨-, rfl⟩ := ha; exact (hnf hfa).elim

end Flick.Equiv
