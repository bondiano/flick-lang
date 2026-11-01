/-!
# Elaboration: classes, instances, dictionaries, entailment (elaboration.md §1.1, §2.1–§2.3)

A model of its own, like `Formal/RT/`: it imports nothing from the core of P1. Constraint
types are first-order terms; a dictionary term is `d ::= δ | d.$slot | I d̄ | struct_C d̄ |
prim_Q ℓ̄`, with application as a binary spine.
-/

namespace Flick.Elab

/-- Types inside constraints: variables, constructors, application. -/
inductive Ty where
  | var (n : Nat)
  | con (c : Nat)
  | app (t u : Ty)
  deriving DecidableEq

def Ty.subst (σ : Nat → Ty) : Ty → Ty
  | .var n => σ n
  | .con c => .con c
  | .app t u => .app (t.subst σ) (u.subst σ)

/-- `n` occurs in `t`. -/
def Ty.Occurs (n : Nat) : Ty → Prop
  | .var m => m = n
  | .con _ => False
  | .app t u => t.Occurs n ∨ u.Occurs n

/-- A ground type has no variables. -/
def Ty.Ground (t : Ty) : Prop := ∀ n, ¬ t.Occurs n

/-- A constraint `C τ̄`. -/
structure Cn where
  cls : Nat
  args : List Ty
  deriving DecidableEq

def Cn.subst (σ : Nat → Ty) (q : Cn) : Cn := ⟨q.cls, q.args.map (·.subst σ)⟩

/-- The substitution of a class's parameters `ᾱ := τ̄` (parameters are `var 0 … var (k-1)`). -/
def params (ts : List Ty) : Nat → Ty := fun n => ts.getD n (.con 0)

/-- Dictionary terms of §2.2; `ap` applies an instance or `struct_C` to a dictionary. -/
inductive Dict where
  | var (k : Nat)
  | slot (d : Dict) (i : Nat)
  | inst (I : Nat)
  | strct (C : Nat)
  | prim (p : Nat)
  | ap (d e : Dict)

/-- `I d̄`, `struct_C d̄`. -/
def apps (f : Dict) (ds : List Dict) : Dict := ds.foldl .ap f

/-- `ρ(d)`: replace the premise variables. -/
def Dict.subst (ρ : Nat → Dict) : Dict → Dict
  | .var k => ρ k
  | .slot d i => .slot (d.subst ρ) i
  | .inst I => .inst I
  | .strct C => .strct C
  | .prim p => .prim p
  | .ap d e => .ap (d.subst ρ) (e.subst ρ)

/-- The dictionaries `d̄` as the values of `δ₀, δ₁, …`. -/
def args (ds : List Dict) : Nat → Dict := fun k => ds.getD k (.var 0)

/-- A scheme `∀ᾱ. Q̄ ⇒ τ`: `k` binders `var 0 … var (k-1)`; the variables from `var k` on are
free. -/
structure Scheme where
  k : Nat
  ctx : List Cn
  ty : Ty

/-- An instance `instance Q̄ ⇒ C τ̄` of §2.5: its head, its context (the premises `δ̄`) and the
dictionaries of its superclass slots, written over the context. -/
structure Inst where
  cls : Nat
  head : List Ty
  ctx : List Cn
  slots : List Dict

/-- A program: its classes (superclasses over the class parameters), its instances `𝓘ₚ`, and
the compiler's classes: structural (`Eq`, `Ord`, `Hash`, `Show` of records) and primitives of
`Record` rows, whose rules (Struct) and (Prim) are black boxes. `methods m` is the class of the
method `m` and its scheme `∀ᾱ β̄. (C ᾱ, Q̄) ⇒ τ` of §1.1: the class constraint comes first. -/
structure Prog where
  supers : Nat → List Cn
  insts : Nat → Option Inst
  structC : Nat → Bool
  fields : Ty → Option (List Ty)
  primC : Nat → Bool
  prims : Cn → Option Nat
  methods : Nat → Option (Nat × Scheme)

variable (P : Prog)

/-- The `i`-th superclass of `Q = C τ̄`: `Sᵢ ῡᵢ[ᾱ := τ̄]`. -/
def Prog.super (Q : Cn) (i : Nat) : Option Cn :=
  (P.supers Q.cls)[i]?.map (·.subst (params Q.args))

/-- `Δ*` of §2.3: (Δ-Given) and (Δ-Super). `Δ` is a list; `δ` is an index into it. -/
inductive Clo (Δ : List Cn) : Dict → Cn → Prop
  | given {k Q} : Δ[k]? = some Q → Clo Δ (.var k) Q
  | super {d Q i S} : Clo Δ d Q → P.super Q i = some S → Clo Δ (.slot d i) S

/-- `Δ ⊩ Q̄ ⇝ d̄`, for a list of constraints at once: (Given), (Prim), (Struct), (Inst).
(Improve) yields no dictionary; the constraints here are the improved ones. -/
inductive Ent (Δ : List Cn) : List Cn → List Dict → Prop
  | nil : Ent Δ [] []
  | cons {Q d qs ds} : Ent Δ [Q] [d] → Ent Δ qs ds → Ent Δ (Q :: qs) (d :: ds)
  | given {Q d} : Clo P Δ d Q → Ent Δ [Q] [d]
  | prim {Q p} : P.primC Q.cls = true → P.prims Q = some p → Ent Δ [Q] [.prim p]
  | strct {C τ ts ds} : P.structC C = true → P.fields τ = some ts →
      Ent Δ (ts.map fun t => ⟨C, [t]⟩) ds → Ent Δ [⟨C, [τ]⟩] [apps (.strct C) ds]
  | inst {Q I ins σ ds} : P.insts I = some ins → ins.cls = Q.cls →
      Q.args = ins.head.map (·.subst σ) → Ent Δ (ins.ctx.map (·.subst σ)) ds →
      Ent Δ [Q] [apps (.inst I) ds]

/-- `≈` of §4.2: the least congruence with the slot rule of an instance, and of a structural
dictionary (Р180). -/
inductive Eqv : Dict → Dict → Prop
  | refl {d} : Eqv d d
  | symm {d e} : Eqv d e → Eqv e d
  | trans {d e f} : Eqv d e → Eqv e f → Eqv d f
  | slot {d e i} : Eqv d e → Eqv (.slot d i) (.slot e i)
  | ap {d d' e e'} : Eqv d d' → Eqv e e' → Eqv (.ap d e) (.ap d' e')
  /-- `(I d̄).$slotᵢ ≈ sᵢ[δ̄ := d̄]` -/
  | instSlot {I ins i s ds} : P.insts I = some ins → ins.slots[i]? = some s →
      Eqv (.slot (apps (.inst I) ds) i) (s.subst (args ds))
  /-- `(struct_C d̄).$slotᵢ ≈ struct_S (d̄.$slotᵢ)` (Р180) -/
  | strctSlot {C i S ds} : P.structC C = true → (P.supers C)[i]? = some S →
      Eqv (.slot (apps (.strct C) ds) i) (apps (.strct S.cls) (ds.map (.slot · i)))

end Flick.Elab
