import Formal.Rules
import Formal.Elab.T6a
import Formal.Elab.Sugar

/-!
# λFlick⁺ as a `Target`: the types

The types of `Elab` in the core of `Formal/` with regions and dictionary records:
`var ↦ tvar`, `app ↦ tapp`, `con c ↦ adt`, `Bool ↦ base bool`, `Int ↦ base int`,
`arr φ a b q ↦ ⟦a⟧ →⟦φ⟧^⌈⟦q⟧⌉ ⟦b⟧`,
`cnil ↦ caps ∅`, `cun ↦ caps (∪)`, `rnm c t ↦ row ({c} ∪ ⌈⟦t⟧⌉)` (`Row.err`),
`run r r' ↦ row (⟦r⟧ ∪ ⟦r'⟧)` with each side what it stands for as a row,
`D_{C τ̄} ↦ dict (cls C |τ̄|) τ̄`. A stage `R r e` is
`rt ⟦r⟧ (⟦e⟧ ∪ ⌈⟦e⟧⌉ ∪ ε)`, a row `⟦r⟧` what the type `⟦r⟧` stands for as a row (`Ty.asRow`: a
variable is a row variable), `⌈⟦e⟧⌉` what its errors reach (`errRow`), `ε` every name
(`allNames`, `docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`). The qualifier of a translated
arrow is what a call of it needs live, so it does not count the variables of its rows.
The translation commutes with substitution (`trT_subst`, `trTT_subst`, `trS_subst`), and
`⌈⟦t⟧⌉` is what `t` shows (`trT_reach`, `Ty.Shown`): a capture variable is an ordinary
variable on both sides.

`Names` chooses the names of the core that realise the program. The class name carries
the number of parameters, so that projecting a superclass slot of `D_{C τ̄}` is typed for any
list `τ̄`, as the law `slot` asks. A structural dictionary is a family over the record type
(`struct_C : D_{C τ₁} →P … →P D_{C τ}` for each `τ`), and so is a primitive one over its
constraint; the model's `Dict.strct C` and `Dict.prim p` do not show them, so their globals
are named by the type too (`docs/formal/elaboration.md#structural-and-primitive-dictionaries-are-families-indexed-by-type`).
-/

namespace Flick.Elab

/-- The names of the core that realise a program: type constructors, dictionary classes
(by class and number of parameters), the labels of methods and slots, the globals of
instances, constants of Γ₀, structural dictionaries (by class and record type) and
primitive ones (by primitive and constraint), the constructor `Some` and the constructors of
the program's data types. -/
structure Names where
  con : Nat → Name
  cls : Nat → Nat → Nat
  meth : Nat → Label
  slot : Nat → Label
  inst : Nat → Nat
  const : Nat → Nat
  strct : Nat → Ty → Nat
  prim : Nat → Cn → Nat
  some : Name
  ctor : Nat → Name

variable (N : Names)

/-- Every name, as a row of errors: `⟦R⟧` lets a body raise any error type whose values
reach what the row reaches, such as `Errors e`, data of the type `Errors` (`docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`). -/
def allNames : Row
  | .nm _ => True
  | _ => False

@[simp] theorem allNames_substT (σ : Atom → Flick.Ty) : allNames.substT σ = allNames := by
  funext x; cases x <;> simp [Row.substT, allNames]

/-- A row of errors of the source in the core: what it stands for as a row and the atoms
it reaches, which bound what its errors reach; so a raised `Errors e` of a row variable `e`,
which reaches `e`, is in the row. -/
def errRow (t : Flick.Ty) : Row := Row.union t.asRow (Row.ofCaps t.reach)

theorem errRow_substT (t : Flick.Ty) (σ : Atom → Flick.Ty) :
    (errRow t).substT σ = errRow (t.substT σ) := by
  simp [errRow, Ty.asRow_substT, Row.ofCaps_substT, Ty.reach_caps]

