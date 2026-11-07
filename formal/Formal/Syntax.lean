import Formal.TyBase

/-!
# Syntax of λFlick with regions (core.md §1, regions.md §1)

Variables of every kind — types, regions, capture rows — are de Bruijn indices
`tvar n`; a run-time region `π` is `rg π`. A scheme of a context has `∀`, and so does `Ty.all`, a
polymorphic value. Terms carry no types (Curry style) and use de Bruijn
indices; see `formal/README.md` for the encoding of lists (record fields, match
arms, constructor arguments, environments, type arguments) as spines.
-/

namespace Flick

/-- `τ₁ →P … →P τₙ →P r`: the stages of a constructor hold nothing their result
does not show. -/
def Ty.arrows : List Ty → Ty → Ty
  | [], r => r
  | a :: rest, r => .arrow a .pure (Ty.arrows rest r) Caps.none

/-- The arguments `ā` of `D_C ā`: a spine `tapp u₁ (… (tapp uₙ rnil))`. -/
def Ty.spine : List Ty → Ty
  | [] => .rnil
  | u :: us => .tapp u (Ty.spine us)

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

/-- How a handler's arm for an operation returns: `tail` gives
the operation its value, `abort` the handler its value without coming back, and
`control` the handler its value with the rest of the body as `resume`. -/
inductive OpClass
  | tail
  | abort
  | control
  deriving DecidableEq, Repr

/-- Constants of Γ₀ (core.md §1.5, regions.md §1.5). The typing rule instantiates
their types (`CArg`); `open_` acquires a capability `H ϱ`, `op` uses one; `intEq`
compares two `Int`s. `scopeRun` is `Scope.run`, a `scope` whose body is its argument. -/
inductive Const
  | refMake
  | refGet
  | refSet
  | open_
  | op
  | addFin
  | spawn
  | intEq
  | scopeRun
  | perform (E o : Name) (c : OpClass)

def Const.arity : Const → Nat
  | .refSet | .addFin | .spawn | .intEq => 2
  | _ => 1

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

/-- `K q₁ … qₙ`: the pattern of a constructor and of its fields. -/
def Pat.spine (K : Name) (qs : List Pat) : Pat := qs.foldl .app (.ctor K)

def Pat.binds : Pat → Nat
  | .var => 1
  | .app p q => p.binds + q.binds
  | .rfield _ p r => p.binds + r.binds
  | .or p _ => p.binds
  | _ => 0

/-- Terms of core.md §1.3 and regions.md §1.3, and the run-time forms of
core.md §3.1 and regions.md §3.1. Binders: `lam b`, `let_ e b` and
`scope e` bind one variable (`scope` binds the owner); `letrec f b` and `fix f`
bind two in `f` (`0 = x`, `1 = f`) and `letrec` one in `b`; arms bind
`p.binds` in the guard and the body. `raise` is a term former, not a constant. `drec c r` is a
dictionary record of λFlick⁺ with the fields `r`, an `rcons` spine; it is a value and
`proj` reads its fields. `glob g` is a global: an instance, a structural or primitive
dictionary or a constant of Γ₀, which steps to its value in `Σ₀`.

`hdl E al r e` handles the effect `E` in `e` with the arms `al`, an `acons` spine of
patterns `Pat.spine o [.var]` (`[.var, .var]` for `control`, with `resume`); `r` takes
the body's value. At run time a handler is the region `p` and the cell `ℓ`: `hwith p ℓ`
around the body, `hctl p ℓ κ tl` while an arm runs and the body waits at `hole p`, `hres p
a b` while the resumed body `b` runs and the arm `a` waits, `hdone p a` after; `unw p ℓ a`
unwinds to the handler, which then runs `a`; `kont p κ` is `resume`. A child or a finalizer
keeps the handlers around where it started: `hkeep q ℓ E al e` around its body answers the
`tail` operations of `E`, `hkarm q ℓ E al a k` while an arm of it runs. They run no `return`,
and pass signals. -/
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
  | scope (e : Tm)
  | inside (p : Nat) (e : Tm)
  | cap (p : Nat)
  | own (p : Nat)
  | drec (c : Nat) (r : Tm)
  | glob (g : Nat)
  | hdl (E : Name) (al r e : Tm)
  | hwith (p ℓ : Nat) (E : Name) (al r e : Tm)
  | hctl (p ℓ κ : Nat) (tl : Bool) (E : Name) (al r a k : Tm)
  | hres (p ℓ : Nat) (a b : Tm)
  | hdone (p : Nat) (a : Tm)
  | unw (p ℓ : Nat) (a : Tm)
  | kont (p ℓ : Nat)
  | hole (p : Nat)
  | hkeep (q ℓ : Nat) (E : Name) (al e : Tm)
  | hkarm (q ℓ : Nat) (E : Name) (al a k : Tm)

