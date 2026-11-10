import Formal.Typing

/-!
# Typing by use: the stage of a use and subtyping

A value is typed by use (`Rules.lean`): a region is needed live where a value is used, at
the stage `Stage.use φ C` of a call of an arrow qualified `C`, not where it is held.
-/

namespace Flick

/-- `Live C` uses the same row atoms as `Scope s`; no new row element is needed.
A pure computation does not use a region, even when its value holds one. -/
def Stage.use : Stage → Caps → Stage
  | .pure, _ => .pure
  | .rt r ε, C => .rt (Row.union r (Row.ofCaps C)) ε

/-- Subtyping compares the stages at which qualified values are used, so it cannot hide a
qualifier in an argument or a result type. -/
inductive LeLive : Ty → Ty → Prop
  | refl : LeLive t t
  | arrow : LeLive a' a →
      (Stage.use φ C).le (Stage.use φ' C') → LeLive t t' →
      Caps.Sub (Ty.arrow a φ t C).reach (Ty.arrow a' φ' t' C').reach →
      LeLive (.arrow a φ t C) (.arrow a' φ' t' C')
  | eff : (Stage.use (.rt r e) C).le (Stage.use (.rt r' e') C') →
      LeLive t t' →
      Caps.Sub (Ty.eff r e t C).reach (Ty.eff r' e' t' C').reach →
      LeLive (.eff r e t C) (.eff r' e' t' C')

/-- Polymorphic instantiation uses live subtyping at its leaves. -/
inductive LeLiveI : Ty → Ty → Prop
  | le : LeLive a b → LeLiveI a b
  | inst {us : List Ty} : us.length = k → LeLiveI (t.substT (instT us)) T → LeLiveI (.all k t) T

theorem LeLiveI.refl : LeLiveI t t := .le .refl

/-- Subtyping cannot erase the stage needed to call a qualified function. -/
theorem LeLive.arrow_use {a ψ t C a' ψ' t' C'}
    (h : LeLive (.arrow a ψ t C) (.arrow a' ψ' t' C')) :
    (Stage.use ψ C).le (Stage.use ψ' C') := by
  cases h with
  | refl =>
    cases ψ with
    | pure => trivial
    | rt _ _ => exact ⟨fun _ h => h, fun _ h => h⟩
  | arrow _ hψ _ _ => exact hψ

/-- The same obligation is kept when a qualified task is widened. -/
theorem LeLive.eff_use {r ε t C r' ε' t' C'}
    (h : LeLive (.eff r ε t C) (.eff r' ε' t' C')) :
    (Stage.use (.rt r ε) C).le (Stage.use (.rt r' ε') C') := by
  cases h with
  | refl => exact ⟨fun _ h => h, fun _ h => h⟩
  | eff hψ _ _ => exact hψ

end Flick
