import Formal.Rules
import Formal.Elab.T6a
import Formal.Elab.Sugar

/-!
# λFlick⁺ as a `Target`: the types

The types of `Elab` in the core of `Formal/` with regions and dictionary records:
`var ↦ tvar`, `app ↦ tapp`, `con c ↦ adt`, `Bool ↦ base bool`, `Int ↦ base int`,
`arr φ a b q ↦ ⟦a⟧ →⟦φ⟧^(⌈⟦q⟧⌉ ∪ vars φ) ⟦b⟧`,
`cnil ↦ caps ∅`, `cun ↦ caps (∪)`, `D_{C τ̄} ↦ dict (cls C |τ̄|) τ̄`. A stage `R r e` is
`rt ⟦r⟧ (⟦e⟧ ∪ ε)`, a row `⟦r⟧` what the type `⟦r⟧` stands for as a row (`Ty.asRow`, without its
atoms for a row of errors, `Row.noAtoms`: an error carries no regions; a variable is
a row variable), `ε` every name (`allNames`, `docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`). The qualifier of a translated
arrow also counts the variables of its rows, as `⌈·⌉` of the source does.
The translation commutes with substitution (`trT_subst`, `trTT_subst`, `trS_subst`) and with
`⌈·⌉` (`trT_reach`): a capture variable is an ordinary variable on both sides.

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
`⟦R⟧` lets a body raise any error type whose values reach nothing (`docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`). -/
def allNames : Row
  | .nm _ => True
  | _ => False

@[simp] theorem allNames_substT (σ : Atom → Flick.Ty) : allNames.substT σ = allNames := by
  funext x; cases x <;> simp [Row.substT, allNames]

@[simp] theorem allNames_substE (σ : Atom → Flick.Ty) : allNames.substE σ = allNames := by
  funext x; cases x <;> simp [Row.substE, Row.substT, allNames]

/-- The variables of the rows of a stage, as atoms. -/
def Stage.atoms (φ : Stage) : Caps := fun a => ∃ n, a = .v n ∧ φ.Occurs n

/-- The qualifier of a translated arrow: its own, and at `R r e` the variables of the rows. -/
def Stage.qual (φ : Stage) (C : Caps) : Caps :=
  match φ with
  | .P => C
  | .R .. => fun x => C x ∨ φ.atoms x

/-- `⟦τ⟧`. -/
def trT : Ty → Flick.Ty
  | .var n => .tvar n
  | .con c => if c = 2 then .base .bool else if c = 4 then .base .int else .adt (N.con c)
  | .app t u => .tapp (trT t) (trT u)
  | .arr .P a b q => .arrow (trT a) .pure (trT b) (trT q).reach
  | .arr (.R r e) a b q => .arrow (trT a) (.rt (trT r).asRow (Row.union (trT e).asRow.noAtoms allNames))
      (trT b) fun x => (trT q).reach x ∨ (Stage.R r e).atoms x
  | .cnil => .caps Caps.none
  | .cun q q' => .caps fun a => (trT q).reach a ∨ (trT q').reach a

/-- `⟦P⟧ = pure`, `⟦R r e⟧ = rt ⟦r⟧ (⟦e⟧ ∪ allNames)`, `⟦e⟧` without atoms. -/
def trStage : Stage → Flick.Stage
  | .P => .pure
  | .R r e => .rt (trT N r).asRow (Row.union (trT N e).asRow.noAtoms allNames)

theorem trT_arr (φ : Stage) (a b q : Ty) :
    trT N (.arr φ a b q) = .arrow (trT N a) (trStage N φ) (trT N b) (φ.qual (trT N q).reach) := by
  cases φ <;> rfl

theorem Stage.qual_reach (φ : Stage) (C : Caps) (x : Atom) : φ.qual C x ↔ C x ∨ φ.atoms x := by
  cases φ <;> simp [Stage.qual, Stage.atoms, Stage.Occurs]

/-- `⟦D_Q⟧`. -/
def trD (q : Cn) : Flick.Ty := .dict (N.cls q.cls q.args.length) (Flick.Ty.spine (q.args.map (trT N)))