def Tm.head : Tm → Tm
  | .app f _ => f.head
  | t => t

def Tm.field : Tm → Label → Option Tm
  | .rcons l' e r, l => if l' = l then some e else r.field l
  | .drec _ r, l => r.field l
  | _, _ => none

def Tm.setField : Tm → Label → Tm → Option Tm
  | .rcons l' e r, l, w =>
    if l' = l then some (.rcons l' w r) else (r.setField l w).map (.rcons l' e ·)
  | _, _, _ => none

def Tm.unguarded : Tm → List Pat
  | .acons p _ r => p :: r.unguarded
  | .aguard _ _ _ r => r.unguarded
  | _ => []

/-- A type scheme `∀ α₀ … αₖ₋₁. Θ ⇒ t` of a typing context; `k = 0` and no
facts is a monotype. -/
structure Scheme where
  k : Nat
  cs : Facts
  t : Ty

def Scheme.mono (t : Ty) : Scheme := ⟨0, Facts.none, t⟩

/-- A scheme that mentions no atom but the variables below `n`. -/
def Scheme.Closed (n : Nat) (s : Scheme) : Prop :=
  ∀ σ σ' : Atom → Ty, (∀ i, i < n → σ (.v i) = σ' (.v i)) →
    s.t.substT σ = s.t.substT σ' ∧ s.cs.substT σ = s.cs.substT σ'

/-- `α₀ … αₖ₋₁`. -/
def idTs (k : Nat) : List Ty := (List.range k).map Ty.tvar

/-- Declared ADTs and effects. A constructor has the number of parameters of its
type, its argument types over them and its type name; its fields mention nothing
but the parameters (`ctor_closed`, `ctor_reach`). The record type `S.Impl` of an
effect, what `Σ₀` gives for it, is closed. A class `c` of λFlick⁺ has `k` parameters and fields with their
schemes, which mention the parameters below their own binders (`dict_closed`); a global
has a closed scheme. -/
structure Sig where
  ctor : Name → Option (Nat × List Ty × Name)
  impl : Name → Ty
  ctor_closed : ∀ K k ts T σ, ctor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → ts.map (Ty.substT σ) = ts
  ctor_reach : ∀ K k ts T, ctor K = some (k, ts, T) →
    ∀ t ∈ ts, ∀ a, t.reach a → ∃ i, i < k ∧ a = .v i
  /-- The arguments of the type a constructor builds, over its `k` variables:
  `α₀ … αₖ₋₁` of an ordinary constructor, any types of one of a GADT. -/
  res : Name → List Ty
  res_closed : ∀ K k ts T σ, ctor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → (res K).map (Ty.substT σ) = res K
  impl_closed : ∀ N σ, (impl N).substT σ = impl N
  dict : Nat → Option (Nat × List (Label × Scheme))
  glob : Nat → Option Scheme
  /-- The operation `o` of the effect `E`: its class, argument and result, closed;
  an `abort` one returns any type. -/
  op : Name → Name → Option (OpClass × Ty × Ty)
  op_closed : ∀ E o c a b σ, op E o = some (c, a, b) → a.substT σ = a ∧ b.substT σ = b
  /-- An effect with an operation that is not `tail` has no root: its record has no
  value (E016). -/
  op_impl : ∀ E o c a b, op E o = some (c, a, b) → c ≠ .tail → impl E = .errors Row.none
  /-- The record of any other effect has a function for each operation, of no row:
  a root answers it without anything around. -/
  op_field : ∀ E o a b, op E o = some (.tail, a, b) → impl E ≠ .errors Row.none →
    (impl E).field o = some (.arrow a (.rt Row.none Row.none) b Caps.none)
  dict_closed : ∀ c k fs, dict c = some (k, fs) → ∀ l s, (l, s) ∈ fs → s.Closed (s.k + k)
  glob_closed : ∀ g s, glob g = some s → s.Closed s.k

