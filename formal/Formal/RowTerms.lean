import Formal.TyOps

/-!
# Syntactic rows and stages: the bridge to predicate rows

The row terms retain their arguments under renaming and substitution. Their
projection still uses the existing parameter-free `Elem`; switching the typing
rules and indexing `Elem` by the arguments are later parts of E11. These lemmas
do not establish the soundness of parameterized handlers.
-/

namespace Flick

/-- The predicate-stage interpretation of a syntactic stage. A non-stage
type has no interpretation, rather than being silently treated as pure. -/
def Ty.stage : Ty → Option Stage
  | .pure => some .pure
  | .rt r e => some (.rt r.asRow e.asRow)
  | _ => none

@[simp] theorem Ty.stage_pure : Ty.pure.stage = some .pure := rfl

@[simp] theorem Ty.stage_rt (r e : Ty) : (Ty.rt r e).stage = some (.rt r.asRow e.asRow) := rfl

theorem Ty.stage_rt_substT (σ : Atom → Ty) (r e : Ty) :
    ((Ty.rt r e).substT σ).stage = (Ty.rt r e).stage.map (Stage.substT σ) := by
  simp [Ty.substT, Ty.stage, Stage.substT, Ty.asRow_substT]

theorem Ty.stage_pure_substT (σ : Atom → Ty) :
    (Ty.pure.substT σ).stage = Ty.pure.stage.map (Stage.substT σ) := rfl

example : (Ty.base .int).stage = none := rfl

/-- Union is extensional at the predicate boundary, even though its syntax
records the order of its operands. -/
theorem Ty.run_asRow_comm (r s : Ty) : (Ty.run r s).asRow = (Ty.run s r).asRow := by
  funext x; simp [Ty.asRow, Row.union, or_comm]

theorem Ty.run_asRow_idem (r : Ty) : (Ty.run r r).asRow = r.asRow := by
  funext x; simp [Ty.asRow, Row.union]

/-- Instantiating a row element keeps its type argument, rather than only the
argument's reach or its name. -/
example : (Ty.rnm "Store" (.tvar 0)).substT (instT [.base .int]) =
    .rnm "Store" (.base .int) := by
  simp [Ty.substT, instT]

example : Ty.rnm "Store" (.base .int) ≠ Ty.rnm "Store" (.base .bool) := by
  intro h; cases h

/-- A binder in an element's argument is protected by lifted substitution. -/
example : (Ty.rnm "Store" (.all 1 (.tvar 0))).substT (instT [.base .int]) =
    .rnm "Store" (.all 1 (.tvar 0)) := by
  simp [Ty.substT, liftT]

/-- Substitution into an open row still excludes a forbidden capability. -/
example : ¬ ((Ty.rless (fun n => n = "Db") (.tvar 0)).substT
    (instT [.rnm "Db" .rnil])).asRow (.nm "Db") := by
  simp [Ty.substT, instT, Ty.asRow, Row.less]

/-- A service not named by the subtraction is retained. -/
example : ((Ty.rless (fun n => n = "Db") (.tvar 0)).substT
    (instT [.rnm "Clock" .rnil])).asRow (.nm "Clock") := by
  simp [Ty.substT, instT, Ty.asRow, Row.less, Row.union, Row.single]

end Flick
