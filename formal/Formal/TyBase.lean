/-!
# Types of λFlick with regions (core.md §1.2, regions.md §1.2)

Variables of every kind — types, regions, capture rows — are de Bruijn indices
`tvar n`; a run-time region `π` is `rg π`. `∀` is the schemes of a typing context
(`Scheme`) and the binder `all k t` of a type.
-/

namespace Flick

abbrev Label := String
abbrev Name := String

/-- An atom of a capture set (regions.md §1.2): a variable of any kind, or a
run-time region `π`. -/
inductive Atom
  | v (n : Nat)
  | r (p : Nat)
  deriving DecidableEq

/-- A set of atoms: a capture qualifier `C` or `⌈τ⌉`. Equality is extensional. -/
abbrev Caps := Atom → Prop

def Caps.none : Caps := fun _ => False

/-- Built-in base types: `Unit`, `Bool`, `Int`. -/
inductive Base
  | unit
  | bool
  | int
  | string
  deriving DecidableEq, Repr

/-- Literals `n` of core.md §1.3; `()`, `True`, `False` are literals too. -/
inductive Lit
  | unit
  | bool (b : Bool)
  | int (n : Int)
  | str (s : String)
  deriving DecidableEq, Repr

def Lit.base : Lit → Base
  | .unit => .unit
  | .bool _ => .bool
  | .int _ => .int
  | .str _ => .string

/-- Types `τ` of core.md §1.2 and regions.md §1.2.
A record type is a spine `rcons ℓ τ (… rnil)`; field order is fixed. A nominal
type `T ā` is a spine `tapp (… (tapp (adt T) a₁) …) aₙ`. A region is `tvar n` or
`rg π`; a capture row is `tvar n` or `caps C`. The hidden qualifier `C` of a
function or a computation is part of `arrow` and `eff`; that of data with hidden
storage is a parameter of kind `CRow` of its type. `hnd ϱ` is `H ϱ`. `dict c ā` is the
dictionary record `D_C ā` of λFlick⁺ (elaboration.md §0.1), its arguments a spine
`Ty.spine`; it reaches nothing. `all k t` is `∀ α₀ … αₖ₋₁. t`: the indices below `k` of `t` are
its variables, the others those around it, shifted by `k`.

Rows of services and errors are types too: a row variable `tvar n` (or a region `rg π`),
the atoms `caps C` (`Scope ϱ` is `caps ⌈ϱ⌉`), the empty row `cnil`, a named element
`rnm N a` whose argument `a` is the nominal type `N ā` it stands for, a union `run`, and a
row less every instance of some names, `rless G r`. A stage is `pure`, `rt r e`, or
`rtob r s e`: `rt` whose services also hold the obligations `Provided r s` of its own row,
one for each of its names and variables about each atom `s` reaches. Types hold no predicate over row elements, so an element can keep its
arguments (`Elem.nm` in `Rows.lean`); what a row or a stage stands for is `Ty.asRow` and
`Ty.asStage`. -/
inductive Ty
  | tvar (n : Nat)
  | rg (p : Nat)
  | caps (C : Caps)
  | cnil
  | rnm (N : Name) (args : Ty)
  | run (left right : Ty)
  | rless (gone : Name → Prop) (row : Ty)
  | pure
  | rt (services errors : Ty)
  | rtob (services owner errors : Ty)
  | base (b : Base)
  | arrow (a : Ty) (φ : Ty) (b : Ty) (C : Caps)
  | ref (t : Ty)
  | adt (T : Name)
  | tapp (f a : Ty)
  | rnil
  | rcons (l : Label) (t rest : Ty)
  | eff (r e : Ty) (t : Ty) (C : Caps)
  | errors (e : Ty)
  | hnd (s : Ty)
  | owner (s : Ty)
  | dict (c : Nat) (args : Ty)
  | all (k : Nat) (t : Ty)
  /-- The hidden row of data with hidden storage written bare in a signature, of kind `CRow`:
  where it is written decides what it stands for (`Scheme.elide`), and no typing rule meets
  it. It reaches nothing, but it is not the written `caps ∅`. -/
  | bare
  /-- `Lazy ρ ε τ` (lazy.md §2), with the hidden qualifier `C` of its initializer, as `eff`. -/
  | lazy (r e : Ty) (t : Ty) (C : Caps)
  /-- The type the store gives a lazy cell of `Lazy ρ ε τ` (lazy.md §3): no value has it, so
  a lazy cell is never a `Ref` cell. -/
  | lazyCell (r e : Ty) (t : Ty) (C : Caps)
  /-- `FuncRef τ` (core.md §2.3 (FuncRef-In)): a function without captures, a value of `τ`
  typed by a term that names no local. It reaches what `τ` reaches. -/
  | funcRef (t : Ty)

