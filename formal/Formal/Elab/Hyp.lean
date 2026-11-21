import Formal.Elab.Variance

/-!
# Elaboration: K1'–K8 and the simulation hypothesis (elaboration.md §4.1)

K1'–K8 are hypotheses on the program (`Coherent`), as the checker establishes them. Only
those T6(b) uses are fields:

- K1' strengthens K1 to variance-directed resolution (B1): two instances of one class whose
  instantiated heads can serve a common query `Q` (`Cn.Serves`) belong to one instance,
  guaranteeing unique resolution independent of search order and metavariable refinement;
- `super_serves` (derived from `Prog.ClsVarOk` via `Prog.ClsVarOk.super_serves`): class
  parameter variances respect superclass projection;
- K5 is a well-founded order on constraints under which an instance's context is below any
  query the head serves (`k5inst`), two instantiations of the same instance serving a common
  query have context constraints serving common queries below it (`k5mono`), a superclass is
  below its class (`k5super`), and a record's field is below the record (`k5fields`);
- K6 keeps the compiler's classes apart from the program's instances (`k6prim`, `k6struct`,
  `primVar`, `fields_serves`, `fields_sub`);
- K7 gives each superclass slot of an instance a derivation from its context.

Not fields: K2 and K3 only make (Improve) order-independent, and the model starts from
improved constraints; K4 gives K1' for the whole `𝓘ₚ`, which `Coherent` assumes directly; K8
makes premise names fresh, which the model's numbered premises are by construction.

`Sim` is the simulation "≈ in dictionary positions ⇒ same result", which needs the target's
semantics; `Target.sim` (`Run.lean`) derives it from the steps of a `Target`.
-/

namespace Flick.Elab

/-- Pointwise relation of two lists. -/
inductive All2 {α β : Type} (R : α → β → Prop) : List α → List β → Prop
  | nil : All2 R [] []
  | cons {a b as bs} : R a b → All2 R as bs → All2 R (a :: as) (b :: bs)

