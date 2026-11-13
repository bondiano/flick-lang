import Formal.IndexedRows

/-!
# GADT row equations constrain all residuals

These are consequences of the existing type congruence, not a claim that
idempotent row equations have one most general unifier. Named arguments stay
in the indexed projection; the legacy parameter-free projection is insufficient.
-/

namespace Flick.GadtRows

/-- A repeated index `r = {S args | r}` proves the operation's membership. -/
theorem member_of_absorption (r : Ty) (N : Name) (args : Ty)
    (h : Ty.Equiv r (.run (.rnm N args) r)) :
    r.asIndexedRow (.nm N args.classOf) := by
  have held : (Ty.run (.rnm N args) r).asIndexedRow (.nm N args.classOf) :=
    .inl (.inl rfl)
  exact Eq.mpr (congrFun h.indexedRow _) held

/-- A hidden task's row is included for every residual satisfying the index. -/
theorem residual_included (known residual : Ty)
    (h : Ty.Equiv (.run known residual) known) :
    ∀ x, residual.asIndexedRow x → known.asIndexedRow x := by
  intro x held
  exact Eq.mp (congrFun h.indexedRow x) (Or.inr held)

/-- Subtraction learned by a pattern excludes the named service. -/
theorem subtraction_excludes (r : Ty) (G : Name → Prop) (N : Name) (args : TyClass)
    (gone : G N) (h : Ty.Equiv r (.rless G r)) :
    ¬ r.asIndexedRow (.nm N args) := by
  intro held
  have after := Eq.mp (congrFun h.indexedRow (.nm N args)) held
  exact after.1 gone

/-- Capture-index equality transports the permitted captures, without
identifying any distinct region atoms. -/
theorem captures_transport (C D : Caps) (h : Ty.Equiv (.caps C) (.caps D)) :
    ∀ a, C a → D a := by
  intro a held
  exact Eq.mp (congrFun h.reach a) held

private def db : Ty := .rnm "Db" .rnil

/-- The same index equation permits an empty residual. -/
example : Ty.Equiv (.run db .cnil) db := .run_nil db (.rnm _ _)

/-- It also permits a nonempty residual. -/
example : Ty.Equiv (.run db db) db := .run_idem db (.rnm _ _)

/-- Choosing the least residual would discard a genuine solution. -/
example : ¬ (∀ residual, Ty.Equiv (.run db residual) db → Ty.Equiv residual .cnil) := by
  intro least
  have h := least db (.run_idem db (.rnm _ _))
  have held : db.asIndexedRow (.nm "Db" Ty.rnil.classOf) := .inl rfl
  have absent := Eq.mp (congrFun h.indexedRow (.nm "Db" Ty.rnil.classOf)) held
  exact absent

/-- Every substitution solving a repeated row index must retain its effect. -/
example (r : Ty) : ∀ θ, Ty.Equiv (r.substT θ) (.run db (r.substT θ)) →
    (r.substT θ).asIndexedRow (.nm "Db" Ty.rnil.classOf) := by
  intro θ h
  exact member_of_absorption _ _ _ h

/-- Equal membership names alone never equate different service arguments. -/
example : ¬ Ty.Equiv (.rnm "Store" (.base .int)) (.rnm "Store" (.base .bool)) :=
  Ty.rnm_int_not_bool "Store"

end Flick.GadtRows
