import Formal.Elab.Syntax

/-!
# Elaboration: K1–K8 and the simulation hypothesis (elaboration.md §4.1)

K1–K8 are hypotheses on the program (`Coherent`), as the checker establishes them. Only
those T6(b) uses are fields:

- K1 is stated semantically: two heads of one class that some substitutions make equal
  belong to one instance (no first-order unifier exists otherwise);
- K5 is a well-founded order on constraints under which an instance's context is below its
  head, a superclass below its class, and a record's field below the record — plus "the
  variables of the context are in the head";
- K6 keeps the compiler's classes apart from the program's instances;
- K7 gives each superclass slot of an instance a derivation from its context.

Not fields: K2 and K3 only make (Improve) order-independent, and the model starts from
improved constraints; K4 gives K1 for the whole `𝓘ₚ`, which `Coherent` assumes directly; K8
makes premise names fresh, which the model's numbered premises are by construction.

`Sim` is the simulation "≈ in dictionary positions ⇒ same result", which needs the target's
semantics; `Target.sim` (`Run.lean`) derives it from the steps of a `Target`.
-/

namespace Flick.Elab

/-- Pointwise relation of two lists. -/
inductive All2 {α β : Type} (R : α → β → Prop) : List α → List β → Prop
  | nil : All2 R [] []
  | cons {a b as bs} : R a b → All2 R as bs → All2 R (a :: as) (b :: bs)

/-- K1–K8; `lt` is the order of K5. -/
structure Coherent (P : Prog) (lt : Cn → Cn → Prop) : Prop where
  /-- K1: no overlap -/
  k1 : ∀ {I J ins jns σ σ'}, P.insts I = some ins → P.insts J = some jns → ins.cls = jns.cls →
    ins.head.map (·.subst σ) = jns.head.map (·.subst σ') → I = J
  /-- K5: the variables of a context are in its head -/
  k5vars : ∀ {I ins c t n}, P.insts I = some ins → c ∈ ins.ctx → t ∈ c.args → t.Occurs n →
    ∃ h ∈ ins.head, h.Occurs n
  /-- K5: termination, as a well-founded transitive order -/
  lt_wf : WellFounded lt
  lt_trans : ∀ {a b c}, lt a b → lt b c → lt a c
  k5inst : ∀ {I ins c σ}, P.insts I = some ins → c ∈ ins.ctx →
    lt (c.subst σ) ⟨ins.cls, ins.head.map (·.subst σ)⟩
  k5super : ∀ {Q i S}, P.super Q i = some S → lt S Q
  k5fields : ∀ {C τ ts t}, P.structC C = true → P.fields τ = some ts → t ∈ ts →
    lt ⟨C, [t]⟩ ⟨C, [τ]⟩
  /-- K6: the compiler's classes -/
  k6prim : ∀ {C I ins}, P.primC C = true → P.insts I = some ins → ins.cls ≠ C
  k6struct : ∀ {C τ ts I ins σ}, P.structC C = true → P.fields τ = some ts →
    P.insts I = some ins → ins.cls = C → [τ] ≠ ins.head.map (·.subst σ)
  primStruct : ∀ {C}, P.primC C = true → P.structC C = false
  primSupers : ∀ {C}, P.primC C = true → P.supers C = []
  structSupers : ∀ {C : Nat} {i : Nat} {S : Cn}, P.structC C = true → (P.supers C)[i]? = some S →
    P.structC S.cls = true ∧ S.args = [Ty.var 0]
  /-- K7: the slots of an instance -/
  k7 : ∀ {I : Nat} {ins : Inst} {i : Nat} {S : Cn}, P.insts I = some ins → P.super ⟨ins.cls, ins.head⟩ i = some S →
    ∃ s, ins.slots[i]? = some s ∧ Ent P ins.ctx [S] [s]
  /-- (Struct) and (Prim) see through type variables -/
  fields_subst : ∀ {τ ts θ}, P.fields τ = some ts → P.fields (τ.subst θ) = some (ts.map (·.subst θ))
  prims_subst : ∀ {Q p θ}, P.prims Q = some p → P.prims (Q.subst θ) = some p

/-- An elaborated expression as a skeleton with holes for dictionaries, and what running it
gives. `Ok s ds` says the filling is one the simulation speaks of: for a target, a closed
term typed at the skeleton's type. -/
structure Exprs where
  Skel : Type
  Out : Type
  Ok : Skel → List Dict → Prop
  run : Skel → List Dict → Out

/-- The simulation of T6(b): filling the holes with `≈` dictionaries gives the same result.
It needs the semantics of λFlick⁺; `Target.sim` derives it from the steps of a target. -/
def Sim (P : Prog) (X : Exprs) : Prop :=
  ∀ s ds ds', All2 (Eqv P) ds ds' → X.Ok s ds → X.Ok s ds' → X.run s ds = X.run s ds'

end Flick.Elab
