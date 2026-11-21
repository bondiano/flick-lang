import Formal.SyntaxTm

/-!
# Schemes, variance polarity, signatures, and constants (`Formal.Syntax`)

Continues `Formal.SyntaxTm` with type schemes, variance polarity (`Ty.Pol`,
`Ty.PolD`), signatures (`Sig`), and constant typing (`Const.ty`, `Const.ok`).
-/

namespace Flick

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

/-- The variable `v i` replaced by `∅`, the other atoms kept. -/
def dropV (i : Nat) (a : Atom) : Ty := if a = .v i then .caps Caps.none else Ty.ident a

/-- `t` does not depend on the variable `v i`: replacing it by `∅` changes nothing. -/
def Ty.NoV (i : Nat) (t : Ty) : Prop := t.substT (dropV i) = t

/-- `φ` does not depend on the variable `v i`. -/
def Stage.NoV (i : Nat) (φ : Stage) : Prop := φ.substT (dropV i) = φ

/-- `Θ` does not depend on the variable `v i`. -/
def Facts.NoV (i : Nat) (Θ : Facts) : Prop := Θ.substT (dropV i) = Θ

/-- Where the hidden row `v i` of data with hidden storage may stand in a field
(regions.md §2.2): nowhere, as the qualifier of a stored function or computation, or as the
hidden row of stored data, so that widening the row widens the field. -/
def Ty.CovRow (hid : Name → Option Nat) (i : Nat) (f : Ty) : Prop :=
  f.NoV i ∨
  (∃ a φ b C, f = .arrow a φ b C ∧ a.NoV i ∧ φ.NoV i ∧ b.NoV i) ∨
  (∃ r e t C, f = .eff r e t C ∧ r.NoV i ∧ e.NoV i ∧ t.NoV i) ∨
  (∃ T us j, f = Ty.tcon T us ∧ hid T = some j ∧ ∀ l u, us[l]? = some u → l ≠ j → u.NoV i)

/-- The variance of a parameter of data (regions.md §2.2.1): covariant, contravariant,
invariant, or unused. -/
inductive Variance
  | pos | neg | inv | bi
  deriving DecidableEq

/-- A type subtyping cannot move up or down: a scalar or a nominal type without
arguments (regions.md §2.2.1, A2). -/
def Ty.Closed : Ty → Prop
  | .base _ | .adt _ => True
  | _ => False

/-- A parameter of variance `s` may stand at polarity `q` (`true`: covariant). -/
def Variance.at (s : Variance) (q : Bool) : Prop :=
  (q = true ∧ s = .pos) ∨ (q = false ∧ s = .neg)

/-- Where a parameter `v i` of variance `s` may stand in a field, at polarity `q`
(regions.md §2.2.1): nowhere, as itself where its variance allows, in the parameter
(flipped) or the result or the qualifier of a stored function, in the result or the
qualifier of a stored computation, in a field of a record as in the record itself, or in
an argument of other data by the variance of that argument. A covariant parameter never stands below a flip:
there its argument would grow what the field reaches where subtyping asks it to shrink
(`LeLive.reach`). Anything else (`Ref`, a handle, an owner, a dictionary, a stage) holds it
only if it does not depend on it. -/
inductive Ty.Pol (var : Name → Nat → Variance) (i : Nat) (s : Variance) : Bool → Ty → Prop
  | none {q t} : t.NoV i → Ty.Pol var i s q t
  | var {q} : s.at q → Ty.Pol var i s q (.tvar i)
  | arrow {q a φ b C} : (s = .pos → a.NoV i) → Ty.Pol var i s (!q) a → φ.NoV i →
      Ty.Pol var i s q b → ((Ty.caps C).NoV i ∨ s.at q) → Ty.Pol var i s q (.arrow a φ b C)
  | eff {q r e t C} : r.NoV i → e.NoV i → Ty.Pol var i s q t →
      ((Ty.caps C).NoV i ∨ s.at q) → Ty.Pol var i s q (.eff r e t C)
  | rcons {q l t r} : Ty.Pol var i s q t → Ty.Pol var i s q r → Ty.Pol var i s q (.rcons l t r)
  | data {q t T us} :
      (∀ j u, us[j]? = some u → var T j = .pos → Ty.Pol var i s q u) →
      (∀ j u, us[j]? = some u → var T j = .neg → s = .pos → u.NoV i) →
      (∀ j u, us[j]? = some u → var T j = .neg → Ty.Pol var i s (!q) u) →
      (∀ j u, us[j]? = some u → var T j = .inv → u.NoV i) →
      (∀ j u, us[j]? = some u → var T j = .bi → ¬ s.at q → u.NoV i) →
      t = Ty.tcon T us → Ty.Pol var i s q t