/-- `⟦τ⟧`. -/
def trT : Ty → Flick.Ty
  | .var n => .tvar n
  | .con c => if c = 2 then .base .bool else if c = 4 then .base .int else .adt (N.con c)
  | .app t u => .tapp (trT t) (trT u)
  | .arr .P a b q => .arrow (trT a) .pure (trT b) (trT q).reach
  | .arr (.R r e) a b q => .arrow (trT a) (.rt (trT r).asRow (Row.union (errRow (trT e)) allNames))
      (trT b) (trT q).reach
  | .cnil => .caps Caps.none
  | .cun q q' => .caps fun a => (trT q).reach a ∨ (trT q').reach a
  | .rnm c t => .row (Row.err (N.con c) (trT t))
  | .run r r' => .row (Row.union (trT r).asRow (trT r').asRow)
  | .all k t => .all k (trT t)

/-- `⟦P⟧ = pure`, `⟦R r e⟧ = rt ⟦r⟧ (⟦e⟧ ∪ ⌈⟦e⟧⌉ ∪ allNames)`. -/
def trStage : Stage → Flick.Stage
  | .P => .pure
  | .R r e => .rt (trT N r).asRow (Row.union (errRow (trT N e)) allNames)

theorem trT_arr (φ : Stage) (a b q : Ty) :
    trT N (.arr φ a b q) = .arrow (trT N a) (trStage N φ) (trT N b) (trT N q).reach := by
  cases φ <;> rfl

/-- `⟦D_Q⟧`. -/
def trD (q : Cn) : Flick.Ty := .dict (N.cls q.cls q.args.length) (Flick.Ty.spine (q.args.map (trT N)))

/-- `⟦t⟧` of a type of an elaborated term. -/
def trTT : TTy → Flick.Ty
  | .mono t => trT N t
  | .arr φ a b q => .arrow (trTT a) (trStage N φ) (trTT b) (trT N q).reach
  | .dict q => trD N q

theorem trTT_arr (φ : Stage) (a b : TTy) (q : Ty) :
    trTT N (.arr φ a b q) = .arrow (trTT N a) (trStage N φ) (trTT N b) (trT N q).reach := rfl

/-- A value bound `(t, D)` as a fact of the core: `Outlives ⌈⟦t⟧⌉ ⌈⟦D⟧⌉`, so `⌈⟦t⟧⌉ ⊆ ⌈⟦D⟧⌉`;
for `t = var i` it is `Facts.bnd i ⌈⟦D⟧⌉` (`trB_var`). -/
def trB (b : Ty × Ty) : Facts := fun C r => C = (trT N b.1).reach ∧ r = .caps (trT N b.2).reach

/-- The value bounds of a scheme as its facts. -/
def trBd : List (Ty × Ty) → Facts
  | [] => Facts.none
  | b :: bd => Facts.union (trB N b) (trBd bd)

/-- The facts of the value bounds of a scheme with `k` variables. A scheme without variables
has no instance to check them at: the source checks them at each use (`Elab.var`), and the
core binds it by (Let-Mono), without facts. -/
def trFacts (k : Nat) (bd : List (Ty × Ty)) : Facts := if k = 0 then Facts.none else trBd N bd

/-- `⟦∀ᾱ. t⟧`, with its value bounds as facts. -/
def trS (s : TSch) : Flick.Scheme := ⟨s.k, trFacts N s.k s.bd, trTT N s.ty⟩

/-- `⟦σ⟧` of a substitution; regions stay. -/
def trσ (σ : Nat → Ty) : Atom → Flick.Ty
  | .v n => trT N (σ n)
  | .r p => .rg p

/-- `t` mentions no variable from `k` on. -/
def TTy.Closed (k : Nat) (t : TTy) : Prop :=
  ∀ σ σ' : Nat → Ty, (∀ j, j < k → σ j = σ' j) → t.subst σ = t.subst σ'

@[simp] theorem trStage_P : trStage N .P = .pure := rfl

theorem trT_bool : trT N boolTy = .base .bool := by simp [trT, boolTy]

theorem trT_int : trT N intTy = .base .int := by simp [trT, intTy]

