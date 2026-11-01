/-!
# Syntax of λFlick (core.md §1)

Rows are closed sets of names. Type variables are de Bruijn indices `tvar n`;
`∀` appears only in the schemes of a typing context (`Scheme`). Terms carry no
types (Curry style) and use de Bruijn indices; see `formal/README.md`
for the encoding of lists (record fields, match arms, constructor arguments,
environments) as spines.
-/

namespace Flick

abbrev Label := String
abbrev Name := String

/-- A closed row of services or errors: a set of names. Equality of rows is
extensional, which is `≡` of core.md §1.2 for closed rows. -/
abbrev Row := Name → Prop

def Row.none : Row := fun _ => False
def Row.single (S : Name) : Row := fun n => n = S
def Row.union (r r' : Row) : Row := fun n => r n ∨ r' n
/-- `ρ ⊆ ρ'`. -/
def Row.Sub (r r' : Row) : Prop := ∀ n, r n → r' n

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

/-- Monotypes `τ` of core.md §1.2 and the run-time `Env ρ` of §3.6.
A record type is a spine `rcons ℓ τ (… rnil)`; field order is fixed. -/
inductive Ty
  | tvar (n : Nat)
  | base (b : Base)
  | arrow (a : Ty) (φ : Stage) (r : Ty)
  | ref (t : Ty)
  | adt (T : Name)
  | rnil
  | rcons (l : Label) (t rest : Ty)
  | eff (r e : Row) (t : Ty)
  | layer (i e o : Row)
  | errors (e : Row)
  | env (r : Row)

/-- Parallel substitution of type variables. Types have no binders. -/
def Ty.substT (ρ : Nat → Ty) : Ty → Ty
  | .tvar n => ρ n
  | .base b => .base b
  | .arrow a φ r => .arrow (a.substT ρ) φ (r.substT ρ)
  | .ref t => .ref (t.substT ρ)
  | .adt T => .adt T
  | .rnil => .rnil
  | .rcons l t r => .rcons l (t.substT ρ) (r.substT ρ)
  | .eff r e t => .eff r e (t.substT ρ)
  | .layer i e o => .layer i e o
  | .errors e => .errors e
  | .env r => .env r

/-- `τ₁ →P … →P τₙ →P r`. -/
def Ty.arrows : List Ty → Ty → Ty
  | [], r => r
  | a :: rest, r => .arrow a .pure (Ty.arrows rest r)

def Ty.labels : Ty → List Label
  | .rcons l _ r => l :: r.labels
  | _ => []

def Ty.field : Ty → Label → Option Ty
  | .rcons l' t r, l => if l' = l then some t else r.field l
  | _, _ => none

/-- Constants of Γ₀ (core.md §1.5). `Ref.*` are polymorphic in their cell type,
which the typing rule instantiates; the rows of `Layer.*` are closed. -/
inductive Const
  | refMake
  | refGet
  | refSet
  | ask (S : Name)
  | succeed (S : Name)
  | effect (S : Name) (r e : Row)
  | merge (i₁ e₁ o₁ i₂ e₂ o₂ : Row)

def Const.arity : Const → Nat
  | .refSet => 2
  | .merge .. => 2
  | _ => 1

/-- `Layer.*`: a saturated application is a value, built by `provide`. -/
def Const.layer : Const → Prop
  | .succeed _ | .effect .. | .merge .. => True
  | _ => False

/-- The side condition of `Layer.merge`: no common output. -/
def Const.wf : Const → Prop
  | .merge _ _ o₁ _ _ o₂ => ∀ n, ¬ (o₁ n ∧ o₂ n)
  | _ => True

/-- Patterns of core.md §1.3: constructor patterns and record patterns are spines. -/
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

def Pat.binds : Pat → Nat
  | .var => 1
  | .app p q => p.binds + q.binds
  | .rfield _ p r => p.binds + r.binds
  | .or p _ => p.binds
  | _ => 0

/-- Terms of core.md §1.3 and the run-time forms of §3.1. Binders as in P1a:
`lam b` and `let_ e b` bind one variable in `b`; `letrec f b` and `fix f`
bind two in `f` (`0 = x`, `1 = f`) and `letrec` one in `b`; arms bind
`p.binds` in the guard and the body. An environment `θ` is a spine
`econs S v (… enil)`; `raise` is a term former, not a constant. -/
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
  | task (e : Tm)
  | await (e : Tm)
  | raise (e : Tm)
  | try_ (e alts : Tm)
  | handle (v alts : Tm)
  | provide (l e : Tm)
  | with_ (θ e : Tm)
  | enil
  | econs (S : Name) (v θ : Tm)
  | bind (S : Name) (e : Tm)
  | join (e₁ e₂ : Tm)

def Tm.head : Tm → Tm
  | .app f _ => f.head
  | t => t

def Tm.field : Tm → Label → Option Tm
  | .rcons l' e r, l => if l' = l then some e else r.field l
  | _, _ => none

def Tm.setField : Tm → Label → Tm → Option Tm
  | .rcons l' e r, l, w =>
    if l' = l then some (.rcons l' w r) else (r.setField l w).map (.rcons l' e ·)
  | _, _, _ => none

def Tm.unguarded : Tm → List Pat
  | .acons p _ r => p :: r.unguarded
  | .aguard _ _ _ r => r.unguarded
  | _ => []

/-- `θ(S)`. -/
def Tm.envLookup : Tm → Name → Option Tm
  | .econs S' v θ, S => if S' = S then some v else θ.envLookup S
  | _, _ => none

/-- `θ₁ ∪ θ₂`: the first binding of a name wins. -/
def Tm.envAppend : Tm → Tm → Tm
  | .econs S v θ, θ₂ => .econs S v (θ.envAppend θ₂)
  | _, θ₂ => θ₂

/-- `build(v)` of core.md §3.4 on a layer value. -/
def Tm.build : Tm → Option Tm
  | .app (.const (.succeed S)) v => some (.econs S v .enil)
  | .app (.const (.effect S _ _)) v => some (.bind S (.await v))
  | .app (.app (.const (.merge ..)) v₁) v₂ => do
    let b₁ ← v₁.build
    let b₂ ← v₂.build
    pure (.join b₁ b₂)
  | _ => none

/-- Declared ADTs and services: a constructor's argument types and type name;
the record type `S.Impl` of a service. Declarations are closed: ADTs and
services have no type parameters. -/
structure Sig where
  ctor : Name → Option (List Ty × Name)
  impl : Name → Ty
  ctor_closed : ∀ K ts T ρ, ctor K = some (ts, T) → ts.map (Ty.substT ρ) = ts
  impl_closed : ∀ N ρ, (impl N).substT ρ = impl N

/-- The type of a constant at the instance `t` of its type variable. -/
def Const.ty (S : Sig) : Const → Ty → Ty
  | .refMake, t => .arrow t (.rt Row.none Row.none) (.ref t)
  | .refGet, t => .arrow (.ref t) (.rt Row.none Row.none) t
  | .refSet, t => .arrow (.ref t) .pure (.arrow t (.rt Row.none Row.none) (.base .unit))
  | .ask Sv, _ => .arrow (.base .unit) (.rt (Row.single Sv) Row.none) (S.impl Sv)
  | .succeed Sv, _ => .arrow (S.impl Sv) .pure (.layer Row.none Row.none (Row.single Sv))
  | .effect Sv r e, _ => .arrow (.eff r e (S.impl Sv)) .pure (.layer r e (Row.single Sv))
  | .merge i₁ e₁ o₁ i₂ e₂ o₂, _ =>
    .arrow (.layer i₁ e₁ o₁) .pure (.arrow (.layer i₂ e₂ o₂) .pure
      (.layer (Row.union i₁ i₂) (Row.union e₁ e₂) (Row.union o₁ o₂)))

/-- A type scheme `∀ α₀ … αₖ₋₁. t` of a typing context; `k = 0` is a monotype. -/
structure Scheme where
  k : Nat
  t : Ty

def Scheme.mono (t : Ty) : Scheme := ⟨0, t⟩

end Flick
