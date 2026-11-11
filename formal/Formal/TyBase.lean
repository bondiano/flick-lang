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

/-- An element of a row: a named service or error type, or an atom. In a row of
services the atom `ϱ` is `Scope ϱ`; in a row of errors the atoms bound what the
errors reach, `⌈Errors ε⌉` (regions.md §2.1). A row of services also carries the
obligations `{Provided {S} ϱ}` not yet discharged (`ob S ϱ`, regions.md §2.7) and those
`{Provided r ϱ}` over a row variable `r` (`obv r G ϱ`). `rv a G` is the row variable `a`
without the names `G`: `G` is empty for `a` itself, and the tail of `{-S | a}` is `a`
without `S`. A substitution replaces it with the row that `σ a` stands for (`Ty.asRow`)
less the names of `G` (`Row.less`); an obligation is over a row variable in the same way. -/
inductive Elem
  | nm (N : Name)
  | atm (a : Atom)
  | ob (N : Name) (a : Atom)
  | rv (a : Atom) (G : Name → Prop)
  | obv (v : Atom) (G : Name → Prop) (a : Atom)

instance : Coe Name Elem := ⟨Elem.nm⟩

/-- A row of services or errors, possibly with row variables. Equality of rows is
extensional, which is `≡` of core.md §1.2. -/
abbrev Row := Elem → Prop

def Row.none : Row := fun _ => False
def Row.single (S : Name) : Row := fun n => n = .nm S
def Row.union (r r' : Row) : Row := fun n => r n ∨ r' n
/-- A row without row variables. -/
def Row.Named (r : Row) : Prop := ∀ a G, ¬ r (.rv a G)

/-- `ρ ⊆ ρ'`. -/
def Row.Sub (r r' : Row) : Prop := ∀ n, r n → r' n

/-- `ρ` less the names of `G`: its names out of `G`, its atoms, and each of its variables
without `G` too. No obligation comes in through a row variable. -/
def Row.less (G : Name → Prop) (r : Row) : Row
  | .nm n => ¬ G n ∧ r (.nm n)
  | .atm a => r (.atm a)
  | .rv c H => ∃ H', r (.rv c H') ∧ H = fun n => G n ∨ H' n
  | _ => False

/-- `ρ` beside the names of `H`, which give back what its variables are taken without:
beside `H`, a variable `a` without `G` is `a` without the names of `G` that `H` lacks,
`H + (a − G) ≡ H + (a − (G − H))`. A handler of `H` leaves `ρ` to its surroundings when
its body is within `H + ρ.back H`, so a body of the row `a` leaves `a − H`. -/
def Row.back (r H : Row) : Row
  | .rv a G' => ∃ G, r (.rv a G) ∧ (∀ n, G' n → G n) ∧ ∀ n, G n → G' n ∨ H (.nm n)
  | x => r x
/-- `ρ` up to the names its variables are taken without: what of a row passes a `handle`,
which removes a name by the error value it holds, not by the row. -/
def Row.modNames (r : Row) : Row
  | .rv a _ => ∃ G, r (.rv a G)
  | x => r x

/-- The atoms of a set as row elements: `Scope ϱ` for a region `ϱ`. -/
def Row.ofCaps (C : Caps) : Row
  | .atm a => C a
  | _ => False

/-- Stages `φ` of core.md §1.2: pure `P` or runtime `R⟨r; ε⟩`. -/
inductive Stage
  | pure
  | rt (r e : Row)

def Stage.svc : Stage → Row
  | .pure => Row.none
  | .rt r _ => r

def Stage.err : Stage → Row
  | .pure => Row.none
  | .rt _ e => e

/-- `φ ⊑ φ'` of core.md §2.1. -/
def Stage.le : Stage → Stage → Prop
  | .pure, _ => True
  | .rt _ _, .pure => False
  | .rt r e, .rt r' e' => Row.Sub r r' ∧ Row.Sub e e'

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
storage is a parameter of kind `CRow` of its type. A row of services or errors is `tvar n`
or `row r`. `hnd ϱ` is `H ϱ`. `dict c ā` is the
dictionary record `D_C ā` of λFlick⁺ (elaboration.md §0.1), its arguments a spine
`Ty.spine`; it reaches nothing. `all k t` is `∀ α₀ … αₖ₋₁. t`: the indices below `k` of `t` are
its variables, the others those around it, shifted by `k`. -/
inductive Ty
  | tvar (n : Nat)
  | rg (p : Nat)
  | caps (C : Caps)
  | row (r : Row)
  | base (b : Base)
  | arrow (a : Ty) (φ : Stage) (b : Ty) (C : Caps)
  | ref (t : Ty)
  | adt (T : Name)
  | tapp (f a : Ty)
  | rnil
  | rcons (l : Label) (t rest : Ty)
  | eff (r e : Row) (t : Ty) (C : Caps)
  | errors (e : Row)
  | hnd (s : Ty)
  | owner (s : Ty)
  | dict (c : Nat) (args : Ty)
  | all (k : Nat) (t : Ty)

/-- An atom under `k` binders. -/
def Atom.up (k : Nat) : Atom → Atom
  | .v n => .v (n + k)
  | .r p => .r p