/-- `⟦t⟧` of a type of an elaborated term. -/
def trTT : TTy → Flick.Ty
  | .mono t => trT N t
  | .arr .P a b q => .arrow (trTT a) .pure (trTT b) (trT N q).reach
  | .arr (.R r e) a b q => .arrow (trTT a) (trStage N (.R r e)) (trTT b)
      fun x => (trT N q).reach x ∨ (Stage.R r e).atoms x
  | .dict q => trD N q

theorem trTT_arr (φ : Stage) (a b : TTy) (q : Ty) :
    trTT N (.arr φ a b q) = .arrow (trTT N a) (trStage N φ) (trTT N b) (φ.qual (trT N q).reach) := by
  cases φ <;> rfl

/-- `⟦∀ᾱ. t⟧`, without facts. -/
def trS (s : TSch) : Flick.Scheme := ⟨s.k, Facts.none, trTT N s.ty⟩

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

/-- `⌈⟦τ⟧⌉ = ⌈τ⌉`: only variables. -/
theorem trT_reach (t : Ty) (a : Atom) : (trT N t).reach a ↔ ∃ n, a = .v n ∧ t.Occurs n := by
  induction t using Ty.ind generalizing a with
  | var m => simp [trT, Ty.Occurs]
  | con c =>
    simp only [trT, Ty.Occurs, and_false, exists_false, iff_false]
    split <;> (try split) <;> simp [Flick.Ty.reach, Caps.none]
  | app t u iht ihu => simp only [trT, Flick.Ty.reach, iht, ihu, Ty.Occurs, ex_or]
  | arr φ a b q iha ihb ihq _ =>
    simp only [trT_arr, Flick.Ty.reach, Stage.qual_reach, iha, ihb, ihq, Ty.Occurs, Stage.atoms,
      ex_or]
  | cnil => simp [trT, Flick.Ty.reach, Caps.none, Ty.Occurs]
  | cun q q' ihq ihq' => simp only [trT, Flick.Ty.reach, ihq, ihq', Ty.Occurs, ex_or]

theorem trT_reach_v {t : Ty} {n : Nat} (h : t.Occurs n) : (trT N t).reach (.v n) :=
  (trT_reach N t _).2 ⟨n, rfl, h⟩

theorem trD_reach (q : Cn) (a : Atom) : ¬ (trD N q).reach a := by
  simp [trD, Flick.Ty.reach, Caps.none]

theorem trTT_reach (t : TTy) (a : Atom) : (trTT N t).reach a ↔ ∃ n, a = .v n ∧ t.reach n := by
  induction t generalizing a with
  | mono t => exact trT_reach N t a
  | arr φ a b q iha ihb =>
    simp only [trTT_arr, Flick.Ty.reach, Stage.qual_reach, iha, ihb, trT_reach, TTy.reach,
      Stage.atoms, ex_or]
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

theorem Ty.occurs_subst_iff (σ : Nat → Ty) (t : Ty) (n : Nat) :
    (t.subst σ).Occurs n ↔ ∃ m, t.Occurs m ∧ (σ m).Occurs n := by
  induction t using Ty.ind with
  | var m => simp [Ty.subst, Ty.Occurs]
  | con | cnil => simp [Ty.subst, Ty.Occurs]
  | app t u iht ihu | cun t u iht ihu =>
    simp only [Ty.subst, Ty.Occurs, iht, ihu]; grind
  | arr φ a b q iha ihb ihq ihφ =>
    cases φ with
    | P => simp only [Ty.subst, Stage.subst, Ty.Occurs, Stage.Occurs, iha, ihb, ihq]; grind
    | R r e =>
      simp only [Ty.subst, Stage.subst, Ty.Occurs, Stage.Occurs, iha, ihb, ihq, ihφ.1, ihφ.2]; grind

theorem Stage.atoms_subst (φ : Stage) (σ : Nat → Ty) :
    (φ.subst σ).atoms = φ.atoms.substT (trσ N σ) := by
  funext x; apply propext
  simp only [Stage.atoms, Caps.substT]
  cases φ with
  | P => simp [Stage.subst, Stage.Occurs]
  | R r e =>
    simp only [Stage.subst, Stage.Occurs, Ty.occurs_subst_iff]
    constructor
    · rintro ⟨n, rfl, ⟨m, hm, hn⟩ | ⟨m, hm, hn⟩⟩
      · exact ⟨.v m, ⟨m, rfl, .inl hm⟩, trT_reach_v N hn⟩
      · exact ⟨.v m, ⟨m, rfl, .inr hm⟩, trT_reach_v N hn⟩
    · rintro ⟨b, ⟨m, rfl, hm⟩, hx⟩
      obtain ⟨n, rfl, hn⟩ := (trT_reach N _ x).1 hx
      exact ⟨n, rfl, hm.imp (fun h => ⟨m, h, hn⟩) fun h => ⟨m, h, hn⟩⟩

/-- **The translation commutes with substitution.** -/
theorem trT_subst (t : Ty) (σ : Nat → Ty) : trT N (t.subst σ) = (trT N t).substT (trσ N σ) := by
  induction t using Ty.ind with
  | var n => rfl
  | con c => simp only [Ty.subst, trT]; split <;> (try split) <;> rfl
  | app t u iht ihu => simp [Ty.subst, trT, Flick.Ty.substT, iht, ihu]
  | arr φ a b q iha ihb ihq ihφ =>
    have hφ : trStage N (φ.subst σ) = (trStage N φ).substT (trσ N σ) := by
      cases φ with
      | P => rfl
      | R r e =>
        simp [Stage.subst, trStage, Flick.Stage.substT, ihφ.1, ihφ.2, Ty.asRow_substT, Row.noAtoms_substE]
    have hq : (φ.subst σ).qual (trT N (q.subst σ)).reach =
        Caps.substT (trσ N σ) (φ.qual (trT N q).reach) := by
      rw [ihq, ← Ty.reach_caps]
      cases φ with
      | P => rfl
      | R r e =>
        show (fun x => _ ∨ _) = Caps.substT _ (fun x => _ ∨ _)
        rw [Caps.substT_or, ← Stage.atoms_subst N (.R r e) σ]
    simp only [Ty.subst, trT_arr, Flick.Ty.substT, iha, ihb, hφ, hq]
  | cnil => simp [Ty.subst, trT, Flick.Ty.substT]
  | cun q q' ihq ihq' =>
    simp only [Ty.subst, trT, Flick.Ty.substT, ihq, ihq', Caps.substT_or, Ty.reach_caps]

theorem trStage_subst (φ : Stage) (σ : Nat → Ty) :
    trStage N (φ.subst σ) = (trStage N φ).substT (trσ N σ) := by
  cases φ with
  | P => rfl
  | R r e =>
    simp [Stage.subst, trStage, Flick.Stage.substT, trT_subst, Ty.asRow_substT, Row.noAtoms_substE]

theorem trD_subst (q : Cn) (σ : Nat → Ty) : trD N (q.subst σ) = (trD N q).substT (trσ N σ) := by
  simp only [trD, Cn.subst, Flick.Ty.substT, Flick.Ty.spine_substT, List.map_map, List.length_map]
  congr 2
  exact List.map_congr_left fun t _ => trT_subst N t σ

theorem trTT_subst (t : TTy) (σ : Nat → Ty) :
    trTT N (t.subst σ) = (trTT N t).substT (trσ N σ) := by
  induction t with
  | mono t => exact trT_subst N t σ
  | arr φ a b q iha ihb =>
    have hq : (φ.subst σ).qual (trT N (q.subst σ)).reach =
        Caps.substT (trσ N σ) (φ.qual (trT N q).reach) := by
      rw [trT_subst, ← Ty.reach_caps]
      cases φ with
      | P => rfl
      | R r e =>
        show (fun x => _ ∨ _) = Caps.substT _ (fun x => _ ∨ _)
        rw [Caps.substT_or, ← Stage.atoms_subst N (.R r e) σ]
    simp only [TTy.subst, trTT_arr, Flick.Ty.substT, iha, ihb, trStage_subst, hq]
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
