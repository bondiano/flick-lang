import Formal.Roots
import Formal.Elab.T6a
import Formal.Elab.Sugar

/-!
# λFlick⁺ as a `Target`: the types

The types of `Elab` in the core of `Formal/` with regions and dictionary records:
`var ↦ tvar`, `app ↦ tapp`, `con c ↦ adt`, `Bool ↦ base bool`, `Int ↦ base int`,
`arr φ a b q ↦ ⟦a⟧ →⟦φ⟧^⌈⟦q⟧⌉ ⟦b⟧`,
`cnil ↦ caps ∅`, `cun ↦ caps (∪)`, `rnm c t ↦ rnm c ⟦t⟧`, the element `{⟦t⟧}` with the atoms
`⌈⟦t⟧⌉` (`Row.err`), `run r r' ↦ run ⟦r⟧ ⟦r'⟧`, `D_{C τ̄} ↦ dict (cls C |τ̄|) τ̄`. A stage
`R r e` is `rt ⟦r⟧ (⟦e⟧ + ⌈⟦e⟧⌉ + {Errors ⟦e⟧})` (`errT`): what `⟦e⟧` stands for as a row
(`Ty.asRow`: a variable is a row variable), the atoms that bound what its errors reach
(`errRow`), and a raised `Errors ⟦e⟧`, the error `from_result` raises
(`docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`). The qualifier of a
translated arrow is what a call of it needs live, so it does not count the variables of its
rows.
The translation commutes with substitution (`trT_subst`, `trTT_subst`, `trS_subst`), and
`⌈⟦t⟧⌉` is what `t` shows (`trT_reach`, `Ty.Shown`): a capture variable is an ordinary
variable on both sides.

`Names` chooses the names of the core that realise the program. The class name carries
the number of parameters, so that projecting a superclass slot of `D_{C τ̄}` is typed for any
list `τ̄`, as the law `slot` asks. A structural dictionary is a family over the record type
(`struct_C : D_{C τ₁} →P … →P D_{C τ}` for each `τ`), and so is a primitive one over its
constraint; the model's `Dict.strct C` and `Dict.prim p` do not show them, so their globals
are named by the type too (`docs/formal/elaboration.md#structural-and-primitive-dictionaries-are-families-indexed-by-type`).
-/

namespace Flick.Elab

/-- The names of the core that realise a program: type constructors, dictionary classes
(by class and number of parameters), the labels of methods and slots, the globals of
instances, constants of Γ₀, structural dictionaries (by class and record type) and
primitive ones (by primitive and constraint), the constructor `Some` and the constructors of
the program's data types. -/
structure Names where
  con : Nat → Name
  cls : Nat → Nat → Nat
  meth : Nat → Label
  slot : Nat → Label
  inst : Nat → Nat
  const : Nat → Nat
  strct : Nat → Ty → Nat
  prim : Nat → Cn → Nat
  some : Name
  ctor : Nat → Name
  /-- The type `Errors` of a raised row of errors (`from_result`). -/
  errors : Name

variable (N : Names)

/-- A row of errors of the source in the core: what it stands for as a row and the atoms
it reaches, which bound what its errors reach; so a raised `Errors e` of a row variable `e`,
which reaches `e`, is in the row. -/
def errRow (t : Flick.Ty) : Row := Row.union t.asRow (Row.ofCaps t.reach)

theorem errRow_substT (t : Flick.Ty) (σ : Atom → Flick.Ty) :
    (errRow t).substT σ = errRow (t.substT σ) := by
  simp [errRow, Ty.asRow_substT, Row.ofCaps_substT, Ty.reach_caps]

/-- The error row of a translated runtime stage: the row `t`, the atoms that bound what its
errors reach, and a raised `Errors t`. -/
def errT (t : Flick.Ty) : Flick.Ty :=
  .run (.run t (.caps t.reach)) (.rnm N.errors (.tapp (.adt N.errors) t))

theorem errT_substT (t : Flick.Ty) (σ : Atom → Flick.Ty) :
    (errT N t).substT σ = errT N (t.substT σ) := by
  simp [errT, Flick.Ty.substT, Ty.reach_caps]

/-- What the error row of a translated stage stands for. -/
theorem errT_asRow (t : Flick.Ty) :
    (errT N t).asRow = Row.union (errRow t) (Row.err N.errors (.tapp (.adt N.errors) t)) := by
  simp only [errT, Flick.Ty.asRow, errRow, Row.err, Flick.Ty.reach]

