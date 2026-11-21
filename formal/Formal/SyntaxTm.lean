import Formal.Rows

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

/-- Facts are queried modulo definitional equality of their right-hand type. -/
def Facts.close (Θ : Facts) : Facts := fun C s => ∃ s₀, Θ C s₀ ∧ Ty.Equiv s₀ s

/-- `Θ ⊢ Outlives C ϱ`: each atom of `C` is the region itself or is given to
outlive it. Facts are not chained: a scope gives one fact for everything outside. -/
def Outl (Θ : Facts) (C : Caps) (s : Ty) : Prop :=
  ∀ k, C k → s.reach k ∨ ∃ C', Θ.close C' s ∧ C' k

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
and pass signals.

`lazy e` makes a lazy cell whose initializer is `e`, and `force e` forces the lazy `e`
(lazy.md §3). At run time the location of a lazy cell is `loc ℓ`, and `settle ℓ e` is the
frame of the cell `ℓ` while its initializer `e` runs: it records the outcome. -/
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
  | lazy (e : Tm)
  | force (e : Tm)
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
  | settle (ℓ : Nat) (e : Tm)

/-- The states of a lazy cell as the store holds them (lazy.md §3): `Pending e` is `lazy e`,
`Running e` is `running e`, which keeps the initializer for a signal that resets the cell, and
`Done o` is `done o` with the term of the outcome `o`: `v`, `raise v` or `todo`. No value has
these forms (`IsCell`), so no `Ref` cell holds one; and they are kept apart by their outer form,
which a relation of terms keeps (`Formal/Elab`). -/
@[match_pattern] abbrev Tm.running (e : Tm) : Tm := .force e

abbrev Tm.done (o : Tm) : Tm := .await o

def Tm.IsCell : Tm → Prop
  | .lazy _ | .await _ | .force _ => True
  | _ => False

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

end Flick
