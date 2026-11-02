import Formal.Rules
import Formal.Elab.T6a
import Formal.Elab.Sugar

/-!
# λFlick⁺ as a `Target`: the types

The types of `Elab` in the core of `Formal/` with regions and dictionary records:
`var ↦ tvar`, `app ↦ tapp`, `con c ↦ adt`, `Bool ↦ base bool`, `Int ↦ base int`,
`arr φ a b q ↦ ⟦a⟧ →⟦φ⟧^⌈⟦q⟧⌉ ⟦b⟧`,
`cnil ↦ caps ∅`, `cun ↦ caps (∪)`, `D_{C τ̄} ↦ dict (cls C |τ̄|) τ̄`. A stage `R` is `rt ∅ ε` with
`ε` every name (`allNames`, Р210).
The translation commutes with substitution (`trT_subst`, `trTT_subst`, `trS_subst`) and with
`⌈·⌉` (`trT_reach`): a capture variable is an ordinary variable on both sides.

`Names` chooses the names of the core that realise the program. The class name carries
the number of parameters, so that projecting a superclass slot of `D_{C τ̄}` is typed for any
list `τ̄`, as the law `slot` asks. A structural dictionary is a family over the record type
(`struct_C : D_{C τ₁} →P … →P D_{C τ}` for each `τ`), and so is a primitive one over its
constraint; the model's `Dict.strct C` and `Dict.prim p` do not show them, so their globals
are named by the type too (Р195).
-/

namespace Flick.Elab

/-- The names of the core that realise a program: type constructors, dictionary classes
(by class and number of parameters), the labels of methods and slots, the globals of
instances, constants of Γ₀, structural dictionaries (by class and record type) and
primitive ones (by primitive and constraint), and the constructor `Some`. -/
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

variable (N : Names)

/-- Every name, as a row of errors: the stage `R` of the source has no row of errors, and
`⟦R⟧` lets a body raise any error type whose values reach nothing (Р210). -/
def allNames : Row
  | .nm _ => True
  | _ => False

@[simp] theorem allNames_substT (σ : Atom → Flick.Ty) : allNames.substT σ = allNames := by
  funext x; cases x <;> simp [Row.substT, allNames]

/-- `⟦P⟧ = pure`, `⟦R⟧ = rt ∅ allNames`. -/
def trStage : Stage → Flick.Stage
  | .P => .pure
  | .R => .rt Row.none allNames

/-- `⟦τ⟧`. -/
def trT : Ty → Flick.Ty
  | .var n => .tvar n
  | .con c => if c = 2 then .base .bool else if c = 4 then .base .int else .adt (N.con c)
  | .app t u => .tapp (trT t) (trT u)
  | .arr φ a b q => .arrow (trT a) (trStage φ) (trT b) (trT q).reach
  | .cnil => .caps Caps.none
  | .cun q q' => .caps fun a => (trT q).reach a ∨ (trT q').reach a

/-- `⟦D_Q⟧`. -/
def trD (q : Cn) : Flick.Ty := .dict (N.cls q.cls q.args.length) (Flick.Ty.spine (q.args.map (trT N)))

/-- `⟦t⟧` of a type of an elaborated term. -/
def trTT : TTy → Flick.Ty
  | .mono t => trT N t
  | .arr φ a b q => .arrow (trTT a) (trStage φ) (trTT b) (trT N q).reach
  | .dict q => trD N q

/-- `⟦∀ᾱ. t⟧`, without facts. -/
def trS (s : TSch) : Flick.Scheme := ⟨s.k, Facts.none, trTT N s.ty⟩

/-- `⟦σ⟧` of a substitution; regions stay. -/
def trσ (σ : Nat → Ty) : Atom → Flick.Ty
  | .v n => trT N (σ n)
  | .r p => .rg p

/-- `t` mentions no variable from `k` on. -/
def TTy.Closed (k : Nat) (t : TTy) : Prop :=
  ∀ σ σ' : Nat → Ty, (∀ j, j < k → σ j = σ' j) → t.subst σ = t.subst σ'

@[simp] theorem trStage_substT (φ : Stage) (σ : Atom → Flick.Ty) :
    (trStage φ).substT σ = trStage φ := by
  cases φ <;> simp [trStage, Flick.Stage.substT]

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