theorem trT_opt (a : Ty) : trT N (optTy a) = .tapp (.adt (N.con 3)) (trT N a) := by
  simp [trT, optTy]

theorem ex_or {P Q : Nat → Prop} {a : Atom} :
    ((∃ n, a = .v n ∧ P n) ∨ ∃ n, a = .v n ∧ Q n) ↔ ∃ n, a = .v n ∧ (P n ∨ Q n) := by
  constructor
  · rintro (⟨n, rfl, h⟩ | ⟨n, rfl, h⟩)
    · exact ⟨n, rfl, .inl h⟩
    · exact ⟨n, rfl, .inr h⟩
  · rintro ⟨n, rfl, h | h⟩
    · exact .inl ⟨n, rfl, h⟩
    · exact .inr ⟨n, rfl, h⟩

/-- `⌈⟦τ⟧⌉` is what `τ` shows: only variables. -/
theorem trT_reach (t : Ty) (a : Atom) : (trT N t).reach a ↔ ∃ n, a = .v n ∧ t.Shown n := by
  induction t using Ty.ind generalizing a with
  | var m => simp [trT, Ty.Shown]
  | con c =>
    simp only [trT, Ty.Shown, and_false, exists_false, iff_false]
    split <;> (try split) <;> simp [Flick.Ty.reach, Caps.none]
  | app t u iht ihu => simp only [trT, Flick.Ty.reach, iht, ihu, Ty.Shown, ex_or]
  | arr φ a b q iha ihb ihq _ =>
    simp only [trT_arr, Flick.Ty.reach, iha, ihb, ihq, Ty.Shown, ex_or]
  | cnil => simp [trT, Flick.Ty.reach, Caps.none, Ty.Shown]
  | cun q q' ihq ihq' => simp only [trT, Flick.Ty.reach, ihq, ihq', Ty.Shown, ex_or]
  | rnm c t iht => simp [trT, Flick.Ty.reach, Row.err, Row.union, Row.single, Row.ofCaps, iht, Ty.Shown]
  | all k t iht =>
    simp only [trT, Flick.Ty.reach, iht, Ty.Shown]
    cases a <;> simp [Atom.up]
  | run r r' ihr ihr' =>
    simp only [Ty.Shown, ← ex_or, ← ihr, ← ihr']
    rw [Ty.reach_asRow (trT N r), Ty.reach_asRow (trT N r')]
    simp only [trT, Flick.Ty.reach, Row.union]
    grind

theorem trT_reach_v {t : Ty} {n : Nat} (h : t.Shown n) : (trT N t).reach (.v n) :=
  (trT_reach N t _).2 ⟨n, rfl, h⟩

/-- What `⟦t⟧` reaches are variables of `t`. -/
theorem trT_reach_sub {t : Ty} {a : Atom} (h : (trT N t).reach a) : ∃ n, a = .v n ∧ t.Occurs n :=
  let ⟨n, e, hn⟩ := (trT_reach N t a).1 h; ⟨n, e, hn.occurs⟩

/-- A row that keeps `n` live has its atom: `Scope s`, or a capture row written in it. -/
theorem trT_keeps {r : Ty} {n : Nat} (h : r.Keeps n) : (trT N r).asRow (.atm (.v n)) := by
  induction r using Ty.ind with
  | var m => exact h.elim
  | rnm c t _ => exact .inr (trT_reach_v N h)
  | run r r' ihr ihr' =>
    rcases h with h | h
    · exact .inl (ihr h)
    · exact .inr (ihr' h)
  | con c =>
    simp only [Ty.Keeps, Ty.Shown] at h
  | app t u _ _ => exact trT_reach_v N h
  | arr φ a b q _ _ _ _ =>
    have := trT_reach_v N h
    rw [trT_arr] at this ⊢
    exact this
  | cnil => exact h.elim
  | cun q q' _ _ => exact trT_reach_v N h
  | all k t _ => exact trT_reach_v N h

theorem trD_reach (q : Cn) (a : Atom) : ¬ (trD N q).reach a := by
  simp [trD, Flick.Ty.reach, Caps.none]

theorem trTT_reach (t : TTy) (a : Atom) : (trTT N t).reach a ↔ ∃ n, a = .v n ∧ t.reach n := by
  induction t generalizing a with
  | mono t => exact trT_reach N t a
  | arr φ a b q iha ihb =>
    simp only [trTT_arr, Flick.Ty.reach, iha, ihb, trT_reach, TTy.reach, ex_or]
  | dict q => simp [trTT, trD_reach, TTy.reach]

theorem Caps.substT_or (C C' : Caps) (σ : Atom → Flick.Ty) :
    Caps.substT σ (fun a => C a ∨ C' a) = fun a => C.substT σ a ∨ C'.substT σ a := by
  funext a; apply propext; simp only [Caps.substT]
  constructor
  · rintro ⟨b, h | h, hb⟩
    · exact .inl ⟨b, h, hb⟩
    · exact .inr ⟨b, h, hb⟩
  · rintro (⟨b, h, hb⟩ | ⟨b, h, hb⟩)
    · exact ⟨b, .inl h, hb⟩
    · exact ⟨b, .inr h, hb⟩

/-- The translation commutes with a substitution whose types shift as they should. -/
theorem trT_subst_of (t : Ty) (σ : Nat → Ty)
    (hσ : ∀ k n, trT N ((σ n).ren (· + k)) = (trT N (σ n)).substT (shiftV k)) :
    trT N (t.subst σ) = (trT N t).substT (trσ N σ) := by
  induction t using Ty.ind generalizing σ with
  | all k t iht =>
    have hl : trσ N (liftTy k σ) = liftT k (trσ N σ) := by
      funext a
      cases a with
      | v n =>
        simp only [trσ, liftTy, liftT]
        split
        · rfl
        · rw [hσ, Flick.Ty.up_def]
      | r p => simp [trσ, liftT, Flick.Ty.up_def, Flick.Ty.substT, shiftV]
    simp only [Ty.subst, trT, Flick.Ty.substT]
    rw [iht _ fun j n => ?_, hl]
    simp only [liftTy]
    split
    · simp [Ty.ren, trT, Flick.Ty.substT, shiftV]
    · rw [Ty.ren_ren, show (fun x => x + k + j) = (· + (k + j)) by funext x; omega, hσ, hσ,
        Flick.Ty.substT_comp]
      congr 1; funext a
      cases a with
      | v i => simp [shiftV, Flick.Ty.substT]; omega
      | r p => rfl
  | var n => rfl
  | con c => simp only [Ty.subst, trT]; split <;> (try split) <;> rfl
  | app t u iht ihu => simp [Ty.subst, trT, Flick.Ty.substT, iht σ hσ, ihu σ hσ]
  | arr φ a b q iha ihb ihq ihφ =>
    have hφ : trStage N (φ.subst σ) = (trStage N φ).substT (trσ N σ) := by
      cases φ with
      | P => rfl
      | R r e =>
        simp [Stage.subst, trStage, Flick.Stage.substT, ihφ.1 σ hσ, ihφ.2 σ hσ, Ty.asRow_substT, errRow_substT]
    have hq : (trT N (q.subst σ)).reach = Caps.substT (trσ N σ) (trT N q).reach := by
      rw [ihq σ hσ, ← Ty.reach_caps]
    simp only [Ty.subst, trT_arr, Flick.Ty.substT, iha σ hσ, ihb σ hσ, hφ, hq]
  | cnil => simp [Ty.subst, trT, Flick.Ty.substT]
  | cun q q' ihq ihq' =>
    simp only [Ty.subst, trT, Flick.Ty.substT, ihq σ hσ, ihq' σ hσ, Caps.substT_or, Ty.reach_caps]
  | rnm c t iht =>
    simp only [Ty.subst, trT, Flick.Ty.substT, iht σ hσ, Row.err, Row.union_substT, Row.single_substT,
      Row.ofCaps_substT, Ty.reach_caps]
  | run r r' ihr ihr' =>
    simp only [Ty.subst, trT, Flick.Ty.substT, ihr σ hσ, ihr' σ hσ, Row.union_substT, Ty.asRow_substT]

theorem trT_ren (t : Ty) (k : Nat) : trT N (t.ren (· + k)) = (trT N t).substT (shiftV k) := by
  rw [Ty.ren_eq, trT_subst_of N t _ fun _ _ => rfl]
  congr 1

/-- **The translation commutes with substitution.** -/
theorem trT_subst (t : Ty) (σ : Nat → Ty) : trT N (t.subst σ) = (trT N t).substT (trσ N σ) :=
  trT_subst_of N t σ fun k n => trT_ren N (σ n) k

theorem trStage_subst (φ : Stage) (σ : Nat → Ty) :
    trStage N (φ.subst σ) = (trStage N φ).substT (trσ N σ) := by
  cases φ with
  | P => rfl
  | R r e =>
    simp [Stage.subst, trStage, Flick.Stage.substT, trT_subst, Ty.asRow_substT, errRow_substT]

theorem trD_subst (q : Cn) (σ : Nat → Ty) : trD N (q.subst σ) = (trD N q).substT (trσ N σ) := by
  simp only [trD, Cn.subst, Flick.Ty.substT, Flick.Ty.spine_substT, List.map_map, List.length_map]
  congr 2
  exact List.map_congr_left fun t _ => trT_subst N t σ

theorem trTT_subst (t : TTy) (σ : Nat → Ty) :
    trTT N (t.subst σ) = (trTT N t).substT (trσ N σ) := by
  induction t with
  | mono t => exact trT_subst N t σ
  | arr φ a b q iha ihb =>
    have hq : (trT N (q.subst σ)).reach = Caps.substT (trσ N σ) (trT N q).reach := by
      rw [trT_subst, ← Ty.reach_caps]
    simp only [TTy.subst, trTT_arr, Flick.Ty.substT, iha, ihb, trStage_subst, hq]
  | dict q => exact trD_subst N q σ

theorem trσ_lift (k : Nat) (σ : Nat → Ty) (a : Atom) : trσ N (lift k σ) a = liftT k (trσ N σ) a := by
  cases a with
  | v n =>
    simp only [trσ, lift, liftT]
    split
    · rfl
    · rw [trT_subst, Flick.Ty.up_def]; apply Flick.Ty.substT_congr; intro b; cases b <;> rfl
  | r p => rfl

theorem trσ_up (k : Nat) (a : Atom) : trσ N (up k) a = shiftV k a := by
  cases a <;> rfl

theorem trσ_inst (ts : List Ty) (a : Atom) : trσ N (inst ts) a = instT (ts.map (trT N)) a := by
  cases a with
  | v n => simp only [trσ, inst, instT, List.length_map]; split <;> simp [trT]
  | r p => rfl

theorem trB_var (i : Nat) (D : Ty) : trB N (.var i, D) = Facts.bnd i (trT N D).reach := rfl

/-- A value bound as a fact under any substitution of the core. -/
theorem trB_substT_any (b : Ty × Ty) (ρ : Atom → Flick.Ty) :
    (trB N b).substT ρ =
      fun C r => C = ((trT N b.1).substT ρ).reach ∧ r = .caps ((trT N b.2).substT ρ).reach := by
  funext C r; apply propext
  simp only [trB, Facts.substT, ← Ty.reach_caps]
  constructor
  · rintro ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩; exact ⟨rfl, rfl⟩
  · rintro ⟨rfl, rfl⟩; exact ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩

theorem trB_substT (b : Ty × Ty) (σ : Nat → Ty) :
    (trB N b).substT (trσ N σ) = trB N (Bound.subst σ b) := by
  rw [trB_substT_any]; unfold trB; simp only [Bound.subst, trT_subst]

theorem trBd_substT (bd : List (Ty × Ty)) (σ : Nat → Ty) :
    (trBd N bd).substT (trσ N σ) = trBd N (bd.map (Bound.subst σ)) := by
  induction bd with
  | nil => exact Facts.none_substT _
  | cons b bd ih => simp only [trBd, List.map_cons, Facts.union_substT, trB_substT, ih]

/-- The facts of value bounds hold under the translation of a substitution that makes the
bounds hold: `⌈⟦t⟧⌉ ⊆ ⌈⟦D⟧⌉` is what `t` shows within what `D` shows (`trT_reach`). -/
theorem trBd_holds {bd : List (Ty × Ty)} {σ : Nat → Ty} {ρ : Atom → Flick.Ty}
    (hρ : ∀ a, ρ a = trσ N σ a) (h : Bounds.Hold (bd.map (Bound.subst σ))) (Θ : Facts) :
    ∀ C r, trBd N bd C r → Outl Θ (C.substT ρ) (r.substT ρ) := by
  rw [funext hρ]
  induction bd with
  | nil => intro _ _ h; exact h.elim
  | cons b bd ih =>
    rintro C r (⟨rfl, rfl⟩ | hr)
    · refine Outl.caps_of_sub fun a ha => ?_
      rw [Ty.reach_caps, ← trT_subst] at ha ⊢
      obtain ⟨n, rfl, hn⟩ := (trT_reach N _ a).1 ha
      exact trT_reach_v N (h _ (.head _) n hn)
    · exact ih (fun b hb => h b (.tail _ hb)) C r hr

theorem trFacts_holds {k : Nat} {bd : List (Ty × Ty)} {σ : Nat → Ty} {ρ : Atom → Flick.Ty}
    (hρ : ∀ a, ρ a = trσ N σ a) (h : Bounds.Hold (bd.map (Bound.subst σ))) (Θ : Facts) :
    ∀ C r, trFacts N k bd C r → Outl Θ (C.substT ρ) (r.substT ρ) := by
  unfold trFacts; split
  · intro _ _ h; exact h.elim
  · exact trBd_holds N hρ h Θ

theorem trS_subst (s : TSch) (σ : Nat → Ty) : trS N (s.subst σ) = (trS N s).substT (trσ N σ) := by
  simp only [trS, TSch.subst, Flick.Scheme.substT, trTT_subst, Flick.Scheme.mk.injEq, true_and]
  rw [funext (trσ_lift N s.k σ)]
  refine ⟨?_, rfl⟩
  rw [← funext (trσ_lift N s.k σ)]
  unfold trFacts; split
  · exact (Facts.none_substT _).symm
  · exact (trBd_substT N _ _).symm

theorem trS_up (s : TSch) (k : Nat) : trS N (s.subst (up k)) = (trS N s).up k := by
  rw [trS_subst, Flick.Scheme.up, funext (trσ_up N k)]

theorem trD_up (q : Cn) (k : Nat) :
    Flick.Scheme.mono (trD N (q.subst (up k))) = (Flick.Scheme.mono (trD N q)).up k := by
  simp only [Flick.Scheme.up, Flick.Scheme.substT, Flick.Scheme.mono, Facts.none_substT,
    trD_subst, liftT_zero]
  rw [funext (trσ_up N k)]

/-- A closed type is an instance of its translation: `⟦t⟧[⟦σ 0⟧ … ⟦σ (k-1)⟧] = ⟦t σ⟧`. -/
theorem trTT_inst {k : Nat} {t : TTy} (h : t.Closed k) (σ : Nat → Ty) :
    (trTT N t).substT (instT ((List.range k).map fun j => trT N (σ j))) = trTT N (t.subst σ) := by
  have e : (List.range k).map (fun j => trT N (σ j)) = ((List.range k).map σ).map (trT N) := by
    simp [List.map_map, Function.comp_def]
  rw [e, ← funext (trσ_inst N _), ← trTT_subst]
  congr 1
  apply h
  intro j hj
  simp [inst, hj]

end Flick.Elab