/-- `⟦τ⟧`. -/
def trT : Ty → Flick.Ty
  | .var n => .tvar n
  | .con c => if c = 2 then .base .bool else if c = 4 then .base .int else .adt (N.con c)
  | .app t u => .tapp (trT t) (trT u)
  | .arr .P a b q => .arrow (trT a) .pure (trT b) (trT q).reach
  | .arr (.R r e) a b q => .arrow (trT a) (.rt (trT r) (errT N (trT e))) (trT b) (trT q).reach
  | .cnil => .caps Caps.none
  | .cun q q' => .caps fun a => (trT q).reach a ∨ (trT q').reach a
  | .rnm c t => .rnm (N.con c) (trT t)
  | .run r r' => .run (trT r) (trT r')
  | .all k t => .all k (trT t)

/-- `⟦P⟧ = pure`, `⟦R r e⟧ = rt ⟦r⟧ (⟦e⟧ + ⌈⟦e⟧⌉ + {Errors ⟦e⟧})`, as a stage term. -/
def trStageT : Stage → Flick.Ty
  | .P => .pure
  | .R r e => .rt (trT N r) (errT N (trT N e))

/-- What a translated stage stands for. -/
def trStage (φ : Stage) : Flick.Stage := (trStageT N φ).asStage

theorem trStage_R (r e : Ty) :
    trStage N (.R r e) = .rt (trT N r).asRow (errT N (trT N e)).asRow := rfl

theorem trT_arr (φ : Stage) (a b q : Ty) :
    trT N (.arr φ a b q) = .arrow (trT N a) (trStageT N φ) (trT N b) (trT N q).reach := by
  cases φ <;> rfl

/-- `⟦D_Q⟧`. -/
def trD (q : Cn) : Flick.Ty := .dict (N.cls q.cls q.args.length) (Flick.Ty.spine (q.args.map (trT N)))

/-- `⟦t⟧` of a type of an elaborated term. -/
def trTT : TTy → Flick.Ty
  | .mono t => trT N t
  | .arr φ a b q => .arrow (trTT a) (trStageT N φ) (trTT b) (trT N q).reach
  | .dict q => trD N q

theorem trTT_arr (φ : Stage) (a b : TTy) (q : Ty) :
    trTT N (.arr φ a b q) = .arrow (trTT N a) (trStageT N φ) (trTT N b) (trT N q).reach := rfl

/-- A value bound `(t, D)` as a fact of the core: `Outlives ⌈⟦t⟧⌉ ⌈⟦D⟧⌉`, so `⌈⟦t⟧⌉ ⊆ ⌈⟦D⟧⌉`;
for `t = var i` it is `Facts.bnd i ⌈⟦D⟧⌉` (`trB_var`). -/
def trB (b : Ty × Ty) : Facts := fun C r => C = (trT N b.1).reach ∧ r = .caps (trT N b.2).reach

/-- The value bounds of a scheme as its facts. -/
def trBd : List (Ty × Ty) → Facts
  | [] => Facts.none
  | b :: bd => Facts.union (trB N b) (trBd bd)

/-- The facts of the value bounds of a scheme with `k` variables. A scheme without variables
has no instance to check them at: the source checks them at each use (`Elab.var`), and the
core binds it by (Let-Mono), without facts. -/
def trFacts (k : Nat) (bd : List (Ty × Ty)) : Facts := if k = 0 then Facts.none else trBd N bd

/-- `⟦∀ᾱ. t⟧`, with its value bounds as facts. -/
def trS (s : TSch) : Flick.Scheme := ⟨s.k, trFacts N s.k s.bd, trTT N s.ty⟩

/-- `⟦σ⟧` of a substitution; regions stay. -/
def trσ (σ : Nat → Ty) : Atom → Flick.Ty
  | .v n => trT N (σ n)
  | .r p => .rg p

/-- `t` mentions no variable from `k` on. -/
def TTy.Closed (k : Nat) (t : TTy) : Prop :=
  ∀ σ σ' : Nat → Ty, (∀ j, j < k → σ j = σ' j) → t.subst σ = t.subst σ'

@[simp] theorem trStage_P : trStage N .P = .pure := rfl

theorem trT_bool : trT N boolTy = .base .bool := by simp [trT, boolTy]

theorem trT_int : trT N intTy = .base .int := by simp [trT, intTy]

theorem trT_opt (a : Ty) : trT N (optTy a) = .tapp (.adt (N.con 3)) (trT N a) := by
  simp [trT, optTy]

theorem ex_or {P Q : Nat → Prop} {a : Atom} :
    ((∃ n, a = .v n ∧ P n) ∨ ∃ n, a = .v n ∧ Q n) ↔ ∃ n, a = .v n ∧ (P n ∨ Q n) := by
  constructor
  · rintro (⟨n, rfl, h⟩ | ⟨n, rfl, h⟩)
    · exact ⟨n, rfl, .inl h⟩
    · exact ⟨n, rfl, .inr h⟩
  · rintro ⟨n, rfl, h | h⟩
    · exact .inl ⟨n, rfl, h⟩
    · exact .inr ⟨n, rfl, h⟩

end Flick.Elab