/-- `⌈τ⌉` of regions.md §2.1: the atoms a value of type `τ` may reach. Rows of
services are not in it; a row of errors contributes its atoms and its variables, which
bound what the errors of an unknown row reach. -/
def Ty.reach : Ty → Caps
  | .tvar n => fun a => a = .v n
  | .rg p => fun a => a = .r p
  | .caps C => C
  | .row r => fun a => r (.atm a) ∨ ∃ G, r (.rv a G)
  | .arrow a _ b C => fun x => a.reach x ∨ b.reach x ∨ C x
  | .ref t => t.reach
  | .tapp f a => fun x => f.reach x ∨ a.reach x
  | .rcons _ t r => fun x => t.reach x ∨ r.reach x
  | .eff _ _ t C => fun x => t.reach x ∨ C x
  | .errors e => fun x => e (.atm x) ∨ ∃ G, e (.rv x G)
  | .hnd s => s.reach
  | .owner s => s.reach
  | .all k t => fun a => t.reach (a.up k)
  | _ => Caps.none

/-- A set of atoms after a substitution: each atom becomes `⌈σ a⌉`. -/
def Caps.substT (σ : Atom → Ty) (C : Caps) : Caps := fun a => ∃ b, C b ∧ (σ b).reach a

/-- The names, atoms and variables of a row: obligations never come in through a row
variable. -/
def Row.plain (r : Row) : Row
  | .ob .. | .obv .. => False
  | x => r x

/-- What a type stands for as the instance of a row variable: a variable is itself, a
`row r` the plain part of `r`, any other type the atoms it reaches. -/
def Ty.asRow : Ty → Row
  | .tvar n => fun x => x = .rv (.v n) fun _ => False
  | .rg p => fun x => x = .rv (.r p) fun _ => False
  | .row r => r.plain
  | t => Row.ofCaps t.reach

/-- A row after a substitution: a name stays, an atom `a` (also of an obligation)
becomes the atoms of `⌈σ a⌉`, a row variable `b` without `G` the row `σ b` stands for
less the names of `G`, and so does the row variable of an obligation: `Provided v ϱ`
becomes one for each name and variable of that row. -/
def Row.substT (σ : Atom → Ty) (r : Row) : Row
  | .nm n => r (.nm n) ∨ ∃ b G, r (.rv b G) ∧ ¬ G n ∧ (σ b).asRow (.nm n)
  | .atm a => (∃ b, r (.atm b) ∧ (σ b).reach a) ∨ ∃ b G, r (.rv b G) ∧ (σ b).asRow (.atm a)
  | .ob n a => (∃ b, r (.ob n b) ∧ (σ b).reach a) ∨
      ∃ v G b, r (.obv v G b) ∧ ¬ G n ∧ (σ v).asRow (.nm n) ∧ (σ b).reach a
  | .obv w H a => ∃ v G b, r (.obv v G b) ∧ (σ v).asRow.less G (.rv w H) ∧ (σ b).reach a
  | .rv c H => ∃ b G, r (.rv b G) ∧ (σ b).asRow.less G (.rv c H)

def Stage.substT (σ : Atom → Ty) : Stage → Stage
  | .pure => .pure
  | .rt r e => .rt (r.substT σ) (e.substT σ)

/-- An atom as a type. -/
def Atom.ty : Atom → Ty
  | .v n => .tvar n
  | .r p => .rg p

/-- A renaming of atoms under `k` binders. -/
def liftA (k : Nat) (ξ : Atom → Atom) : Atom → Atom
  | .v i => if i < k then .v i else (ξ (.v (i - k))).up k
  | .r p => (ξ (.r p)).up k

/-- A renaming of atoms: what a substitution under a binder shifts by. -/
def Ty.ren (ξ : Atom → Atom) : Ty → Ty
  | .tvar n => (ξ (.v n)).ty
  | .rg p => (ξ (.r p)).ty
  | .caps C => .caps (C.substT fun a => (ξ a).ty)
  | .row r => .row (r.substT fun a => (ξ a).ty)
  | .base b => .base b
  | .arrow a φ b C => .arrow (a.ren ξ) (φ.substT fun a => (ξ a).ty) (b.ren ξ) (C.substT fun a => (ξ a).ty)
  | .ref t => .ref (t.ren ξ)
  | .adt T => .adt T
  | .tapp f a => .tapp (f.ren ξ) (a.ren ξ)
  | .rnil => .rnil
  | .rcons l t r => .rcons l (t.ren ξ) (r.ren ξ)
  | .eff r e t C => .eff (r.substT fun a => (ξ a).ty) (e.substT fun a => (ξ a).ty) (t.ren ξ) (C.substT fun a => (ξ a).ty)
  | .errors e => .errors (e.substT fun a => (ξ a).ty)
  | .hnd s => .hnd (s.ren ξ)
  | .owner s => .owner (s.ren ξ)
  | .dict c a => .dict c (a.ren ξ)
  | .all k t => .all k (t.ren (liftA k ξ))

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

/-- Parallel substitution of variables and run-time regions. -/
def Ty.substT (σ : Atom → Ty) : Ty → Ty
  | .tvar n => σ (.v n)
  | .rg p => σ (.r p)
  | .caps C => .caps (C.substT σ)
  | .row r => .row (r.substT σ)
  | .base b => .base b
  | .arrow a φ b C => .arrow (a.substT σ) (φ.substT σ) (b.substT σ) (C.substT σ)
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

/-- The identity substitution. -/
def Ty.ident : Atom → Ty
  | .v n => .tvar n
  | .r p => .rg p

end Flick
