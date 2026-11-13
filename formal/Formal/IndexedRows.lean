import Formal.TypeEquivSubst

/-!
# Rows indexed by type arguments

Unlike the legacy `Elem`, these elements retain the arguments of services
and errors modulo the type congruence. The projection is used to validate
that congruence before transporting typing across it.
-/

namespace Flick

def Ty.equivSetoid : Setoid Ty where
  r := Ty.Equiv
  iseqv := ⟨fun _ => .refl, fun h => .symm h, fun h h' => .trans h h'⟩

abbrev TyClass := Quotient Ty.equivSetoid

def Ty.classOf (t : Ty) : TyClass := Quotient.mk _ t

inductive IndexedElem
  | nm (name : Name) (args : TyClass)
  | atm (atom : Atom)
  | ob (name : Name) (args : TyClass) (atom : Atom)
  | rv (atom : Atom) (gone : Name → Prop)
  | obv (varAtom : Atom) (gone : Name → Prop) (owner : Atom)

abbrev IndexedRow := IndexedElem → Prop

def IndexedRow.none : IndexedRow := fun _ => False
def IndexedRow.union (r s : IndexedRow) : IndexedRow := fun x => r x ∨ s x
def IndexedRow.single (N : Name) (args : TyClass) : IndexedRow := fun x => x = .nm N args
def IndexedRow.ofCaps (C : Caps) : IndexedRow
  | .atm a => C a
  | _ => False

def IndexedRow.less (G : Name → Prop) (r : IndexedRow) : IndexedRow
  | .nm N args => ¬ G N ∧ r (.nm N args)
  | .atm a => r (.atm a)
  | .rv c H => ∃ H', r (.rv c H') ∧ H = fun n => G n ∨ H' n
  | _ => False

/-- Legacy rows have no type arguments. Their plain part embeds at the
empty argument spine; obligations are not imported through a row variable. -/
def IndexedRow.ofLegacy (r : Row) : IndexedRow
  | .nm N args => args = Ty.rnil.classOf ∧ r (.nm N)
  | .atm a => r (.atm a)
  | .rv c G => r (.rv c G)
  | _ => False

def Ty.asIndexedRow : Ty → IndexedRow
  | .tvar n => fun x => x = .rv (.v n) fun _ => False
  | .rg p => fun x => x = .rv (.r p) fun _ => False
  | .row r => .ofLegacy r
  | .cnil => .none
  | .rnm N args => .union (.single N args.classOf) (.ofCaps args.reach)
  | .run r s => .union r.asIndexedRow s.asIndexedRow
  | .rless G r => r.asIndexedRow.less G
  | t => .ofCaps t.reach

@[simp] theorem Ty.asIndexedRow_ob (t : Ty) (N : Name) (args : TyClass) (a : Atom) :
    ¬ t.asIndexedRow (.ob N args a) := by
  induction t <;> simp_all [Ty.asIndexedRow, IndexedRow.ofLegacy, IndexedRow.ofCaps,
    IndexedRow.none, IndexedRow.single, IndexedRow.union, IndexedRow.less]

@[simp] theorem Ty.asIndexedRow_obv (t : Ty) (v a : Atom) (G : Name → Prop) :
    ¬ t.asIndexedRow (.obv v G a) := by
  induction t <;> simp_all [Ty.asIndexedRow, IndexedRow.ofLegacy, IndexedRow.ofCaps,
    IndexedRow.none, IndexedRow.single, IndexedRow.union, IndexedRow.less]

