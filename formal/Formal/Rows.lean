import Formal.TyReach

/-!
# What a row term stands for

A row is a set of elements, `Elem → Prop`, with variables among them; equality of rows is
extensional, which is the row equality of `core.md`. A named element keeps its argument, the nominal type it
stands for, up to congruence (`TyClass`): `Store Int` and `Store Bool` are two elements,
and so are `NotFound Int` and `NotFound String`. Subtraction is by name: `Row.less G`
removes every instance of each name of `G`.

Types hold row and stage terms only (`TyBase.lean`). An element with a type argument could
not live in a predicate row that types themselves contain: `Ty` would include the subsets
of a set that includes `Ty`. The judgments use the predicate rows below, what the terms
stand for (`Ty.asRow`, `Ty.asStage`).
-/

namespace Flick

def Ty.equivSetoid : Setoid Ty where
  r := Ty.Equiv
  iseqv := ⟨fun _ => .refl, fun h => .symm h, fun h h' => .trans h h'⟩

/-- The argument of a named row element, up to congruence. -/
abbrev TyClass := Quotient Ty.equivSetoid

def Ty.classOf (t : Ty) : TyClass := Quotient.mk _ t

theorem Ty.classOf_eq {a b : Ty} : a.classOf = b.classOf ↔ Ty.Equiv a b :=
  ⟨fun h => Quotient.exact h, fun h => Quotient.sound (s := Ty.equivSetoid) h⟩

theorem Ty.Equiv.classOf {a b : Ty} (h : Ty.Equiv a b) : a.classOf = b.classOf :=
  Ty.classOf_eq.mpr h

/-- An argument after a substitution. -/
def TyClass.substT (σ : Atom → Ty) : TyClass → TyClass :=
  Quotient.lift (fun t => (t.substT σ).classOf) fun _ _ h => (Ty.Equiv.substT h σ).classOf

@[simp] theorem TyClass.substT_classOf (σ : Atom → Ty) (t : Ty) :
    t.classOf.substT σ = (t.substT σ).classOf := rfl

