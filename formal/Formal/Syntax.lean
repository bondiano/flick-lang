/-!
# Syntax of λFlick with regions (core.md §1, regions.md §1)

Variables of every kind — types, regions, capture rows — are de Bruijn indices
`tvar n`; a run-time region `π` is `rg π`. `∀` appears only in the schemes of a
typing context (`Scheme`). Terms carry no types (Curry style) and use de Bruijn
indices; see `formal/README.md` for the encoding of lists (record fields, match
arms, constructor arguments, environments, type arguments) as spines.
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
obligations `{Provided {S} ϱ}` not yet discharged (`ob S ϱ`, regions.md §2.7) and, at
run time, the uses `need S π` of a service `S` that a `with` frame of the `provide`
region `π` answers. -/
inductive Elem
  | nm (N : Name)
  | atm (a : Atom)
  | ob (N : Name) (a : Atom)
  | need (N : Name) (a : Atom)

instance : Coe Name Elem := ⟨Elem.nm⟩

/-- A closed row of services or errors. Equality of rows is extensional, which is
`≡` of core.md §1.2 for closed rows. -/
abbrev Row := Elem → Prop

def Row.none : Row := fun _ => False
def Row.single (S : Name) : Row := fun n => n = .nm S
def Row.union (r r' : Row) : Row := fun n => r n ∨ r' n
/-- `ρ ⊆ ρ'`. -/
def Row.Sub (r r' : Row) : Prop := ∀ n, r n → r' n
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

/-- Types `τ` of core.md §1.2 and regions.md §1.2, and the run-time `Env ρ`.
A record type is a spine `rcons ℓ τ (… rnil)`; field order is fixed. A nominal
type `T ā` is a spine `tapp (… (tapp (adt T) a₁) …) aₙ`. A region is `tvar n` or
`rg π`; a capture row is `tvar n` or `caps C`. The hidden qualifier `C` of a
function or a computation is part of `arrow` and `eff`; that of data with hidden
storage is a parameter of kind `CRow` of its type. `hnd ϱ` is `H ϱ`. -/
inductive Ty
  | tvar (n : Nat)
  | rg (p : Nat)
  | caps (C : Caps)
  | base (b : Base)
  | arrow (a : Ty) (φ : Stage) (b : Ty) (C : Caps)
  | ref (t : Ty)
  | adt (T : Name)
  | tapp (f a : Ty)
  | rnil
  | rcons (l : Label) (t rest : Ty)
  | eff (r e : Row) (t : Ty) (C : Caps)
  | layer (i e o : Row)
  | errors (e : Row)
  | env (r : Row)
  | hnd (s : Ty)
  | owner (s : Ty)

/-- `⌈τ⌉` of regions.md §2.1: the atoms a value of type `τ` may reach. Rows of
services are not in it; a row of errors contributes its atoms. -/
def Ty.reach : Ty → Caps
  | .tvar n => fun a => a = .v n
  | .rg p => fun a => a = .r p
  | .caps C => C
  | .arrow a _ b C => fun x => a.reach x ∨ b.reach x ∨ C x
  | .ref t => t.reach
  | .tapp f a => fun x => f.reach x ∨ a.reach x
  | .rcons _ t r => fun x => t.reach x ∨ r.reach x
  | .eff _ _ t C => fun x => t.reach x ∨ C x
  | .errors e => fun x => e (.atm x)
  | .hnd s => s.reach
  | .owner s => s.reach
  | _ => Caps.none

/-- A set of atoms after a substitution: each atom becomes `⌈σ a⌉`. -/
def Caps.substT (σ : Atom → Ty) (C : Caps) : Caps := fun a => ∃ b, C b ∧ (σ b).reach a

def Row.substT (σ : Atom → Ty) (r : Row) : Row
  | .nm n => r (.nm n)
  | .atm a => ∃ b, r (.atm b) ∧ (σ b).reach a
  | .ob n a => ∃ b, r (.ob n b) ∧ (σ b).reach a
  | .need n a => ∃ b, r (.need n b) ∧ (σ b).reach a

def Stage.substT (σ : Atom → Ty) : Stage → Stage
  | .pure => .pure
  | .rt r e => .rt (r.substT σ) (e.substT σ)

/-- Parallel substitution of variables and run-time regions. Types have no binders. -/
def Ty.substT (σ : Atom → Ty) : Ty → Ty
  | .tvar n => σ (.v n)
  | .rg p => σ (.r p)
  | .caps C => .caps (C.substT σ)
  | .base b => .base b
  | .arrow a φ b C => .arrow (a.substT σ) (φ.substT σ) (b.substT σ) (C.substT σ)
  | .ref t => .ref (t.substT σ)
  | .adt T => .adt T
  | .tapp f a => .tapp (f.substT σ) (a.substT σ)
  | .rnil => .rnil
  | .rcons l t r => .rcons l (t.substT σ) (r.substT σ)
  | .eff r e t C => .eff (r.substT σ) (e.substT σ) (t.substT σ) (C.substT σ)
  | .layer i e o => .layer (i.substT σ) (e.substT σ) (o.substT σ)
  | .errors e => .errors (e.substT σ)
  | .env r => .env (r.substT σ)
  | .hnd s => .hnd (s.substT σ)
  | .owner s => .owner (s.substT σ)

/-- The identity substitution. -/
def Ty.ident : Atom → Ty
  | .v n => .tvar n
  | .r p => .rg p

/-- `τ₁ →P … →P τₙ →P r`: the stages of a constructor hold nothing their result
does not show. -/
def Ty.arrows : List Ty → Ty → Ty
  | [], r => r
  | a :: rest, r => .arrow a .pure (Ty.arrows rest r) Caps.none

/-- `T ā`. -/
def Ty.tcon (T : Name) (us : List Ty) : Ty := us.foldl .tapp (.adt T)

/-- The head of a spine of type applications. -/
def Ty.hd : Ty → Ty
  | .tapp f _ => f.hd
  | t => t

def Ty.labels : Ty → List Label
  | .rcons l _ r => l :: r.labels
  | _ => []

def Ty.field : Ty → Label → Option Ty
  | .rcons l' t r, l => if l' = l then some t else r.field l
  | _, _ => none

/-- Facts `Θ` of regions.md §2.4: `Θ C ϱ` is `Outlives C ϱ`. -/
abbrev Facts := Caps → Ty → Prop

def Facts.none : Facts := fun _ _ => False
def Facts.union (Θ Θ' : Facts) : Facts := fun C s => Θ C s ∨ Θ' C s
def Facts.substT (σ : Atom → Ty) (Θ : Facts) : Facts :=
  fun C s => ∃ C₀ s₀, Θ C₀ s₀ ∧ C = C₀.substT σ ∧ s = s₀.substT σ

/-- `Θ ⊢ Outlives C ϱ`: each atom of `C` is the region itself or is given to
outlive it. Facts are not chained: a scope gives one fact for everything outside. -/
def Outl (Θ : Facts) (C : Caps) (s : Ty) : Prop :=
  ∀ k, C k → s.reach k ∨ ∃ C', Θ C' s ∧ C' k

/-- Constants of Γ₀ (core.md §1.5, regions.md §1.5). The typing rule instantiates
their types (`CArg`); `open_` acquires a capability `H ϱ`, `op` uses one. -/
inductive Const
  | refMake
  | refGet
  | refSet
  | ask (S : Name)
  | succeed (S : Name)
  | effect (S : Name)
  | merge
  | open_
  | op
  | addFin
  | spawn

def Const.arity : Const → Nat
  | .refSet | .merge | .addFin | .spawn => 2
  | _ => 1

/-- `Layer.*`: a saturated application is a value, built by `provide`. -/
def Const.layer : Const → Prop
  | .succeed _ | .effect _ | .merge => True
  | _ => False

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

/-- Terms of core.md §1.3 and regions.md §1.3, and the run-time forms of
core.md §3.1 and regions.md §3.1. Binders as in P1: `lam b`, `let_ e b` and
`scope e` bind one variable (`scope` binds the owner); `letrec f b` and `fix f`
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
  | with_ (p : Nat) (θ e : Tm)
  | enil
  | econs (S : Name) (v θ : Tm)
  | bind (S : Name) (e : Tm)
  | join (e₁ e₂ : Tm)
  | scope (e : Tm)
  | inside (p : Nat) (e : Tm)
  | cap (p : Nat)
  | own (p : Nat)

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
  | .app (.const (.effect S)) v => some (.bind S (.await v))
  | .app (.app (.const .merge) v₁) v₂ => do
    let b₁ ← v₁.build
    let b₂ ← v₂.build
    pure (.join b₁ b₂)
  | _ => none

/-- Declared ADTs and services. A constructor has the number of parameters of its
type, its argument types over them and its type name; its fields mention nothing
but the parameters (`ctor_closed`, `ctor_reach`). The record type `S.Impl` of a
service is closed. -/
structure Sig where
  ctor : Name → Option (Nat × List Ty × Name)
  impl : Name → Ty
  ctor_closed : ∀ K k ts T σ, ctor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → ts.map (Ty.substT σ) = ts
  ctor_reach : ∀ K k ts T, ctor K = some (k, ts, T) →
    ∀ t ∈ ts, ∀ a, t.reach a → ∃ i, i < k ∧ a = .v i
  impl_closed : ∀ N σ, (impl N).substT σ = impl N

/-- The instance of a constant's type variables. -/
structure CArg where
  t : Ty
  s : Ty
  C : Caps
  r : Row
  e : Row
  o : Row
  r' : Row
  e' : Row
  o' : Row

instance : Inhabited CArg :=
  ⟨⟨.base .unit, .base .unit, Caps.none, Row.none, Row.none, Row.none, Row.none, Row.none, Row.none⟩⟩

/-- `Scope ϱ` as a row. -/
def Row.scope (s : Ty) : Row := Row.ofCaps s.reach

/-- `Fiber.Provided r s` of regions.md §2.7 as row elements: one obligation for each
service of `r`. -/
def Row.obs (r : Row) (s : Ty) : Row
  | .ob N a => r (.nm N) ∧ s.reach a
  | _ => False

/-- An atom as a type. -/
def Atom.ty : Atom → Ty
  | .v n => .tvar n
  | .r p => .rg p

/-- The type of a constant at an instance. A spawned child and a finalizer are
`Eff` with qualifier `C`; their owner is `s`. -/
def Const.ty (S : Sig) : Const → CArg → Ty
  | .refMake, ι => .arrow ι.t (.rt Row.none Row.none) (.ref ι.t) Caps.none
  | .refGet, ι => .arrow (.ref ι.t) (.rt Row.none Row.none) ι.t Caps.none
  | .refSet, ι =>
    .arrow (.ref ι.t) .pure (.arrow ι.t (.rt Row.none Row.none) (.base .unit) Caps.none) Caps.none
  | .ask Sv, _ => .arrow (.base .unit) (.rt (Row.single Sv) Row.none) (S.impl Sv) Caps.none
  | .succeed Sv, _ => .arrow (S.impl Sv) .pure (.layer Row.none Row.none (Row.single Sv)) Caps.none
  | .effect Sv, ι =>
    .arrow (.eff ι.r ι.e (S.impl Sv) Caps.none) .pure (.layer ι.r ι.e (Row.single Sv)) Caps.none
  | .merge, ι =>
    .arrow (.layer ι.r ι.e ι.o) .pure (.arrow (.layer ι.r' ι.e' ι.o') .pure
      (.layer (Row.union ι.r ι.r') (Row.union ι.e ι.e') (Row.union ι.o ι.o')) Caps.none) Caps.none
  | .open_, ι => .arrow (.owner ι.s) (.rt (Row.scope ι.s) Row.none) (.hnd ι.s) Caps.none
  | .op, ι => .arrow (.hnd ι.s) (.rt Row.none Row.none) (.base .unit) Caps.none
  | .addFin, ι =>
    .arrow (.owner ι.s) .pure (.arrow (.eff ι.r Row.none (.base .unit) ι.C)
      (.rt (Row.union (Row.scope ι.s) (Row.union ι.r (Row.obs ι.r ι.s))) Row.none) (.base .unit)
      ι.s.reach) Caps.none
  | .spawn, ι =>
    .arrow (.owner ι.s) .pure (.arrow (.eff (Row.union (Row.scope ι.s) ι.r) ι.e ι.t ι.C)
      (.rt (Row.union ι.r (Row.obs ι.r ι.s)) Row.none) (.base .unit) ι.s.reach) Caps.none

/-- Side conditions of a constant at an instance: `Layer.merge` has disjoint
outputs; a finalizer and a child outlive their owner, with everything they hold
(Р140). -/
def Const.ok (Θ : Facts) : Const → CArg → Prop
  | .merge, ι => ∀ N, ¬ (ι.o (.nm N) ∧ ι.o' (.nm N))
  | .addFin, ι => Outl Θ ι.C ι.s ∧ ∀ N a, ¬ ι.r (.need N a)
  | .spawn, ι => Outl Θ (fun a => ι.C a ∨ ι.t.reach a) ι.s ∧ ∀ N a, ¬ ι.r (.need N a)
  | _, _ => True

/-- A type scheme `∀ α₀ … αₖ₋₁. Θ ⇒ t` of a typing context; `k = 0` and no
facts is a monotype. -/
structure Scheme where
  k : Nat
  cs : Facts
  t : Ty

def Scheme.mono (t : Ty) : Scheme := ⟨0, Facts.none, t⟩

end Flick
