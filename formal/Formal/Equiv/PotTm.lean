import Formal.Subst
import Formal.EvalCtx

/-!
# Contextual equivalence: the forms a step of one side alone consumes

A step of one side that the other side does not take makes a lazy cell, binds it, forces it or
settles it (`Formal/Equiv/Pot.lean`, `Formal/Equiv/CatchUp.lean`). `Tm.pot` counts these forms; each
such step lowers it.
-/

namespace Flick.Equiv

/-- The forms a step of the left side alone consumes: a lazy cell to make, a name to bind, a cell
to force and the frame of a cell to settle. -/
def _root_.Flick.Tm.pot : Tm → Nat
  | .var _ | .lit _ | .loc _ | .const _ | .ctor _ | .rnil | .anil | .todo | .cap _ | .own _
  | .glob _ | .kont _ _ | .hole _ | .lam _ | .fix _ | .task _ | .drec _ _ => 0
  | .await b | .raise b | .scope b | .proj b _
  | .inside _ b | .hdone _ b | .unw _ _ b => b.pot
  | .lazy b => b.pot + 1
  | .force b => b.pot + 2
  | .settle _ b => b.pot + 1
  | .let_ a b => a.pot + b.pot + 1
  | .app a b | .letrec a b | .rcons _ a b | .update a _ b | .match_ a b | .acons _ a b
  | .try_ a b | .handle a b | .hres _ _ a b | .hkeep _ _ _ a b => a.pot + b.pot
  | .aguard _ a b c | .hdl _ a b c | .hwith _ _ _ a b c | .hkarm _ _ _ a b c =>
    a.pot + b.pot + c.pot
  | .hctl _ _ _ _ _ a b c d => a.pot + b.pot + c.pot + d.pot

theorem _root_.Flick.Value.pot_zero {v : Tm} (hv : Value v) : v.pot = 0 := by
  induction hv with
  | rcons _ _ ih₁ ih₂ => simp [Tm.pot, ih₁, ih₂]
  | ctorApp _ _ _ ih₁ ih₂ => simp [Tm.pot, ih₁, ih₂]
  | constApp _ _ ih => simp [Tm.pot, ih]
  | _ => rfl

theorem _root_.Flick.Tm.pot_rename : ∀ (e : Tm) (ξ : Nat → Nat), (e.rename ξ).pot = e.pot := by
  intro e
  induction e <;> intro ξ <;> simp_all [Tm.rename, Tm.pot]

theorem _root_.Flick.Tm.pot_subst_zero : ∀ (e : Tm) (σ : Nat → Tm),
    (∀ i, (σ i).pot = 0) → (e.subst σ).pot = e.pot := by
  have lift : ∀ {n : Nat} {σ : Nat → Tm}, (∀ i, (σ i).pot = 0) → ∀ i, (liftS n σ i).pot = 0 := by
    intro n σ h i; unfold liftS; split
    · rfl
    · rw [Tm.pot_rename]; exact h _
  intro e
  induction e <;> intro σ h <;> simp only [Tm.subst, Tm.pot] <;> simp_all [lift h]

theorem _root_.Flick.Tm.pot_subst_loc (e : Tm) (l : Nat) : (e.subst (inst [.loc l])).pot = e.pot :=
  Tm.pot_subst_zero e (inst [.loc l]) fun i => by cases i <;> rfl

theorem _root_.Flick.Frame.pot_plug (F : Frame) (x y : Tm) :
    (F.plug x).pot + y.pot = (F.plug y).pot + x.pot := by
  cases F <;> simp only [Frame.plug, Tm.pot] <;> omega

theorem pot_plug : ∀ (E : List Frame) (x y : Tm), (plug E x).pot + y.pot = (plug E y).pot + x.pot
  | [], x, y => by simp only [plug]; omega
  | F :: E, x, y => by
    simp only [plug]
    have h₁ := pot_plug E (F.plug x) (F.plug y)
    have h₂ := F.pot_plug x y
    omega

theorem pot_plug_lt (E : List Frame) {x y : Tm} (h : x.pot < y.pot) :
    (plug E x).pot < (plug E y).pot := by
  have := pot_plug E x y; omega

end Flick.Equiv