/-- `⌈⟦τ⟧⌉ = ⌈τ⌉`: only variables. -/
theorem trT_reach (t : Ty) (a : Atom) : (trT N t).reach a ↔ ∃ n, a = .v n ∧ t.Occurs n := by
  induction t generalizing a with
  | var m => simp [trT, Ty.Occurs]
  | con c =>
    simp only [trT, Ty.Occurs, and_false, exists_false, iff_false]
    split <;> (try split) <;> simp [Flick.Ty.reach, Caps.none]
  | app t u iht ihu => simp only [trT, Flick.Ty.reach, iht, ihu, Ty.Occurs, ex_or]
  | arr φ a b q iha ihb ihq => simp only [trT, Flick.Ty.reach, iha, ihb, ihq, Ty.Occurs, ex_or]
  | cnil => simp [trT, Flick.Ty.reach, Caps.none, Ty.Occurs]
  | cun q q' ihq ihq' => simp only [trT, Flick.Ty.reach, ihq, ihq', Ty.Occurs, ex_or]

theorem trT_reach_v {t : Ty} {n : Nat} (h : t.Occurs n) : (trT N t).reach (.v n) :=
  (trT_reach N t _).2 ⟨n, rfl, h⟩

theorem trD_reach (q : Cn) (a : Atom) : ¬ (trD N q).reach a := by
  simp [trD, Flick.Ty.reach, Caps.none]

theorem trTT_reach (t : TTy) (a : Atom) : (trTT N t).reach a ↔ ∃ n, a = .v n ∧ t.reach n := by
  induction t generalizing a with
  | mono t => exact trT_reach N t a
  | arr φ a b q iha ihb => simp only [trTT, Flick.Ty.reach, iha, ihb, trT_reach, TTy.reach, ex_or]
  | dict q => simp [trTT, trD_reach, TTy.reach]

theorem trTT_reach_dicts {q : Ty} {qs : List Cn} {t : TTy} {a : Atom} (h : (trTT N t).reach a) :
    (trTT N (dicts q qs t)).reach a := by
  induction qs with
  | nil => exact h
  | cons c qs ih => exact .inr (.inl ih)

theorem trS_reach (s : TSch) (a : Atom) : (trS N s).reach a ↔ ∃ n, a = .v n ∧ s.reach n := by
  simp only [Flick.Scheme.reach, trS, trTT_reach, TSch.reach]
  constructor
  · rintro ⟨b, ⟨n, rfl, h⟩, hd⟩
    simp only [Atom.down] at hd
    split at hd
    · cases hd
    · cases hd; exact ⟨n - s.k, rfl, by rwa [Nat.sub_add_cancel (by omega)]⟩
  · rintro ⟨n, rfl, h⟩
    exact ⟨.v (n + s.k), ⟨_, rfl, h⟩, by simp [Atom.down]⟩

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

/-- **The translation commutes with substitution.** -/
theorem trT_subst (t : Ty) (σ : Nat → Ty) : trT N (t.subst σ) = (trT N t).substT (trσ N σ) := by
  induction t with
  | var n => rfl
  | con c => simp only [Ty.subst, trT]; split <;> (try split) <;> rfl
  | app t u iht ihu => simp [Ty.subst, trT, Flick.Ty.substT, iht, ihu]
  | arr φ a b q iha ihb ihq =>
    simp [Ty.subst, trT, Flick.Ty.substT, iha, ihb, ihq, Ty.reach_caps]
  | cnil => simp [Ty.subst, trT, Flick.Ty.substT]
  | cun q q' ihq ihq' =>
    simp only [Ty.subst, trT, Flick.Ty.substT, ihq, ihq', Caps.substT_or, Ty.reach_caps]

theorem trD_subst (q : Cn) (σ : Nat → Ty) : trD N (q.subst σ) = (trD N q).substT (trσ N σ) := by
  simp only [trD, Cn.subst, Flick.Ty.substT, Flick.Ty.spine_substT, List.map_map, List.length_map]
  congr 2
  exact List.map_congr_left fun t _ => trT_subst N t σ

theorem trTT_subst (t : TTy) (σ : Nat → Ty) :
    trTT N (t.subst σ) = (trTT N t).substT (trσ N σ) := by
  induction t with
  | mono t => exact trT_subst N t σ
  | arr φ a b q iha ihb =>
    simp [TTy.subst, trTT, Flick.Ty.substT, iha, ihb, trT_subst, Ty.reach_caps]
  | dict q => exact trD_subst N q σ

theorem trσ_lift (k : Nat) (σ : Nat → Ty) (a : Atom) : trσ N (lift k σ) a = liftT k (trσ N σ) a := by
  cases a with
  | v n =>
    simp only [trσ, lift, liftT]
    split
    · rfl
    · rw [trT_subst]; apply Flick.Ty.substT_congr; intro b; cases b <;> rfl
  | r p => rfl

theorem trσ_up (k : Nat) (a : Atom) : trσ N (up k) a = shiftV k a := by
  cases a <;> rfl

theorem trσ_inst (ts : List Ty) (a : Atom) : trσ N (inst ts) a = instT (ts.map (trT N)) a := by
  cases a with
  | v n => simp only [trσ, inst, instT, List.length_map]; split <;> simp [trT]
  | r p => rfl

theorem trS_subst (s : TSch) (σ : Nat → Ty) : trS N (s.subst σ) = (trS N s).substT (trσ N σ) := by
  simp only [trS, TSch.subst, Flick.Scheme.substT, Facts.none_substT, trTT_subst]
  rw [funext (trσ_lift N s.k σ)]

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