/-- Like `Ty.Pol`, also descending into dictionary types `D_C ūs` by their class parameter
variances `dvar`: used for the methods and superclass slots of a dictionary record
(`Sig.dictVar_ok`). -/
inductive Ty.PolD (var : Name → Nat → Variance) (dvar : Nat → Nat → Variance)
    (i : Nat) (s : Variance) : Bool → Ty → Prop
  | none {q t} : t.NoV i → Ty.PolD var dvar i s q t
  | var {q} : s.at q → Ty.PolD var dvar i s q (.tvar i)
  | arrow {q a φ b C} : (s = .pos → a.NoV i) → Ty.PolD var dvar i s (!q) a → φ.NoV i →
      Ty.PolD var dvar i s q b → ((Ty.caps C).NoV i ∨ s.at q) → Ty.PolD var dvar i s q (.arrow a φ b C)
  | eff {q r e t C} : r.NoV i → e.NoV i → Ty.PolD var dvar i s q t →
      ((Ty.caps C).NoV i ∨ s.at q) → Ty.PolD var dvar i s q (.eff r e t C)
  | rcons {q l t r} : Ty.PolD var dvar i s q t → Ty.PolD var dvar i s q r →
      Ty.PolD var dvar i s q (.rcons l t r)
  | data {q t T us} :
      (∀ j u, us[j]? = some u → var T j = .pos → Ty.PolD var dvar i s q u) →
      (∀ j u, us[j]? = some u → var T j = .neg → s = .pos → u.NoV i) →
      (∀ j u, us[j]? = some u → var T j = .neg → Ty.PolD var dvar i s (!q) u) →
      (∀ j u, us[j]? = some u → var T j = .inv → u.NoV i) →
      (∀ j u, us[j]? = some u → var T j = .bi → ¬ s.at q → u.NoV i) →
      t = Ty.tcon T us → Ty.PolD var dvar i s q t
  | dict {q c us} :
      (∀ j u, us[j]? = some u → dvar c j = .pos → Ty.PolD var dvar i s q u) →
      (∀ j u, us[j]? = some u → dvar c j = .neg → s = .pos → u.NoV i) →
      (∀ j u, us[j]? = some u → dvar c j = .neg → Ty.PolD var dvar i s (!q) u) →
      (∀ j u, us[j]? = some u → (dvar c j = .inv ∨ dvar c j = .bi) → u.NoV i) →
      Ty.PolD var dvar i s q (.dict c (Ty.spine us))

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
  /-- The parameters of an effect or a service `E`: its operations and its record are over the
  variables `0 … arity E - 1`, which an instance `E ū` fills. -/
  arity : Name → Nat := fun _ => 0
  impl_closed : ∀ N σ, (∀ i, i < arity N → σ (.v i) = .tvar i) → (impl N).substT σ = impl N
  dict : Nat → Option (Nat × List (Label × Scheme))
  glob : Nat → Option Scheme
  /-- The operation `o` of the effect `E`: its class, argument and result, over the parameters
  of `E`; an `abort` one returns any type. -/
  op : Name → Name → Option (OpClass × Ty × Ty)
  op_closed : ∀ E o c a b σ, op E o = some (c, a, b) → (∀ i, i < arity E → σ (.v i) = .tvar i) →
    a.substT σ = a ∧ b.substT σ = b
  /-- An effect with an operation that is not `tail` has no root: its record has no
  value (E016). -/
  op_impl : ∀ E o c a b, op E o = some (c, a, b) → c ≠ .tail → impl E = .errors .cnil
  /-- The record of any other effect has a function for each operation, of no row:
  a root answers it without anything around. -/
  op_field : ∀ E o a b, op E o = some (.tail, a, b) → impl E ≠ .errors .cnil →
    (impl E).field o = some (.arrow a (.rt .cnil .cnil) b Caps.none)
  dict_closed : ∀ c k fs, dict c = some (k, fs) → ∀ l s, (l, s) ∈ fs → s.Closed (s.k + k)
  glob_closed : ∀ g s, glob g = some s → s.Closed s.k
  /-- The hidden row of data with hidden storage: the parameter the reading of a signature
  fills (regions.md §1.4). Its constructors are ordinary at it, and it stands in their
  fields only where widening it widens them (`Ty.CovRow`), so subtyping may widen it; nothing
  else a constructor builds depends on it, so the constructor still builds the widened type.
  A row a declaration names is no hidden row. -/
  hidden : Name → Option Nat := fun _ => none
  hidden_ok : ∀ T i, hidden T = some i → ∀ K k ts, ctor K = some (k, ts, T) →
    i < k ∧ (res K)[i]? = some (.tvar i) ∧
      (∀ j u, (res K)[j]? = some u → j ≠ i → u.NoV i) ∧ ∀ f ∈ ts, f.CovRow hidden i := by
    intro _ _ h; simp at h
  /-- The variance of each parameter of data (regions.md §2.2.1). Nothing else a
  constructor builds depends on a parameter that is not invariant. Either it is an ordinary
  variable of the constructor, held by the fields only where its variance allows (`Ty.Pol`;
  an unused one is in no field), or (A2) the constructor fixes it to a type subtyping cannot
  move (`Ty.Closed`) and does not use it at all. -/
  var : Name → Nat → Variance := fun _ _ => .inv
  var_ok : ∀ T i, var T i ≠ .inv → ∀ K k ts, ctor K = some (k, ts, T) →
      (∀ j u, (res K)[j]? = some u → j ≠ i → u.NoV i) ∧
      ((i < k ∧ (res K)[i]? = some (.tvar i) ∧ ∀ f ∈ ts, (var T i = .bi → f.NoV i) ∧
          (var T i = .pos ∨ var T i = .neg → Ty.Pol var i (var T i) true f)) ∨
        (var T i ≠ .bi ∧ (∃ c, (res K)[i]? = some c ∧ c.Closed) ∧ ∀ f ∈ ts, f.NoV i)) := by
    intro _ _ h; exact absurd rfl h
  /-- The variance of each parameter of a dictionary record `D_C ᾱ` (regions.md §2.2.1):
  inferred from its methods and superclass slots. Every field scheme is independent of
  non-invariant parameters in its value-bound facts `cs`, and respects the parameter's sign
  in its type (`Ty.PolD`). -/
  dictVar : Nat → Nat → Variance := fun _ _ => .inv
  dictVar_ok : ∀ c k fs i, dict c = some (k, fs) → dictVar c i ≠ .inv →
      ∀ l s, (l, s) ∈ fs → s.cs.NoV (s.k + i) ∧
        (dictVar c i = .bi → s.t.NoV (s.k + i)) ∧
        (dictVar c i = .pos ∨ dictVar c i = .neg →
          Ty.PolD var dictVar (s.k + i) (dictVar c i) true s.t) := by
    intro _ _ _ _ _ h; exact absurd rfl h

