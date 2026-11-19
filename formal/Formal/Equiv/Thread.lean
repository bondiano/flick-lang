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

theorem HSeq.ksteps {root : Roots} {μ μ' : List Tm} {t t' : Tm}
    (h : HSeq μ t μ' t') {E : List Frame} (hE : OkE E) :
    KSteps root μ (plug E t) μ' (plug E t') := by
  induction h with
  | refl => exact .refl
  | rt hh _ ih => exact .step (fun Λ => ⟨_, .rt hE (hh Λ)⟩) ih
  | pure hh _ ih => exact .step (fun _ => ⟨_, .pure hE hh⟩) ih

/-- The answer of the right side to a step of a thread: steps that keep the regions, then at most
one step. -/
def TAns (root : Roots) (Λ : List Rgn) (μ : List Tm) (t : Tm) (Λ' : List Rgn) (μ' : List Tm)
    (t' : Tm) : Prop :=
  ∃ μ₁ t₁, KSteps root μ t μ₁ t₁ ∧
    ((Λ' = Λ ∧ μ' = μ₁ ∧ t' = t₁) ∨ ∃ l, Step root l Λ μ₁ t₁ (.cfg Λ' μ' t'))

end Flick.Equiv