/-- A stage term. In the stage position of an arrow anything else is no stage: it reads as
`pure` (`Ty.asStage`) and a substitution leaves it as it is, so that what an arrow's stage
stands for commutes with every substitution although kinds are not checked. -/
def Ty.isStage : Ty → Bool
  | .pure | .rt .. | .rtob .. => true
  | _ => false

def Atom.up (k : Nat) : Atom → Atom
  | .v n => .v (n + k)
  | .r p => .r p

/-- `⌈τ⌉` of regions.md §2.1: the atoms a value of type `τ` may reach. Rows of
services are not in it; a row of errors contributes its atoms and its variables, which
bound what the errors of an unknown row reach: as a type, a row reaches just these. -/
def Ty.reach : Ty → Caps
  | .tvar n => fun a => a = .v n
  | .rg p => fun a => a = .r p
  | .caps C => C
  | .rnm _ args => args.reach
  | .run left right => fun a => left.reach a ∨ right.reach a
  | .rless _ r => r.reach
  | .arrow a _ b C => fun x => a.reach x ∨ b.reach x ∨ C x
  | .ref t | .funcRef t => t.reach
  | .tapp f a => fun x => f.reach x ∨ a.reach x
  | .rcons _ t r => fun x => t.reach x ∨ r.reach x
  | .eff _ _ t C => fun x => t.reach x ∨ C x
  | .errors e => e.reach
  | .hnd s => s.reach
  | .owner s => s.reach
  | .all k t => fun a => t.reach (a.up k)
  -- a lazy keeps the outcome it caches: an error too
  | .lazy _ e t C | .lazyCell _ e t C => fun x => t.reach x ∨ e.reach x ∨ C x
  | _ => Caps.none

/-- A set of atoms after a substitution: each atom becomes `⌈σ a⌉`. -/
def Caps.substT (σ : Atom → Ty) (C : Caps) : Caps := fun a => ∃ b, C b ∧ (σ b).reach a

/-- An atom as a type. -/
def Atom.ty : Atom → Ty
  | .v n => .tvar n
  | .r p => .rg p

/-- A renaming of atoms under `k` binders. -/
def liftA (k : Nat) (ξ : Atom → Atom) : Atom → Atom
  | .v i => if i < k then .v i else (ξ (.v (i - k))).up k
  | .r p => (ξ (.r p)).up k

/-- A renaming of atoms: what a substitution under a binder shifts by. An arrow's stage is
renamed only if it is a stage term (`Ty.isStage`). -/
def Ty.ren (ξ : Atom → Atom) : Ty → Ty
  | .tvar n => (ξ (.v n)).ty
  | .rg p => (ξ (.r p)).ty
  | .caps C => .caps (C.substT fun a => (ξ a).ty)
  | .cnil => .cnil
  | .rnm N args => .rnm N (args.ren ξ)
  | .run left right => .run (left.ren ξ) (right.ren ξ)
  | .rless gone r => .rless gone (r.ren ξ)
  | .pure => .pure
  | .rt r e => .rt (r.ren ξ) (e.ren ξ)
  | .rtob r s e => .rtob (r.ren ξ) (s.ren ξ) (e.ren ξ)
  | .base b => .base b
  | .arrow a φ b C =>
    .arrow (a.ren ξ) (if φ.isStage then φ.ren ξ else φ) (b.ren ξ) (C.substT fun a => (ξ a).ty)
  | .ref t => .ref (t.ren ξ)
  | .adt T => .adt T
  | .tapp f a => .tapp (f.ren ξ) (a.ren ξ)
  | .rnil => .rnil
  | .rcons l t r => .rcons l (t.ren ξ) (r.ren ξ)
  | .eff r e t C => .eff (r.ren ξ) (e.ren ξ) (t.ren ξ) (C.substT fun a => (ξ a).ty)
  | .errors e => .errors (e.ren ξ)
  | .hnd s => .hnd (s.ren ξ)
  | .owner s => .owner (s.ren ξ)
  | .dict c a => .dict c (a.ren ξ)
  | .all k t => .all k (t.ren (liftA k ξ))
  | .bare => .bare
  | .lazy r e t C => .lazy (r.ren ξ) (e.ren ξ) (t.ren ξ) (C.substT fun a => (ξ a).ty)
  | .lazyCell r e t C => .lazyCell (r.ren ξ) (e.ren ξ) (t.ren ξ) (C.substT fun a => (ξ a).ty)
  | .funcRef t => .funcRef (t.ren ξ)