/-- The instance of a constant's type variables; `r` and `e` are rows. -/
structure CArg where
  t : Ty
  s : Ty
  C : Caps
  r : Ty
  e : Ty

instance : Inhabited CArg :=
  ⟨⟨.base .unit, .base .unit, Caps.none, .cnil, .cnil⟩⟩

/-- `Scope ϱ` as a row. -/
def Row.scope (s : Ty) : Row := Row.ofCaps s.reach

/-- The type of a constant at an instance. A spawned child and a finalizer are
`Eff` with qualifier `C`; their owner is `s`. A cell stores `t`, and `C` is its capacity,
the bound `α^c` of `Ref.*`: storing into a cell needs the capacity live. -/
def Const.ty : Const → CArg → Ty
  | .refMake, ι => .arrow ι.t (.rt .cnil .cnil) (.ref ι.t) Caps.none
  | .refGet, ι => .arrow (.ref ι.t) (.rt .cnil .cnil) ι.t Caps.none
  | .refSet, ι =>
    .arrow (.ref ι.t) .pure (.arrow ι.t (.rt .cnil .cnil) (.base .unit) ι.C) Caps.none
  | .open_, ι => .arrow (.owner ι.s) (.rt (.caps ι.s.reach) .cnil) (.hnd ι.s) Caps.none
  | .op, ι =>
    .arrow (.hnd ι.s) (.rt (.caps ι.s.reach) .cnil) (.base .unit) Caps.none
  | .addFin, ι =>
    .arrow (.owner ι.s) .pure (.arrow (.eff ι.r .cnil (.base .unit) ι.C)
      (.rtob (.run (.caps ι.s.reach) ι.r) ι.s .cnil) (.base .unit) ι.s.reach) Caps.none
  | .spawn, ι =>
    .arrow (.owner ι.s) .pure (.arrow (.eff (.run (.caps ι.s.reach) ι.r) ι.e ι.t ι.C)
      (.rtob ι.r ι.s .cnil) (.base .unit) ι.s.reach) Caps.none
  | .intEq, _ => .arrow (.base .int) .pure (.arrow (.base .int) .pure (.base .bool) Caps.none) Caps.none
  | .scopeRun, ι =>
    let r := ι.r.up 1
    .arrow (.all 1 (.arrow (.owner (.tvar 0))
        (.rtob (.run (.caps (Ty.tvar 0).reach) r) (.tvar 0) (ι.e.up 1))
        (ι.t.up 1) (ι.C.substT (shiftV 1))))
      (.rt ι.r ι.e) ι.t Caps.none
  -- an operation is typed by (Perform) at the arguments of its effect, never by (Const)
  | .perform E _ _, _ => .arrow (.base .unit) (.rt (.rnm E (.adt E)) .cnil) (.base .unit) Caps.none