theorem TyClass.substT_comp (c : TyClass) (σ σ' : Atom → Ty) :
    (c.substT σ).substT σ' = c.substT fun a => (σ a).substT σ' := by
  induction c using Quotient.ind with
  | _ t => exact congrArg Ty.classOf (Ty.substT_comp t σ σ')

theorem TyClass.substT_ident (c : TyClass) : c.substT Ty.ident = c := by
  induction c using Quotient.ind with
  | _ t => exact congrArg Ty.classOf (Ty.substT_ident t)

theorem TyClass.substT_congr {σ σ' : Atom → Ty} (h : ∀ a, σ a = σ' a) (c : TyClass) :
    c.substT σ = c.substT σ' := by
  rw [funext h]

/-- An element of a row: a named service or error type with its argument, or an atom. In a
row of services the atom `ϱ` is `Scope ϱ`; in a row of errors the atoms bound what the
errors reach, `⌈Errors ε⌉` (regions.md §2.1). A row of services also carries the
obligations `{Provided {S} ϱ}` not yet discharged (`ob S a ϱ`, regions.md §2.7) and those
`{Provided r ϱ}` over a row variable `r` (`obv r G ϱ`). `rv a G` is the row variable `a`
without the names `G`: `G` is empty for `a` itself, and the tail of `{-S | a}` is `a`
without `S`. A substitution replaces it with the row that `σ a` stands for (`Ty.asRow`)
less the names of `G` (`Row.less`); an obligation is over a row variable in the same way. -/
inductive Elem
  | nm (N : Name) (args : TyClass)
  | atm (a : Atom)
  | ob (N : Name) (args : TyClass) (a : Atom)
  | rv (a : Atom) (G : Name → Prop)
  | obv (v : Atom) (G : Name → Prop) (a : Atom)

/-- The element of a service or an error type without parameters: the name at itself. -/
abbrev Elem.nm₀ (N : Name) : Elem := .nm N (Ty.adt N).classOf

/-- A row of services or errors, possibly with row variables. Equality of rows is
extensional, which is `≡` of core.md §1.2. -/
abbrev Row := Elem → Prop

def Row.none : Row := fun _ => False
/-- One named element: `N` at the argument `c`. -/
def Row.inst (N : Name) (c : TyClass) : Row := fun n => n = .nm N c
/-- The element of a service or an error type without parameters: the name at itself. -/
def Row.single (S : Name) : Row := Row.inst S (Ty.adt S).classOf
def Row.union (r r' : Row) : Row := fun n => r n ∨ r' n
/-- A row without row variables. -/
def Row.Named (r : Row) : Prop := ∀ a G, ¬ r (.rv a G)

/-- `ρ ⊆ ρ'`. -/
def Row.Sub (r r' : Row) : Prop := ∀ n, r n → r' n

/-- `ρ` less the names of `G`: its named elements whose name is out of `G`, whatever their
arguments, its atoms, and each of its variables without `G` too. No obligation comes in
through a row variable. -/
def Row.less (G : Name → Prop) (r : Row) : Row
  | .nm n c => ¬ G n ∧ r (.nm n c)
  | .atm a => r (.atm a)
  | .rv c H => ∃ H', r (.rv c H') ∧ H = fun n => G n ∨ H' n
  | _ => False

/-- `ρ` beside the names of `H`, which give back what its variables are taken without:
beside `H`, a variable `a` without `G` is `a` without the names of `G` that `H` lacks,
`H + (a − G) ≡ H + (a − (G − H))`. A handler of `H` leaves `ρ` to its surroundings when
its body is within `H + ρ.back H`, so a body of the row `a` leaves `a − H`. A name is given
back whatever argument `H` has it at. -/
def Row.back (r H : Row) : Row
  | .rv a G' => ∃ G, r (.rv a G) ∧ (∀ n, G' n → G n) ∧ ∀ n, G n → G' n ∨ ∃ c, H (.nm n c)
  | x => r x
/-- `ρ` with every instance of each of its names. An arm that catches a constructor and a
handler of an effect act by name at run time, whatever the arguments, so this is what they
remove; unlike a set of instances, it stays the same under a substitution
(`Row.nameClose_substT`). -/
def Row.nameClose (r : Row) : Row
  | .nm n _ => ∃ c, r (.nm n c)
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

/-- What the errors of a row reach: its atoms and its variables (`⌈Errors ε⌉`). -/
def Row.reach (r : Row) : Caps := fun a => r (.atm a) ∨ ∃ G, r (.rv a G)

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

/-- The names, atoms and variables of a row: obligations never come in through a row
variable. -/
def Row.plain (r : Row) : Row
  | .ob .. | .obv .. => False
  | x => r x

/-- `Fiber.Provided r s` of regions.md §2.7 as row elements: one obligation for each
service and each row variable of `r`. -/
def Row.obs (r : Row) (s : Ty) : Row
  | .ob N c a => r (.nm N c) ∧ s.reach a
  | .obv v G a => r (.rv v G) ∧ s.reach a
  | _ => False

/-- What a type stands for as a row, and as the instance of a row variable: a variable is
itself, a named element its name at its argument with the atoms the argument reaches, a
union and a subtraction what their parts stand for, any other type the atoms it reaches.
It holds no obligation. -/
def Ty.asRow : Ty → Row
  | .tvar n => fun x => x = .rv (.v n) fun _ => False
  | .rg p => fun x => x = .rv (.r p) fun _ => False
  | .cnil => Row.none
  | .rnm N args => Row.union (Row.inst N args.classOf) (Row.ofCaps args.reach)
  | .run left right => Row.union left.asRow right.asRow
  | .rless gone r => r.asRow.less gone
  | t => Row.ofCaps t.reach

/-- What a stage term stands for; a type that is no stage term reads as `pure`. The
obligations of a stage come only from `rtob`, never through a row variable. -/
def Ty.asStage : Ty → Stage
  | .pure => .pure
  | .rt r e => .rt r.asRow e.asRow
  | .rtob r s e => .rt (Row.union r.asRow (Row.obs r.asRow s)) e.asRow
  | _ => .pure

/-- A row after a substitution: a named element keeps its name at the substituted
argument, an atom `a` (also of an obligation) becomes the atoms of `⌈σ a⌉`, a row variable
`b` without `G` the row `σ b` stands for less the names of `G`, and so does the row variable
of an obligation: `Provided v ϱ` becomes one for each name and variable of that row. -/
def Row.substT (σ : Atom → Ty) (r : Row) : Row
  | .nm n c => (∃ c₀, r (.nm n c₀) ∧ c₀.substT σ = c) ∨
      ∃ b G, r (.rv b G) ∧ ¬ G n ∧ (σ b).asRow (.nm n c)
  | .atm a => (∃ b, r (.atm b) ∧ (σ b).reach a) ∨ ∃ b G, r (.rv b G) ∧ (σ b).asRow (.atm a)
  | .ob n c a => (∃ c₀ b, r (.ob n c₀ b) ∧ c₀.substT σ = c ∧ (σ b).reach a) ∨
      ∃ v G b, r (.obv v G b) ∧ ¬ G n ∧ (σ v).asRow (.nm n c) ∧ (σ b).reach a
  | .obv w H a => ∃ v G b, r (.obv v G b) ∧ (σ v).asRow.less G (.rv w H) ∧ (σ b).reach a
  | .rv c H => ∃ b G, r (.rv b G) ∧ (σ b).asRow.less G (.rv c H)

def Stage.substT (σ : Atom → Ty) : Stage → Stage
  | .pure => .pure
  | .rt r e => .rt (r.substT σ) (e.substT σ)

end Flick
