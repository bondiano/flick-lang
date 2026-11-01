/-!
# Syntax of λFlick (core.md §1), stage P1a

Types are monomorphic: no type variables, no stages, no rows. Terms use
de Bruijn indices; see `formal/README.md` for the encoding of lists
(record fields, match arms, constructor arguments) as spines.
-/

namespace Flick

abbrev Label := String
abbrev Name := String

/-- Built-in base types: `Unit`, `Bool`, `Int`. -/
inductive Base
  | unit
  | bool
  | int
  deriving DecidableEq, Repr

/-- Literals `n` of core.md §1.3; `()`, `True`, `False` are literals too. -/
inductive Lit
  | unit
  | bool (b : Bool)
  | int (n : Int)
  deriving DecidableEq, Repr

def Lit.base : Lit → Base
  | .unit => .unit
  | .bool _ => .bool
  | .int _ => .int

/-- Monotypes `τ` of core.md §1.2 without variables, stages and rows.
A record type is a spine `rcons ℓ τ (… rnil)`; field order is fixed. -/
inductive Ty
  | base (b : Base)
  | arrow (a r : Ty)
  | ref (t : Ty)
  | adt (T : Name)
  | rnil
  | rcons (l : Label) (t rest : Ty)
  deriving DecidableEq, Repr

/-- `τ₁ → … → τₙ → r`. -/
def Ty.arrows : List Ty → Ty → Ty
  | [], r => r
  | a :: rest, r => .arrow a (Ty.arrows rest r)

/-- Labels of a record type. -/
def Ty.labels : Ty → List Label
  | .rcons l _ r => l :: r.labels
  | _ => []

/-- The type of field `l` of a record type. -/
def Ty.field : Ty → Label → Option Ty
  | .rcons l' t r, l => if l' = l then some t else r.field l
  | _, _ => none

/-- Constants of Γ₀ (core.md §1.5), instantiated at a monotype. -/
inductive Const
  | refMake (t : Ty)
  | refGet (t : Ty)
  | refSet (t : Ty)
  deriving DecidableEq, Repr

def Const.ty : Const → Ty
  | .refMake t => .arrow t (.ref t)
  | .refGet t => .arrow (.ref t) t
  | .refSet t => .arrow (.ref t) (.arrow t (.base .unit))

def Const.arity : Const → Nat
  | .refMake _ => 1
  | .refGet _ => 1
  | .refSet _ => 2

/-- Patterns of core.md §1.3. A constructor pattern `K p₁ … pₙ` is the spine
`app (… (app (ctor K) p₁) …) pₙ`; a record pattern `{ℓ₁ = p₁, …, ..}` is
`rfield ℓ₁ p₁ (… rany)`. -/
inductive Pat
  | wild
  | var
  | lit (l : Lit)
  | ctor (K : Name)
  | app (p q : Pat)
  | rany
  | rfield (l : Label) (p rest : Pat)
  | or (p q : Pat)
  deriving DecidableEq, Repr

/-- Number of variables a pattern binds, left to right. -/
def Pat.binds : Pat → Nat
  | .var => 1
  | .app p q => p.binds + q.binds
  | .rfield _ p r => p.binds + r.binds
  | .or p _ => p.binds
  | _ => 0

/-- Terms of core.md §1.3 and the run-time forms of §3.1 (`loc`, `fix`).
Binders:
* `lam b` — `b` under one variable;
* `let_ e b` — `b` under one;
* `letrec f b` — `let rec f = λx. f in b`: `f` under two (`0 = x`, `1 = f`),
  `b` under one (`f`);
* `fix f` — `fix f. λx. f`, two binders as in `letrec`;
* `acons p b rest`, `aguard p g b rest` — match arms; `g` and `b` under
  `p.binds` variables. Arms end in `anil`.

Records are spines `rcons ℓ e (… rnil)`. -/
inductive Tm
  | var (i : Nat)
  | lam (b : Tm)
  | app (f a : Tm)
  | let_ (e b : Tm)
  | letrec (f b : Tm)
  | fix (f : Tm)
  | lit (l : Lit)
  | loc (l : Nat)
  | const (c : Const)
  | ctor (K : Name)
  | rnil
  | rcons (l : Label) (e rest : Tm)
  | proj (e : Tm) (l : Label)
  | update (e : Tm) (l : Label) (e' : Tm)
  | match_ (e alts : Tm)
  | anil
  | acons (p : Pat) (b rest : Tm)
  | aguard (p : Pat) (g b rest : Tm)
  | todo
  deriving DecidableEq, Repr

/-- Head of an application spine. -/
def Tm.head : Tm → Tm
  | .app f _ => f.head
  | t => t

/-- Field `l` of a record value. -/
def Tm.field : Tm → Label → Option Tm
  | .rcons l' e r, l => if l' = l then some e else r.field l
  | _, _ => none

/-- `{v with l = w}` on a record value. -/
def Tm.setField : Tm → Label → Tm → Option Tm
  | .rcons l' e r, l, w =>
    if l' = l then some (.rcons l' w r) else (r.setField l w).map (.rcons l' e ·)
  | _, _, _ => none

/-- Patterns of the arms without `when`: the set `Π` of `Exh` in (Match). -/
def Tm.unguarded : Tm → List Pat
  | .acons p _ r => p :: r.unguarded
  | .aguard _ _ _ r => r.unguarded
  | _ => []

/-- Declared ADTs: a constructor's argument types and its type's name. -/
structure Sig where
  ctor : Name → Option (List Ty × Name)

end Flick