/-- The renaming `i ↦ i + k` as a substitution; run-time regions stay. -/
def shiftV (k : Nat) : Atom → Ty
  | .v i => .tvar (i + k)
  | .r p => .rg p

/-- Shift the free variables by `k`. -/
def Ty.up (k : Nat) (t : Ty) : Ty := t.ren (Atom.up k)

/-- A substitution under `k` binders. -/
def liftT (k : Nat) (σ : Atom → Ty) : Atom → Ty
  | .v i => if i < k then .tvar i else (σ (.v (i - k))).up k
  | .r p => (σ (.r p)).up k

/-- Parallel substitution of variables and run-time regions. It is homomorphic, except that
an arrow's stage is substituted into only if it is a stage term (`Ty.isStage`). -/
def Ty.substT (σ : Atom → Ty) : Ty → Ty
  | .tvar n => σ (.v n)
  | .rg p => σ (.r p)
  | .caps C => .caps (C.substT σ)
  | .cnil => .cnil
  | .rnm N args => .rnm N (args.substT σ)
  | .run left right => .run (left.substT σ) (right.substT σ)
  | .rless gone r => .rless gone (r.substT σ)
  | .pure => .pure
  | .rt r e => .rt (r.substT σ) (e.substT σ)
  | .rtob r s e => .rtob (r.substT σ) (s.substT σ) (e.substT σ)
  | .base b => .base b
  | .arrow a φ b C =>
    .arrow (a.substT σ) (if φ.isStage then φ.substT σ else φ) (b.substT σ) (C.substT σ)
  | .ref t => .ref (t.substT σ)
  | .adt T => .adt T
  | .tapp f a => .tapp (f.substT σ) (a.substT σ)
  | .rnil => .rnil
  | .rcons l t r => .rcons l (t.substT σ) (r.substT σ)
  | .eff r e t C => .eff (r.substT σ) (e.substT σ) (t.substT σ) (C.substT σ)
  | .errors e => .errors (e.substT σ)
  | .hnd s => .hnd (s.substT σ)
  | .owner s => .owner (s.substT σ)
  | .dict c a => .dict c (a.substT σ)
  | .all k t => .all k (t.substT (liftT k σ))
  | .bare => .bare
  | .lazy r e t C => .lazy (r.substT σ) (e.substT σ) (t.substT σ) (C.substT σ)
  | .lazyCell r e t C => .lazyCell (r.substT σ) (e.substT σ) (t.substT σ) (C.substT σ)
  | .funcRef t => .funcRef (t.substT σ)

/-- An arrow's stage after a substitution. -/
abbrev Ty.substS (σ : Atom → Ty) (φ : Ty) : Ty := if φ.isStage then φ.substT σ else φ

@[simp] theorem Ty.substT_arrow (σ : Atom → Ty) (a φ b : Ty) (C : Caps) :
    (Ty.arrow a φ b C).substT σ = .arrow (a.substT σ) (φ.substS σ) (b.substT σ) (C.substT σ) := rfl

@[simp] theorem Ty.substS_pure (σ : Atom → Ty) : Ty.pure.substS σ = .pure := rfl

@[simp] theorem Ty.substS_rt (σ : Atom → Ty) (r e : Ty) :
    (Ty.rt r e).substS σ = .rt (r.substT σ) (e.substT σ) := rfl

@[simp] theorem Ty.substS_rtob (σ : Atom → Ty) (r s e : Ty) :
    (Ty.rtob r s e).substS σ = .rtob (r.substT σ) (s.substT σ) (e.substT σ) := rfl

theorem Ty.isStage_substS (σ : Atom → Ty) (φ : Ty) : (φ.substS σ).isStage = φ.isStage := by
  cases φ <;> rfl

/-- The identity substitution. -/
def Ty.ident : Atom → Ty
  | .v n => .tvar n
  | .r p => .rg p

end Flick
