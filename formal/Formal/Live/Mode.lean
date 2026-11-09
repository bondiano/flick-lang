import Formal.Typing

/-!
# Typing modes: capture discipline and typing by use

`TypM` (`Rules.lean`) is parameterized by a mode. The capture mode keeps the
original relations; the live mode needs a region where a value is used.
-/

namespace Flick

/-- `Live C` uses the same row atoms as `Scope s`; no new row element is needed.
A pure computation does not use a region, even when its value holds one. -/
def TypingMode.use : TypingMode → Stage → Caps → Stage
  | .captures, φ, _ => φ
  | .live, .pure, _ => .pure
  | .live, .rt r ε, C => .rt (Row.union r (Row.ofCaps C)) ε

/-- Captures account for everything held by a value. Live accounts for use in its
stage instead, so merely holding a value imposes no liveness obligation. -/
def TypingMode.held : TypingMode → Caps → Caps → Prop
  | .captures, C, D => Caps.Sub C D
  | .live, _, _ => True

/-- Subtyping for live compares the stages at which qualified values are used.
The captures relation alone may hide a qualifier in an argument or a result type. -/
inductive LeLive : Ty → Ty → Prop
  | refl : LeLive t t
  | arrow : LeLive a' a →
      (TypingMode.live.use φ C).le (TypingMode.live.use φ' C') → LeLive t t' →
      Caps.Sub (Ty.arrow a φ t C).reach (Ty.arrow a' φ' t' C').reach →
      LeLive (.arrow a φ t C) (.arrow a' φ' t' C')
  | eff : (TypingMode.live.use (.rt r e) C).le (TypingMode.live.use (.rt r' e') C') →
      LeLive t t' →
      Caps.Sub (Ty.eff r e t C).reach (Ty.eff r' e' t' C').reach →
      LeLive (.eff r e t C) (.eff r' e' t' C')

/-- Polymorphic instantiation uses live subtyping at its leaves. -/
inductive LeLiveI : Ty → Ty → Prop
  | le : LeLive a b → LeLiveI a b
  | inst {us : List Ty} : us.length = k → LeLiveI (t.substT (instT us)) T → LeLiveI (.all k t) T

theorem LeLiveI.refl : LeLiveI t t := .le .refl

def TypingMode.subtype : TypingMode → Ty → Ty → Prop
  | .captures => Le
  | .live => LeLive

def TypingMode.instSubtype : TypingMode → Ty → Ty → Prop
  | .captures => LeI
  | .live => LeLiveI

/-- Subtyping cannot erase the stage needed to call a qualified function. -/
theorem LeLive.arrow_use {a ψ t C a' ψ' t' C'}
    (h : LeLive (.arrow a ψ t C) (.arrow a' ψ' t' C')) :
    (TypingMode.live.use ψ C).le (TypingMode.live.use ψ' C') := by
  cases h with
  | refl =>
    cases ψ with
    | pure => trivial
    | rt _ _ => exact ⟨fun _ h => h, fun _ h => h⟩
  | arrow _ hψ _ _ => exact hψ

/-- The same obligation is kept when a qualified task is widened. -/
theorem LeLive.eff_use {r ε t C r' ε' t' C'}
    (h : LeLive (.eff r ε t C) (.eff r' ε' t' C')) :
    (TypingMode.live.use (.rt r ε) C).le (TypingMode.live.use (.rt r' ε') C') := by
  cases h with
  | refl => exact ⟨fun _ h => h, fun _ h => h⟩
  | eff hψ _ _ => exact hψ

end Flick