theorem Ty.Equiv.indexedRow {a b : Ty} (h : Ty.Equiv a b) :
    a.asIndexedRow = b.asIndexedRow := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | arrow h₁ h₂ => exact congrArg IndexedRow.ofCaps (Ty.Equiv.arrow h₁ h₂).reach
  | ref h => exact congrArg IndexedRow.ofCaps (Ty.Equiv.ref h).reach
  | tapp h₁ h₂ => exact congrArg IndexedRow.ofCaps (Ty.Equiv.tapp h₁ h₂).reach
  | rcons h₁ h₂ => exact congrArg IndexedRow.ofCaps (Ty.Equiv.rcons h₁ h₂).reach
  | eff h => exact congrArg IndexedRow.ofCaps (Ty.Equiv.eff h).reach
  | hnd h => exact congrArg IndexedRow.ofCaps (Ty.Equiv.hnd h).reach
  | owner h => exact congrArg IndexedRow.ofCaps (Ty.Equiv.owner h).reach
  | dict h => exact congrArg IndexedRow.ofCaps (Ty.Equiv.dict h).reach
  | all h => exact congrArg IndexedRow.ofCaps (Ty.Equiv.all h).reach
  | rnm h =>
    have he : Ty.classOf _ = Ty.classOf _ := Quotient.sound h
    simp only [Ty.asIndexedRow, he, h.reach]
  | run _ _ ih₁ ih₂ => simp only [Ty.asIndexedRow, ih₁, ih₂]
  | rless G _ ih => simp only [Ty.asIndexedRow, ih]
  | rt => rfl
  | run_comm r s => funext x; simp [Ty.asIndexedRow, IndexedRow.union, or_comm]
  | run_assoc r s t => funext x; simp [Ty.asIndexedRow, IndexedRow.union, or_assoc]
  | run_idem r _ => funext x; simp [Ty.asIndexedRow, IndexedRow.union]
  | run_nil r _ => funext x; simp [Ty.asIndexedRow, IndexedRow.union, IndexedRow.none]
  | rless_nil G => funext x; cases x <;> simp [Ty.asIndexedRow, IndexedRow.less, IndexedRow.none]
  | rless_none r _ =>
    funext x; cases x <;> simp [Ty.asIndexedRow, IndexedRow.less]
  | rless_run G r s =>
    funext x; apply propext
    cases x <;> simp only [Ty.asIndexedRow, IndexedRow.less, IndexedRow.union] <;> grind
  | rless_merge G H r =>
    funext x; apply propext
    cases x with
    | nm N args | atm a | ob N args a | obv v G a => simp [Ty.asIndexedRow, IndexedRow.less] <;> grind
    | rv c K =>
      simp only [Ty.asIndexedRow, IndexedRow.less]
      constructor
      · rintro ⟨K', ⟨K'', h, rfl⟩, rfl⟩
        exact ⟨K'', h, by funext n; apply propext; exact or_assoc.symm⟩
      · rintro ⟨K', h, rfl⟩
        exact ⟨fun n => H n ∨ K' n, ⟨K', h, rfl⟩, by
          funext n; apply propext; exact or_assoc⟩
  | rless_keep G N a h | rless_drop G N a h =>
    funext x; cases x <;>
      simp [Ty.asIndexedRow, IndexedRow.less, IndexedRow.union, IndexedRow.single,
        IndexedRow.ofCaps, Ty.reach] <;> grind
  | caps_nil =>
    funext x; cases x <;> simp [Ty.asIndexedRow, Ty.reach, IndexedRow.ofCaps, IndexedRow.none, Caps.none]
  | caps_union C D =>
    funext x; cases x <;> simp [Ty.asIndexedRow, Ty.reach, IndexedRow.ofCaps, IndexedRow.union]
  | rless_caps G C =>
    funext x; cases x <;> simp [Ty.asIndexedRow, Ty.reach, IndexedRow.ofCaps, IndexedRow.less]

theorem Ty.rnm_int_not_bool (N : Name) :
    ¬ Ty.Equiv (.rnm N (.base .int)) (.rnm N (.base .bool)) := by
  intro h
  have hi : (Ty.rnm N (.base .int)).asIndexedRow (.nm N (Ty.base .int).classOf) := .inl rfl
  have hb := Eq.mp (congrFun h.indexedRow (.nm N (Ty.base .int).classOf)) hi
  have he : (Ty.base .int).classOf = (Ty.base .bool).classOf := by
    simpa [Ty.asIndexedRow, IndexedRow.union, IndexedRow.single, IndexedRow.ofCaps] using hb
  have hc := (Quotient.exact he : Ty.Equiv (.base .int) (.base .bool)).base_inj
  cases hc

end Flick
