import Formal.TyOps

/-!
# Row and stage terms: what they stand for

The row terms keep their arguments under renaming and substitution, and what they stand
for (`Ty.asRow`, `Ty.asStage`) keeps them too: a named element is its name at its argument
up to congruence. Subtraction removes a name at every argument.
-/

namespace Flick

@[simp] theorem Ty.asStage_pure : Ty.pure.asStage = .pure := rfl

@[simp] theorem Ty.asStage_rt (r e : Ty) : (Ty.rt r e).asStage = .rt r.asRow e.asRow := rfl

example : (Ty.base .int).asStage = .pure := rfl

/-- Union is extensional at the predicate boundary, even though its syntax
records the order of its operands. -/
theorem Ty.run_asRow_comm (r s : Ty) : (Ty.run r s).asRow = (Ty.run s r).asRow := by
  funext x; simp [Ty.asRow, Row.union, or_comm]

theorem Ty.run_asRow_idem (r : Ty) : (Ty.run r r).asRow = r.asRow := by
  funext x; simp [Ty.asRow, Row.union]

/-- `Store α`, the named element of the service `Store` at the argument `α`. -/
def storeOf (a : Ty) : Ty := .rnm "Store" (.tapp (.adt "Store") a)

/-- Instantiating a row element keeps its type argument, rather than only the
argument's reach or its name. -/
example : (storeOf (.tvar 0)).substT (instT [.base .int]) = storeOf (.base .int) := by
  simp [storeOf, Ty.substT, instT]

example : storeOf (.base .int) ≠ storeOf (.base .bool) := by
  intro h; cases h

/-- A binder in an element's argument is protected by lifted substitution. -/
example : (Ty.rnm "Store" (.all 1 (.tvar 0))).substT (instT [.base .int]) =
    .rnm "Store" (.all 1 (.tvar 0)) := by
  simp [Ty.substT, liftT]

/-- Substitution into an open row still excludes a forbidden capability, at every argument. -/
example (c : TyClass) : ¬ ((Ty.rless (fun n => n = "Store") (.tvar 0)).substT
    (instT [.run (storeOf (.base .int)) (storeOf (.base .bool))])).asRow (.nm "Store" c) := by
  simp [Ty.substT, instT, Ty.asRow, Row.less]

/-- A service not named by the subtraction is retained. -/
example : ((Ty.rless (fun n => n = "Db") (.tvar 0)).substT
    (instT [.rnm "Clock" (.adt "Clock")])).asRow (.nm "Clock" (Ty.adt "Clock").classOf) := by
  simp [Ty.substT, instT, Ty.asRow, Row.less, Row.union, Row.inst]

end Flick
