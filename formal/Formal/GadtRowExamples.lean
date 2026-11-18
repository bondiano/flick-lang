import Formal.IndexedRows

/-!
# GADT row equations constrain all residuals

These are consequences of the existing type congruence, not a claim that
idempotent row equations have one most general unifier. A named element keeps its
argument in what a row stands for (`Ty.asRow`).
-/

namespace Flick.GadtRows

/-- A repeated index `r = {S args | r}` proves the operation's membership. -/
theorem member_of_absorption (r : Ty) (N : Name) (args : Ty)
    (h : Ty.Equiv r (.run (.rnm N args) r)) :
    r.asRow (.nm N args.classOf) := by
  have held : (Ty.run (.rnm N args) r).asRow (.nm N args.classOf) :=
    .inl (.inl rfl)
  exact Eq.mpr (congrFun h.asRow _) held

/-- A hidden task's row is included for every residual satisfying the index. -/
theorem residual_included (known residual : Ty)
    (h : Ty.Equiv (.run known residual) known) :
    ∀ x, residual.asRow x → known.asRow x := by
  intro x held
  exact Eq.mp (congrFun h.asRow x) (Or.inr held)

/-- Subtraction learned by a pattern excludes the named service. -/
theorem subtraction_excludes (r : Ty) (G : Name → Prop) (N : Name) (args : TyClass)
    (gone : G N) (h : Ty.Equiv r (.rless G r)) :
    ¬ r.asRow (.nm N args) := by
  intro held
  have after := Eq.mp (congrFun h.asRow (.nm N args)) held
  exact after.1 gone

/-- Capture-index equality transports the permitted captures, without
identifying any distinct region atoms. -/
theorem captures_transport (C D : Caps) (h : Ty.Equiv (.caps C) (.caps D)) :
    ∀ a, C a → D a := by
  intro a held
  exact Eq.mp (congrFun h.reach a) held

private def db : Ty := .rnm "Db" (.adt "Db")

/-- A later tuple column cannot require an empty row after an earlier
constructor has proved that the same index contains Db. -/
example (r : Ty) (required : Ty.Equiv r (.run db r)) : ¬ Ty.Equiv r .cnil := by
  intro empty
  have held := member_of_absorption r "Db" (.adt "Db") required
  exact Eq.mp (congrFun empty.asRow (.nm "Db" (Ty.adt "Db").classOf)) held

/-- The same index equation permits an empty residual. -/
example : Ty.Equiv (.run db .cnil) db := .run_nil db (.rnm _ _)

/-- It also permits a nonempty residual. -/
example : Ty.Equiv (.run db db) db := .run_idem db (.rnm _ _)

/-- Choosing the least residual would discard a genuine solution. -/
example : ¬ (∀ residual, Ty.Equiv (.run db residual) db → Ty.Equiv residual .cnil) := by
  intro least
  have h := least db (.run_idem db (.rnm _ _))
  have held : db.asRow (.nm "Db" (Ty.adt "Db").classOf) := .inl rfl
  have absent := Eq.mp (congrFun h.asRow (.nm "Db" (Ty.adt "Db").classOf)) held
  exact absent

/-- Every substitution solving a repeated row index must retain its effect. -/
example (r : Ty) : ∀ θ, Ty.Equiv (r.substT θ) (.run db (r.substT θ)) →
    (r.substT θ).asRow (.nm "Db" (Ty.adt "Db").classOf) := by
  intro θ h
  exact member_of_absorption _ _ _ h

/-- Equal membership names alone never equate different service arguments. -/
example : ¬ Ty.Equiv (.rnm "Store" (.base .int)) (.rnm "Store" (.base .bool)) :=
  Ty.rnm_int_not_bool "Store"

end Flick.GadtRows
