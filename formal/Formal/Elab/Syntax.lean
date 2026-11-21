/-!
# Elaboration: classes, instances, dictionaries, entailment (elaboration.md §1.1, §2.1–§2.3)

A model of its own, like `Formal/RT/`: it imports nothing from the core (`Formal/Syntax.lean`). Constraint
types are first-order terms; a dictionary term is `d ::= δ | d.$slot | I d̄ | struct_C d̄ |
prim_Q ℓ̄`, with application as a binary spine. A structural dictionary carries its record type
and a primitive one its constraint (`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`): the run of the target picks its global by them.
-/

namespace Flick.Elab

mutual
/-- The stages: `P`, and `R` with its rows of services and of errors, each written as a type:
a row variable is a variable, the empty row `cnil`, a named element `rnm`, a union `run`
(`docs/formal/elaboration.md`). -/
inductive Stage where
  | P
  | R (r e : Ty)

/-- Types: variables, constructors, application, and the arrow `a →φ^q b` of `Fun` in the
checker (`docs/formal/elaboration.md#the-source-arrow-was-a-reserved-constructor`). Its qualifier `q` is a capture row written as a type: a variable, `cnil`
(`∅`) or a union `cun`. Capture variables are ordinary variables. A row of services or errors
is a type too: a variable, `cnil`, the row `{t}` of one service or error type `t` whose
constructor is `c` (`rnm c t`), or a union `ρ₁ + ρ₂` (`run`). `all k t` is the type `∀ᾱ. t` of a
parameter, with `k` variables `var 0 … var (k-1)`. -/
inductive Ty where
  | var (n : Nat)
  | con (c : Nat)
  | app (t u : Ty)
  | arr (φ : Stage) (a b q : Ty)
  | cnil
  | cun (q q' : Ty)
  | rnm (c : Nat) (t : Ty)
  | run (r r' : Ty)
  | all (k : Nat) (t : Ty)
end

deriving instance DecidableEq for Stage, Ty

/-- A property of the rows of a stage. -/
def Stage.All (m : Ty → Prop) : Stage → Prop
  | .P => True
  | .R r e => m r ∧ m e

/-- Induction on types: an arrow sees the rows of its stage as smaller types. -/
theorem Ty.ind {motive : Ty → Prop} (var : ∀ n, motive (.var n)) (con : ∀ c, motive (.con c))
    (app : ∀ t u, motive t → motive u → motive (.app t u))
    (arr : ∀ φ a b q, motive a → motive b → motive q → φ.All motive → motive (.arr φ a b q))
    (cnil : motive .cnil) (cun : ∀ q q', motive q → motive q' → motive (.cun q q'))
    (rnm : ∀ c t, motive t → motive (.rnm c t))
    (run : ∀ r r', motive r → motive r' → motive (.run r r'))
    (all : ∀ k t, motive t → motive (.all k t)) : ∀ t, motive t := by
  intro t
  induction t using Ty.rec (motive_1 := Stage.All motive) with
  | P => trivial
  | R r e ihr ihe => exact ⟨ihr, ihe⟩
  | var n => exact var n
  | con c => exact con c
  | app t u iht ihu => exact app t u iht ihu
  | arr φ a b q ihφ iha ihb ihq => exact arr φ a b q iha ihb ihq ihφ
  | cnil => exact cnil
  | cun q q' ihq ihq' => exact cun q q' ihq ihq'
  | rnm c t iht => exact rnm c t iht
  | run r r' ihr ihr' => exact run r r' ihr ihr'
  | all k t iht => exact all k t iht

/-- A renaming under `k` binders. -/
def liftN (k : Nat) (ξ : Nat → Nat) : Nat → Nat := fun n => if n < k then n else ξ (n - k) + k

mutual
def Ty.ren (ξ : Nat → Nat) : Ty → Ty
  | .var n => .var (ξ n)
  | .con c => .con c
  | .app t u => .app (t.ren ξ) (u.ren ξ)
  | .arr φ a b q => .arr (φ.ren ξ) (a.ren ξ) (b.ren ξ) (q.ren ξ)
  | .cnil => .cnil
  | .cun q q' => .cun (q.ren ξ) (q'.ren ξ)
  | .rnm c t => .rnm c (t.ren ξ)
  | .run r r' => .run (r.ren ξ) (r'.ren ξ)
  | .all k t => .all k (t.ren (liftN k ξ))

def Stage.ren (ξ : Nat → Nat) : Stage → Stage
  | .P => .P
  | .R r e => .R (r.ren ξ) (e.ren ξ)
end

/-- A substitution under `k` binders. -/
def liftTy (k : Nat) (σ : Nat → Ty) : Nat → Ty :=
  fun n => if n < k then .var n else (σ (n - k)).ren (· + k)

mutual
def Ty.subst (σ : Nat → Ty) : Ty → Ty
  | .var n => σ n
  | .con c => .con c
  | .app t u => .app (t.subst σ) (u.subst σ)
  | .arr φ a b q => .arr (φ.subst σ) (a.subst σ) (b.subst σ) (q.subst σ)
  | .cnil => .cnil
  | .cun q q' => .cun (q.subst σ) (q'.subst σ)
  | .rnm c t => .rnm c (t.subst σ)
  | .run r r' => .run (r.subst σ) (r'.subst σ)
  | .all k t => .all k (t.subst (liftTy k σ))

def Stage.subst (σ : Nat → Ty) : Stage → Stage
  | .P => .P
  | .R r e => .R (r.subst σ) (e.subst σ)
end

mutual
/-- `n` occurs in `t`, also in the rows of the stage of an arrow. -/
def Ty.Occurs (n : Nat) : Ty → Prop
  | .var m => m = n
  | .con _ => False
  | .app t u => t.Occurs n ∨ u.Occurs n
  | .arr φ a b q => a.Occurs n ∨ b.Occurs n ∨ q.Occurs n ∨ φ.Occurs n
  | .cnil => False
  | .cun q q' => q.Occurs n ∨ q'.Occurs n
  | .rnm _ t => t.Occurs n
  | .run r r' => r.Occurs n ∨ r'.Occurs n
  | .all k t => t.Occurs (n + k)

def Stage.Occurs (n : Nat) : Stage → Prop
  | .P => False
  | .R r e => r.Occurs n ∨ e.Occurs n
end

@[simp] theorem Stage.occurs_P (n : Nat) : Stage.P.Occurs n ↔ False := Iff.rfl

/-- `⌈t⌉`: `⌈var n⌉ = {n}`, `⌈con⌉ = ⌈cnil⌉ = ∅`, `⌈arr φ a b q⌉ = ⌈a⌉ ∪ ⌈b⌉ ∪ ⌈q⌉` with the
variables of the rows of `φ`, `⌈rnm c t⌉ = ⌈t⌉`, the rest by union. It is exactly the set of variables of `t`. -/
abbrev Ty.reach (t : Ty) (n : Nat) : Prop := t.Occurs n

/-- A ground type has no variables. -/
def Ty.Ground (t : Ty) : Prop := ∀ n, ¬ t.Occurs n

/-- The variables of `t` that a translated value reaches: those of `t` but the rows of the
stages of its arrows, which the qualifier of an arrow does not count. -/
def Ty.Shown : Ty → Nat → Prop
  | .var m, n => m = n
  | .con _, _ => False
  | .app t u, n => t.Shown n ∨ u.Shown n
  | .arr _ a b q, n => a.Shown n ∨ b.Shown n ∨ q.Shown n
  | .cnil, _ => False
  | .cun q q', n => q.Shown n ∨ q'.Shown n
  | .rnm _ t, n => t.Shown n
  | .run r r', n => r.Shown n ∨ r'.Shown n
  | .all k t, n => t.Shown (n + k)

/-- The row `r` keeps `n` live: a variable of the index of a named element, as `Scope s`
keeps `s`, or of a type written in the row, as `Live q` is the capture row `q` there. A row
variable keeps nothing. -/
def Ty.Keeps : Ty → Nat → Prop
  | .var _, _ => False
  | .rnm _ t, n => t.Shown n
  | .run r r', n => r.Keeps n ∨ r'.Keeps n
  | t, n => t.Shown n

/-- Parameter variance: covariant (`+`, producer-only), contravariant (`-`, consumer-only),
invariant (`inv`, mixed/higher-kinded), or bivariant/unused (`bi`). -/
inductive Variance where
  | pos | neg | inv | bi
  deriving DecidableEq

/-- A parameter of variance `s` may stand at polarity `q` (`true`: covariant). -/
def Variance.at (s : Variance) (q : Bool) : Prop :=
  (q = true ∧ s = .pos) ∨ (q = false ∧ s = .neg)

/-- Inclusion of service rows in the elaboration layer. -/
inductive RowLe : Ty → Ty → Prop
  | refl {r} : RowLe r r
  | cnil {r} : RowLe .cnil r
  | runL {r₁ r₂ r} : RowLe r₁ r → RowLe r₂ r → RowLe (.run r₁ r₂) r
  | runRL {r r₁ r₂} : RowLe r r₁ → RowLe r (.run r₁ r₂)
  | runRR {r r₁ r₂} : RowLe r r₂ → RowLe r (.run r₁ r₂)
  | keeps {q r} : (∀ n, q.Shown n → r.Keeps n) → RowLe (.cun q .cnil) r

/-- Use-stage comparison `(φ₁, q₁) ⊑ (φ₂, q₂)` for arrows `a →φ₁^q₁ b` and `a' →φ₂^q₂ b'`:
a pure arrow fits any stage; a runtime arrow widens its service row (including moving
qualifier atoms into a row that keeps them live) and keeps its error row. -/
def Stage.UseLe : Stage → Ty → Stage → Ty → Prop
  | .P, _, _, _ => True
  | .R _ _, _, .P, _ => False
  | .R r₁ e₁, q₁, .R r₂ e₂, q₂ =>
      RowLe r₁ r₂ ∧ e₁ = e₂ ∧ ∀ n, q₁.Shown n → q₂.Shown n ∨ r₂.Keeps n

/-- `T τ̄`: a type constructor applied to its arguments. -/
def Ty.tcon (T : Nat) (us : List Ty) : Ty := us.foldl .app (.con T)

/-- Subtyping on elaboration types, parameterized by data-constructor variances `dvar`:
- arrows compare contravariantly in the parameter, covariantly in the result, by
  `Stage.UseLe` on stages/qualifiers, and preserve shown variables (`Ty.Shown`);
- nominal data `T ū` (`T ≠ 2, 4`, not `Bool`/`Int`) compares arguments by `dvar T j`
  while preserving shown variables at every argument. -/
inductive Ty.Le (dvar : Nat → Nat → Variance) : Ty → Ty → Prop
  | refl {t} : Ty.Le dvar t t
  | arr {φ₁ φ₂ a₁ a₂ b₁ b₂ q₁ q₂} :
      Ty.Le dvar a₂ a₁ →
      Stage.UseLe φ₁ q₁ φ₂ q₂ →
      Ty.Le dvar b₁ b₂ →
      (∀ n, (Ty.arr φ₁ a₁ b₁ q₁).Shown n → (Ty.arr φ₂ a₂ b₂ q₂).Shown n) →
      Ty.Le dvar (.arr φ₁ a₁ b₁ q₁) (.arr φ₂ a₂ b₂ q₂)
  | data {T : Nat} {us us' : List Ty} :
      T ≠ 2 → T ≠ 4 →
      us.length = us'.length →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
        ∀ n, u.Shown n → u'.Shown n) →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
        dvar T j = .pos → Ty.Le dvar u u') →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
        dvar T j = .neg → Ty.Le dvar u' u) →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
        dvar T j = .inv → u = u') →
      Ty.Le dvar (Ty.tcon T us) (Ty.tcon T us')

/-- A constraint `C τ̄`. -/
structure Cn where
  cls : Nat
  args : List Ty
  deriving DecidableEq

def Cn.subst (σ : Nat → Ty) (q : Cn) : Cn := ⟨q.cls, q.args.map (·.subst σ)⟩

/-- A ground constraint. -/
def Cn.Ground (q : Cn) : Prop := ∀ t ∈ q.args, t.Ground

/-- The substitution of a class's parameters `ᾱ := τ̄` (parameters are `var 0 … var (k-1)`). -/
def params (ts : List Ty) : Nat → Ty := fun n => ts.getD n (.con 0)

/-- Dictionary terms of §2.2; `ap` applies an instance or `struct_C^τ` to a dictionary.
`strct C τ` is `struct_C` for the record `τ`, `prim p q` is `prim_q` with the labels `p`
(`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`). -/
inductive Dict where
  | var (k : Nat)
  | slot (d : Dict) (i : Nat)
  | inst (I : Nat)
  | strct (C : Nat) (τ : Ty)
  | prim (p : Nat) (q : Cn)
  | ap (d e : Dict)

/-- `I d̄`, `struct_C d̄`. -/
def apps (f : Dict) (ds : List Dict) : Dict := ds.foldl .ap f

/-- `ρ(d)`: replace the premise variables. -/
def Dict.subst (ρ : Nat → Dict) : Dict → Dict
  | .var k => ρ k
  | .slot d i => .slot (d.subst ρ) i
  | .inst I => .inst I
  | .strct C τ => .strct C τ
  | .prim p q => .prim p q
  | .ap d e => .ap (d.subst ρ) (e.subst ρ)

/-- The dictionaries `d̄` as the values of `δ₀, δ₁, …`. -/
def args (ds : List Dict) : Nat → Dict := fun k => ds.getD k (.var 0)

/-- A scheme `∀ᾱ. Q̄ ⇒ τ`: `k` binders `var 0 … var (k-1)`; the variables from `var k` on are
free. `q` is the qualifier of its dictionary arrows; as they are pure, a call of them needs
nothing live, and nothing reads it for methods and constants of Γ₀. `bd` are its value bounds
(regions.md §2.1): `(t, D)` asks that what `t` shows is within what `D` shows at every
instance; a bound `αᵢ^{≤D}` of the checker is `(var i, D)`, and `τ` writes `αᵢ` for it. -/
structure Scheme where
  k : Nat
  ctx : List Cn
  ty : Ty
  q : Ty
  bd : List (Ty × Ty)

/-- An instance `instance Q̄ ⇒ C τ̄` of §2.5: its head, its context (the premises `δ̄`) and the
dictionaries of its superclass slots, written over the context. -/
structure Inst where
  cls : Nat
  head : List Ty
  ctx : List Cn
  slots : List Dict

/-- A constructor `K : ∀ᾱ. Q̄ ⇒ τ̄ → T ρ̄` of a data type, `ᾱ` its `k` variables
`var 0 … var (k-1)`: the constraints whose dictionaries a value of `K` carries, the types of its
fields, its type constructor and the arguments `ρ̄` of the type it builds. An ordinary
constructor builds `T ᾱ`, one of a GADT any instance of `T`; a variable of `ᾱ` that is not in
`ρ̄` is existential. -/
structure Ctor where
  k : Nat
  ctx : List Cn
  fields : List Ty
  T : Nat
  res : List Ty

/-- A program: its classes (superclasses over the class parameters), its instances `𝓘ₚ`, and
the compiler's classes: structural (`Eq`, `Ord`, `Hash`, `Show` of records) and primitives of
`Record` rows, whose rules (Struct) and (Prim) are black boxes. `methods m` is the class of the
method `m` and its scheme `∀ᾱ β̄. (C ᾱ, Q̄) ⇒ τ` of §1.1: the class constraint comes first.
`consts c` is the scheme of a constant of Γ₀, `ctors K` a constructor of a data type.
`dataVar` and `clsVar` give the inferred variances of data and class parameters (defaulting to
`inv`). -/
structure Prog where
  supers : Nat → List Cn
  insts : Nat → Option Inst
  structC : Nat → Bool
  fields : Ty → Option (List Ty)
  primC : Nat → Bool
  prims : Cn → Option Nat
  methods : Nat → Option (Nat × Scheme)
  consts : Nat → Option Scheme
  ctors : Nat → Option Ctor
  dataVar : Nat → Nat → Variance := fun _ _ => .inv
  clsVar : Nat → Nat → Variance := fun _ _ => .inv

variable (P : Prog)

/-- `H` serves the wanted query `Q` in the direction of `P.clsVar` (B1):
- `+` (producer-only): `H_j ≤ Q_j`;
- `-` (consumer-only): `Q_j ≤ H_j` while keeping shown variables (`⌈H_j⌉ ⊆ ⌈Q_j⌉`);
- `inv` or `bi` (mixed/higher-kinded or unused marker): exact equality `Q_j = H_j`. -/
def Cn.Serves (P : Prog) (Q H : Cn) : Prop :=
  H.cls = Q.cls ∧
  Q.args.length = H.args.length ∧
  (∀ (j : Nat) (q h : Ty), Q.args[j]? = some q → H.args[j]? = some h →
    P.clsVar Q.cls j = .pos → Ty.Le P.dataVar h q) ∧
  (∀ (j : Nat) (q h : Ty), Q.args[j]? = some q → H.args[j]? = some h →
    P.clsVar Q.cls j = .neg → Ty.Le P.dataVar q h ∧ ∀ n, h.Shown n → q.Shown n) ∧
  (∀ (j : Nat) (q h : Ty), Q.args[j]? = some q → H.args[j]? = some h →
    (P.clsVar Q.cls j = .inv ∨ P.clsVar Q.cls j = .bi) → q = h)

/-- The `i`-th superclass of `Q = C τ̄`: `Sᵢ ῡᵢ[ᾱ := τ̄]`. -/
def Prog.super (Q : Cn) (i : Nat) : Option Cn :=
  (P.supers Q.cls)[i]?.map (·.subst (params Q.args))

/-- `Δ*` of §2.3: (Δ-Given) and (Δ-Super). `Δ` is a list; `δ` is an index into it. -/
inductive Clo (Δ : List Cn) : Dict → Cn → Prop
  | given {k Q} : Δ[k]? = some Q → Clo Δ (.var k) Q
  | super {d Q i S} : Clo Δ d Q → P.super Q i = some S → Clo Δ (.slot d i) S

/-- `Δ ⊩ Q̄ ⇝ d̄`, for a list of constraints at once: (Given), (Prim), (Struct), (Inst).
In (Inst), the instantiated head serves `Q` in the class's variance direction (`Cn.Serves`, B1).
(Improve) yields no dictionary; the constraints here are the improved ones. -/
inductive Ent (Δ : List Cn) : List Cn → List Dict → Prop
  | nil : Ent Δ [] []
  | cons {Q d qs ds} : Ent Δ [Q] [d] → Ent Δ qs ds → Ent Δ (Q :: qs) (d :: ds)
  | given {Q d} : Clo P Δ d Q → Ent Δ [Q] [d]
  | prim {Q p} : P.primC Q.cls = true → P.prims Q = some p → Ent Δ [Q] [.prim p Q]
  | strct {C τ ts ds} : P.structC C = true → P.fields τ = some ts →
      Ent Δ (ts.map fun t => ⟨C, [t]⟩) ds → Ent Δ [⟨C, [τ]⟩] [apps (.strct C τ) ds]
  | inst {Q I ins σ ds} : P.insts I = some ins →
      Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ)⟩ →
      Ent Δ (ins.ctx.map (·.subst σ)) ds →
      Ent Δ [Q] [apps (.inst I) ds]

/-- `ρ` of Lemma 6.1: types `θ` and closed dictionaries `ρ(δ)` with `· ⊩ Q'θ ⇝ ρ(δ)`. -/
def Closes (P : Prog) (Δ : List Cn) (θ : Nat → Ty) (ρ : Nat → Dict) : Prop :=
  ∀ k Q, Δ[k]? = some Q → Ent P [] [Q.subst θ] [ρ k]

/-- `≈` of §4.2: the least congruence with the slot rule of an instance, and of a structural
dictionary (`docs/formal/elaboration.md#equivalence-did-not-relate-structural-slots-to-superclass-dictionaries`), both for closed derivations of the arguments (`docs/formal/elaboration.md#slot-rules-of-equivalence-take-closed-derivations`); a structural or primitive dictionary of a type is `≈` the one of its
instance, as the run erases the type (`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`). -/
inductive Eqv : Dict → Dict → Prop
  | refl {d} : Eqv d d
  | symm {d e} : Eqv d e → Eqv e d
  | trans {d e f} : Eqv d e → Eqv e f → Eqv d f
  | slot {d e i} : Eqv d e → Eqv (.slot d i) (.slot e i)
  | ap {d d' e e'} : Eqv d d' → Eqv e e' → Eqv (.ap d e) (.ap d' e')
  /-- `(I d̄).$slotᵢ ≈ sᵢ[δ̄ := d̄]`, `d̄` closed derivations of the context (`docs/formal/elaboration.md#slot-rules-of-equivalence-take-closed-derivations`) -/
  | instSlot {I ins i s σ ds} : P.insts I = some ins → ins.slots[i]? = some s →
      Ent P [] (ins.ctx.map (·.subst σ)) ds →
      Eqv (.slot (apps (.inst I) ds) i) (s.subst (args ds))
  /-- `(struct_C d̄).$slotᵢ ≈ struct_S (d̄.$slotᵢ)` (`docs/formal/elaboration.md#equivalence-did-not-relate-structural-slots-to-superclass-dictionaries`), `d̄` closed derivations (`docs/formal/elaboration.md#slot-rules-of-equivalence-take-closed-derivations`) -/
  | strctSlot {C τ ts i S ds} : P.structC C = true → (P.supers C)[i]? = some S →
      P.fields τ = some ts → Ent P [] (ts.map fun t => ⟨C, [t]⟩) ds →
      Eqv (.slot (apps (.strct C τ) ds) i) (apps (.strct S.cls τ) (ds.map (.slot · i)))
  /-- `struct_C^τ ≈ struct_C^{τθ}` (`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`) -/
  | strctSub {C τ ts θ} : P.structC C = true → P.fields τ = some ts →
      Eqv (.strct C τ) (.strct C (τ.subst θ))
  /-- `struct_C^{τ₁} ≈ struct_C^{τ₂}` when both records serve a common constraint `C τ`: the
  run erases the record type. -/
  | strctLe {C τ₁ τ₂ ts₁ ts₂} : P.structC C = true → P.fields τ₁ = some ts₁ →
      P.fields τ₂ = some ts₂ →
      (∃ τ, Cn.Serves P ⟨C, [τ]⟩ ⟨C, [τ₁]⟩ ∧ Cn.Serves P ⟨C, [τ]⟩ ⟨C, [τ₂]⟩) →
      Eqv (.strct C τ₁) (.strct C τ₂)
  /-- `prim_q ℓ̄ ≈ prim_{qθ} ℓ̄` (`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`) -/
  | primSub {p q θ} : P.primC q.cls = true → P.prims q = some p →
      Eqv (.prim p q) (.prim p (q.subst θ))

end Flick.Elab