/-- K1'–K8; `lt` is the order of K5. -/
structure Coherent (P : Prog) (lt : Cn → Cn → Prop) : Prop where
  /-- K1': no overlap under variance-directed matching (B1): two instances of one class that
  can serve a common query under some substitutions must be the same instance. -/
  k1' : ∀ {I J ins jns σ σ' Q}, P.insts I = some ins → P.insts J = some jns →
    Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ)⟩ →
    Cn.Serves P Q ⟨jns.cls, jns.head.map (·.subst σ')⟩ → I = J
  /-- Class parameter variances respect superclasses (`Prog.ClsVarOk.super_serves`). -/
  super_serves : ∀ {Q H i SQ SH}, Cn.Serves P Q H →
    P.super Q i = some SQ → P.super H i = some SH → Cn.Serves P SQ SH
  /-- K5: termination, as a well-founded transitive order -/
  lt_wf : WellFounded lt
  lt_trans : ∀ {a b c}, lt a b → lt b c → lt a c
  /-- K5 (B1): every context constraint of an instance serving `Q` is strictly below `Q`. -/
  k5inst : ∀ {I ins c σ Q}, P.insts I = some ins → c ∈ ins.ctx →
    Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ)⟩ → lt (c.subst σ) Q
  /-- K5 (monotone contexts): if two instantiations of one instance both serve a common
  query `Q`, each context constraint serves a common query strictly below `Q`. -/
  k5mono : ∀ {I ins σ₁ σ₂ Q}, P.insts I = some ins →
    Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ₁)⟩ →
    Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ₂)⟩ →
    ∀ c ∈ ins.ctx, ∃ q, lt q Q ∧ Cn.Serves P q (c.subst σ₁) ∧ Cn.Serves P q (c.subst σ₂)
  k5super : ∀ {Q i S}, P.super Q i = some S → lt S Q
  k5fields : ∀ {C τ ts t}, P.structC C = true → P.fields τ = some ts → t ∈ ts →
    lt ⟨C, [t]⟩ ⟨C, [τ]⟩
  /-- K6: the compiler's classes -/
  k6prim : ∀ {C I ins}, P.primC C = true → P.insts I = some ins → ins.cls ≠ C
  k6struct : ∀ {C τ ts I ins σ Q}, P.structC C = true → P.fields τ = some ts →
    P.insts I = some ins → Cn.Serves P Q ⟨C, [τ]⟩ →
    Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ)⟩ → False
  primStruct : ∀ {C}, P.primC C = true → P.structC C = false
  primSupers : ∀ {C}, P.primC C = true → P.supers C = []
  primVar : ∀ {C}, P.primC C = true → ∀ j, P.clsVar C j = .inv
  structSupers : ∀ {C : Nat} {i : Nat} {S : Cn}, P.structC C = true → (P.supers C)[i]? = some S →
    P.structC S.cls = true ∧ S.args = [Ty.var 0]
  /-- Structural classes respect variance on record fields: two records serving a common
  query `C τ` have matching fields serving common queries strictly below `C τ`. -/
  fields_serves : ∀ {C τ τ₁ τ₂ ts₁ ts₂}, P.structC C = true →
    P.fields τ₁ = some ts₁ → P.fields τ₂ = some ts₂ →
    Cn.Serves P ⟨C, [τ]⟩ ⟨C, [τ₁]⟩ → Cn.Serves P ⟨C, [τ]⟩ ⟨C, [τ₂]⟩ →
    ts₁.length = ts₂.length ∧
    ∀ (j : Nat) (t₁ t₂ : Ty), ts₁[j]? = some t₁ → ts₂[j]? = some t₂ →
      ∃ q, lt q ⟨C, [τ]⟩ ∧ Cn.Serves P q ⟨C, [t₁]⟩ ∧ Cn.Serves P q ⟨C, [t₂]⟩
  /-- Structural fields stay structural when `C τ_H` serves `C τ_Q`. -/
  fields_sub : ∀ {C τ_Q τ_H ts_H}, P.structC C = true → P.fields τ_H = some ts_H →
    Cn.Serves P ⟨C, [τ_Q]⟩ ⟨C, [τ_H]⟩ →
    ∃ ts_Q, P.fields τ_Q = some ts_Q ∧ ts_Q.length = ts_H.length ∧
      ∀ (j : Nat) (t_Q t_H : Ty), ts_Q[j]? = some t_Q → ts_H[j]? = some t_H →
        Cn.Serves P ⟨C, [t_Q]⟩ ⟨C, [t_H]⟩
  /-- K7: the slots of an instance -/
  k7 : ∀ {I : Nat} {ins : Inst} {i : Nat} {S : Cn}, P.insts I = some ins → P.super ⟨ins.cls, ins.head⟩ i = some S →
    ∃ s, ins.slots[i]? = some s ∧ Ent P ins.ctx [S] [s]
  /-- (Struct) and (Prim) see through type variables -/
  fields_subst : ∀ {τ ts θ}, P.fields τ = some ts → P.fields (τ.subst θ) = some (ts.map (·.subst θ))
  prims_subst : ∀ {Q p θ}, P.prims Q = some p → P.prims (Q.subst θ) = some p

/-- K1' strictly generalizes the exact-equality K1 condition. -/
theorem Coherent.k1 {P : Prog} {lt : Cn → Cn → Prop} (h : Coherent P lt)
    {I J ins jns σ σ'} (hI : P.insts I = some ins) (hJ : P.insts J = some jns)
    (hc : ins.cls = jns.cls) (hh : ins.head.map (·.subst σ) = jns.head.map (·.subst σ')) :
    I = J :=
  h.k1' (Q := ⟨ins.cls, ins.head.map (·.subst σ)⟩) (σ := σ) (σ' := σ') hI hJ
    (.refl _) (.of_eq (by simp [hc, hh]))

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