/-- Regions required by a work row must outlive the owner that keeps that work.
Row variables are included: instantiating one may introduce a Live atom. -/
def Row.Outlives (Θ : Facts) (r : Ty) (s : Ty) : Prop := Outl Θ r.reach s

/-- Side conditions of a constant at an instance: a finalizer and a child outlive their
owner, with everything they hold
(`docs/formal/regions.md#spawn-bounds-everything-the-task-holds`); a cell's capacity
bounds what the cell stores, `⌈t⌉ ⊆ C`, the premise (Inst) gives the value bound `α^c`. -/
def Const.ok (Θ : Facts) : Const → CArg → Prop
  | .addFin, ι =>
    Outl Θ ι.C ι.s ∧ Row.Outlives Θ ι.r ι.s ∧ Row.Sub (Row.ofCaps ι.C) ι.r.asRow
  | .scopeRun, ι => Row.Sub (Row.ofCaps ι.C) ι.r.asRow
  | .spawn, ι =>
    Outl Θ (fun a => ι.C a ∨ ι.t.reach a) ι.s ∧ Row.Outlives Θ ι.r ι.s ∧
      Row.Sub (Row.ofCaps ι.C) ι.r.asRow
  | .refMake, ι | .refGet, ι | .refSet, ι => Outl Θ ι.t.reach (.caps ι.C)
  | .perform _ _ _, _ => False
  | _, _ => True

end Flick
