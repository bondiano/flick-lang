import Formal.RuleRows

/-!
# A handler is installed where what indexes its effect is live

An effect indexed by a type `U`, as `Setup s` of a component (`Prelude.fl`), is the row
`Row.err E U = {E} ∪ ⌈U⌉` in the core: its name and the atoms of its index. A body whose
row has it may therefore use `⌈U⌉` as live, which is how the compiler and the model read
`Setup s` (`flick-core/src/types/regions/live_rows.rs`, `scope_live`). (Handle) gives no
new rule for that: it removes only the name `E` from the body's row, and every atom of the
body's stage stays in the stage where the handler is installed (`hdl_atm_out`). So a
handler of an effect that a region indexes is installed only where that region is live
(`hdl_err_live`), which the compiler and the model check as `live_install` and
`indexed_live`.
-/

namespace Flick

/-- The premises of (Handle) on the stages: an atom of the body's stage `φ₀`, which is
below `{E} ∪ r`, is in the stage `φ` of the handler, which is above `r`. -/
theorem hdl_atm_out {φ₀ φ : Stage} {E : Name} {r ε : Row}
    (h₀ : φ₀.le (.rt (Row.union (Row.single E) r) ε)) (h : (Stage.rt r ε).le φ)
    {a : Atom} (ha : φ₀.svc (.atm a)) : φ.svc (.atm a) := by
  cases φ₀ with
  | pure => exact ha.elim
  | rt r₀ e₀ =>
    cases φ with
    | pure => exact h.elim
    | rt r' e' =>
      simp only [Stage.le, Stage.svc] at h₀ h ha ⊢
      rcases h₀.1 _ ha with hE | hr
      · cases hE
      · exact h.1 _ hr

/-- A body whose row has the effect `E` indexed by `U` is handled at a stage that holds
the atoms of `⌈U⌉`: the region of `Setup s` is live where its handler is installed. -/
theorem hdl_err_live {φ₀ φ : Stage} {E : Name} {U : Ty} {r ε : Row}
    (hb : Row.Sub (Row.err E U) φ₀.svc)
    (h₀ : φ₀.le (.rt (Row.union (Row.single E) r) ε)) (h : (Stage.rt r ε).le φ) :
    Row.Sub (Row.ofCaps U.reach) φ.svc := by
  intro x hx
  cases x with
  | atm a => exact hdl_atm_out h₀ h (hb _ (Or.inr hx))
  | _ => exact hx.elim

/-- Rejected: a handler of an effect indexed by a region `a` is not installed at a stage
without `a`, as one of a closure that leaves the `scope` of `a`. -/
example {φ₀ : Stage} {E : Name} {U : Ty} {r ε : Row} {a : Atom} (hU : U.reach a)
    (hb : Row.Sub (Row.err E U) φ₀.svc)
    (h₀ : φ₀.le (.rt (Row.union (Row.single E) r) ε)) :
    ¬ (Stage.rt r ε).le (.rt Row.none Row.none) :=
  fun h => hdl_err_live hb h₀ h (.atm a) hU

end Flick