/-- The instance of a constant's type variables. -/
structure CArg where
  t : Ty
  s : Ty
  C : Caps
  r : Row
  e : Row

instance : Inhabited CArg :=
  ⟨⟨.base .unit, .base .unit, Caps.none, Row.none, Row.none⟩⟩

/-- `Scope ϱ` as a row. -/
def Row.scope (s : Ty) : Row := Row.ofCaps s.reach

/-- `Fiber.Provided r s` of regions.md §2.7 as row elements: one obligation for each
service and each row variable of `r`. -/
def Row.obs (r : Row) (s : Ty) : Row
  | .ob N a => r (.nm N) ∧ s.reach a
  | .obv v a => r (.rv v) ∧ s.reach a
  | _ => False

/-- The type of a constant at an instance. A spawned child and a finalizer are
`Eff` with qualifier `C`; their owner is `s`. -/
def Const.ty (S : Sig) : Const → CArg → Ty
  | .refMake, ι => .arrow ι.t (.rt Row.none Row.none) (.ref ι.t) Caps.none
  | .refGet, ι => .arrow (.ref ι.t) (.rt Row.none Row.none) ι.t Caps.none
  | .refSet, ι =>
    .arrow (.ref ι.t) .pure (.arrow ι.t (.rt Row.none Row.none) (.base .unit) Caps.none) Caps.none
  | .open_, ι => .arrow (.owner ι.s) (.rt (Row.scope ι.s) Row.none) (.hnd ι.s) Caps.none
  | .op, ι => .arrow (.hnd ι.s) (.rt Row.none Row.none) (.base .unit) Caps.none
  | .addFin, ι =>
    .arrow (.owner ι.s) .pure (.arrow (.eff ι.r Row.none (.base .unit) ι.C)
      (.rt (Row.union (Row.scope ι.s) (Row.union ι.r (Row.obs ι.r ι.s))) Row.none) (.base .unit)
      ι.s.reach) Caps.none
  | .spawn, ι =>
    .arrow (.owner ι.s) .pure (.arrow (.eff (Row.union (Row.scope ι.s) ι.r) ι.e ι.t ι.C)
      (.rt (Row.union ι.r (Row.obs ι.r ι.s)) Row.none) (.base .unit) ι.s.reach) Caps.none
  | .intEq, _ => .arrow (.base .int) .pure (.arrow (.base .int) .pure (.base .bool) Caps.none) Caps.none
  | .scopeRun, ι =>
    let r := ι.r.substT (shiftV 1)
    .arrow (.all 1 (.arrow (.owner (.tvar 0))
        (.rt (Row.union (Row.scope (.tvar 0)) (Row.union r (Row.obs r (.tvar 0)))) (ι.e.substT (shiftV 1)))
        (ι.t.up 1) (ι.C.substT (shiftV 1))))
      (.rt ι.r ι.e) ι.t Caps.none
  | .perform E o _, ι => match S.op E o with
    | some (.abort, a, _) => .arrow a (.rt (Row.single E) Row.none) ι.t Caps.none
    | some (_, a, b) => .arrow a (.rt (Row.single E) Row.none) b Caps.none
    | none => .arrow (.base .unit) (.rt (Row.single E) Row.none) (.base .unit) Caps.none

/-- Side conditions of a constant at an instance: a finalizer and a child outlive their
owner, with everything they hold
(`docs/formal/regions.md#spawn-bounds-everything-the-task-holds`). -/
def Const.ok (Θ : Facts) : Const → CArg → Prop
  | .addFin, ι => Outl Θ ι.C ι.s
  | .spawn, ι => Outl Θ (fun a => ι.C a ∨ ι.t.reach a) ι.s
  | .perform _ _ _, _ => False
  | _, _ => True

end Flick
