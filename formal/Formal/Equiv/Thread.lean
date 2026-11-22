import Formal.Equiv.RHead

/-!
# Contextual equivalence: related threads and regions

A thread is related to a thread (`ThR`) by `Sim`, or it is about to force a left cell it has just
allocated for O1c (`OneShot`): only that thread may (`World.O`), so the cell is still pending.
Regions are related one by one (`RgsR`): their liveness, their children as threads (but the one
that steps, `ex`), and their finalizers. The right side answers a step of a thread by steps that
keep the regions and at most one last step (`TAns`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- A thread that allocated a left cell for O1c and forces it at its next step. -/
def OneShot (W : World) (μ₁ : List Tm) (τ : Option (Nat × Nat)) (t t' : Tm) : Prop :=
  ∃ E E' n e e', t = plug E (.force (.loc n)) ∧ t' = plug E' e' ∧ OkE E ∧ OkE E' ∧ FrL W E E' ∧
    W.O n τ ∧ μ₁[n]? = some (.lazy e) ∧ Sim W .any e e'

/-- Related threads. -/
def ThR (W : World) (μ₁ : List Tm) (τ : Option (Nat × Nat)) (t t' : Tm) : Prop :=
  Sim W .any t t' ∨ OneShot W μ₁ τ t t'

/-- Related regions; `ex` is a thread not related here. A finished child of a live region, and a
finished finalizer, is related to a finished one. -/
structure RgR (W : World) (μ₁ : List Tm) (ex : Option (Nat × Nat)) (p : Nat) (g g' : Rgn) :
    Prop where
  live : g.live = g'.live
  len : g.kids.length = g'.kids.length
  kids : ∀ j k k', some (p, j) ≠ ex → g.kids[j]? = some k → g'.kids[j]? = some k' →
    ThR W μ₁ (some (p, j)) k k'
  fin : ∀ j k k', some (p, j) ≠ ex → g.live = true → g.kids[j]? = some k →
    g'.kids[j]? = some k' → Final k → Final k'
  fins : All2 (fun f f' => Sim W .any f f' ∧ (Final f → Final f')) g.fins g'.fins

def RgsR (W : World) (μ₁ : List Tm) (ex : Option (Nat × Nat)) (Λ Λ' : List Rgn) : Prop :=
  Λ.length = Λ'.length ∧ ∀ p g g', Λ[p]? = some g → Λ'[p]? = some g' → RgR W μ₁ ex p g g'

/-- Steps of a thread that read no region and keep them. -/
inductive KSteps (root : Roots) : List Tm → Tm → List Tm → Tm → Prop
  | refl {μ t} : KSteps root μ t μ t
  | step {μ t μ₁ t₁ μ' t'} : (∀ Λ, ∃ l, Step root l Λ μ t (.cfg Λ μ₁ t₁)) →
      KSteps root μ₁ t₁ μ' t' → KSteps root μ t μ' t'

theorem KSteps.trans {root : Roots} {μ μ₁ μ' : List Tm} {t t₁ t' : Tm}
    (h₁ : KSteps root μ t μ₁ t₁) (h₂ : KSteps root μ₁ t₁ μ' t') : KSteps root μ t μ' t' := by
  induction h₁ with
  | refl => exact h₂
  | step s _ ih => exact .step s (ih h₂)

theorem AStar.ksteps {root : Roots} {μ μ' : List Tm} {t t' : Tm}
    (h : AStar μ t μ' t') : KSteps root μ t μ' t' := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step (fun Λ => s.step Λ) ih

theorem RJ.ksteps {root : Roots} {W' : World} {μ μ' : List Tm} {t t' : Tm}
    (h : RJ W μ t W' μ' t') : KSteps root μ t μ' t' := h.steps.ksteps

theorem PStep.plug {root : Roots} {a b : Tm} (h : PStep root a b) {E : List Frame} (hE : OkE E) :
    PStep root (Flick.plug E a) (Flick.plug E b) := by
  cases h with
  | head hE' hh =>
    have := PStep.head (root := root) (hE'.append' hE) hh
    rwa [plug_append, plug_append] at this
  | glob hE' hg =>
    have := PStep.glob (root := root) (hE'.append' hE) hg
    rwa [plug_append, plug_append] at this

theorem HSeq.ksteps {root : Roots} {μ μ' : List Tm} {t t' : Tm}
    (h : HSeq root μ t μ' t') {E : List Frame} (hE : OkE E) :
    KSteps root μ (plug E t) μ' (plug E t') := by
  induction h with
  | refl => exact .refl
  | rt hh _ ih => exact .step (fun Λ => ⟨_, .rt hE (hh Λ)⟩) ih
  | pure hh _ ih => exact .step (fun _ => ⟨_, .pure hE hh⟩) ih
  | pstep hp _ ih => exact .step (fun Λ => ⟨_, (PStep.plug hp hE).step Λ _⟩) ih

/-- At least one step of a thread that keeps the regions. -/
def KPos (root : Roots) (μ : List Tm) (t : Tm) (μ' : List Tm) (t' : Tm) : Prop :=
  ∃ μ₁ t₁, (∀ Λ, ∃ l, Step root l Λ μ t (.cfg Λ μ₁ t₁)) ∧ KSteps root μ₁ t₁ μ' t'

theorem KPos.ksteps {root : Roots} {μ μ' : List Tm} {t t' : Tm} (h : KPos root μ t μ' t') :
    KSteps root μ t μ' t' :=
  let ⟨_, _, s, k⟩ := h; .step s k

theorem KSteps.pos {root : Roots} {μ μ₁ μ' : List Tm} {t t₁ t' : Tm} (h₁ : KSteps root μ t μ₁ t₁)
    (h₂ : KPos root μ₁ t₁ μ' t') : KPos root μ t μ' t' := by
  cases h₁ with
  | refl => exact h₂
  | step s k => exact ⟨_, _, s, k.trans h₂.ksteps⟩

theorem KPos.trans {root : Roots} {μ μ₁ μ' : List Tm} {t t₁ t' : Tm} (h₁ : KPos root μ t μ₁ t₁)
    (h₂ : KSteps root μ₁ t₁ μ' t') : KPos root μ t μ' t' :=
  let ⟨_, _, s, k⟩ := h₁; ⟨_, _, s, k.trans h₂⟩

/-- Steps that keep the regions: none, or at least one. -/
theorem KSteps.split {root : Roots} {μ μ' : List Tm} {t t' : Tm} (h : KSteps root μ t μ' t') :
    (μ' = μ ∧ t' = t) ∨ KPos root μ t μ' t' := by
  cases h with
  | refl => exact .inl ⟨rfl, rfl⟩
  | step s k => exact .inr ⟨_, _, s, k⟩

theorem HSeqP.kpos {root : Roots} {μ μ' : List Tm} {t t' : Tm} (hR : W.R = root)
    (h : HSeqP W μ t μ' t') {E : List Frame} (hE : OkE E) :
    KPos root μ (plug E t) μ' (plug E t') ∨ (W.B ∧ μ' = μ ∧ t' = t) := by
  rcases h with ⟨_, _, hh, k⟩ | ⟨_, hh, k⟩ | h
  · exact .inl ⟨_, _, fun Λ => ⟨_, .rt hE (hh Λ)⟩, (hR ▸ k).ksteps hE⟩
  · exact .inl ⟨_, _, fun _ => ⟨_, .pure hE hh⟩, (hR ▸ k).ksteps hE⟩
  · exact .inr h

/-- The answer of the right side to a step of a thread: steps that keep the regions, then at most
one step. -/
def TAns (root : Roots) (Λ : List Rgn) (μ : List Tm) (t : Tm) (Λ' : List Rgn) (μ' : List Tm)
    (t' : Tm) : Prop :=
  ∃ μ₁ t₁, KSteps root μ t μ₁ t₁ ∧
    ((Λ' = Λ ∧ μ' = μ₁ ∧ t' = t₁) ∨ ∃ l, Step root l Λ μ₁ t₁ (.cfg Λ' μ' t'))

end Flick.Equiv
