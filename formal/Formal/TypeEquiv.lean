import Formal.RowTerms

/-!
# Congruence of syntactic types and rows

Row union is associative, commutative and idempotent. These equations are
available under invariant type constructors as equality, not as subtyping.
Named arguments are compared by this same congruence, never by the old
parameter-free `asRow` projection alone.
-/

namespace Flick

theorem Ty.Equiv.reach {a b : Ty} (h : Ty.Equiv a b) : a.reach = b.reach := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | run_comm r s => funext x; simp [Ty.reach, or_comm]
  | run_assoc r s t => funext x; simp [Ty.reach, or_assoc]
  | run_idem r _ => funext x; simp [Ty.reach]
  | run_nil r _ => funext x; simp [Ty.reach, Caps.none]
  | _ => simp_all [Ty.reach]

def Ty.baseView : Ty → Option Base
  | .base x => some x
  | _ => none

theorem Ty.RowForm.baseView {r : Ty} (h : r.RowForm) : r.baseView = none := by
  cases h <;> rfl

theorem Ty.Equiv.baseView {a b : Ty} (h : Ty.Equiv a b) : a.baseView = b.baseView := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | run_idem r hr | run_nil r hr | rless_none r hr => exact hr.baseView.symm
  | _ => rfl

theorem Ty.Equiv.base_inj {a b : Base} (h : Ty.Equiv (.base a) (.base b)) : a = b :=
  Option.some.inj h.baseView

theorem Ty.Equiv.asRow {a b : Ty} (h : Ty.Equiv a b) : a.asRow = b.asRow := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | arrow h₁ h₂ => exact congrArg Row.ofCaps (Ty.Equiv.arrow h₁ h₂).reach
  | ref h => exact congrArg Row.ofCaps (Ty.Equiv.ref h).reach
  | tapp h₁ h₂ => exact congrArg Row.ofCaps (Ty.Equiv.tapp h₁ h₂).reach
  | rcons h₁ h₂ => exact congrArg Row.ofCaps (Ty.Equiv.rcons h₁ h₂).reach
  | eff h => exact congrArg Row.ofCaps (Ty.Equiv.eff h).reach
  | hnd h => exact congrArg Row.ofCaps (Ty.Equiv.hnd h).reach
  | owner h => exact congrArg Row.ofCaps (Ty.Equiv.owner h).reach
  | dict h => exact congrArg Row.ofCaps (Ty.Equiv.dict h).reach
  | all h => exact congrArg Row.ofCaps (Ty.Equiv.all h).reach
  | rnm h => simp only [Ty.asRow, h.reach]
  | run _ _ ih₁ ih₂ => simp only [Ty.asRow, ih₁, ih₂]
  | rless G _ ih => simp only [Ty.asRow, ih]
  | rt => rfl
  | run_comm r s => funext x; simp [Ty.asRow, Row.union, or_comm]
  | run_assoc r s t => funext x; simp [Ty.asRow, Row.union, or_assoc]
  | run_idem r _ => funext x; simp [Ty.asRow, Row.union]
  | run_nil r _ => funext x; simp [Ty.asRow, Row.union, Row.none]
  | rless_nil G => funext x; cases x <;> simp [Ty.asRow, Row.less, Row.none]
  | rless_none r _ => funext x; cases x <;> simp [Ty.asRow, Row.less]
  | rless_run G r s =>
    funext x; apply propext
    cases x <;> simp only [Ty.asRow, Row.less, Row.union] <;> grind
  | rless_merge G H r =>
    funext x; apply propext
    cases x with
    | nm N | atm a | ob N a | obv v G a => simp [Ty.asRow, Row.less] <;> grind
    | rv c K =>
      simp only [Ty.asRow, Row.less]
      constructor
      · rintro ⟨K', ⟨K'', h, rfl⟩, rfl⟩
        exact ⟨K'', h, by funext n; apply propext; exact or_assoc.symm⟩
      · rintro ⟨K', h, rfl⟩
        exact ⟨fun n => H n ∨ K' n, ⟨K', h, rfl⟩, by
          funext n; apply propext; exact or_assoc⟩
  | rless_keep G N a h | rless_drop G N a h =>
    funext x; cases x <;>
      simp [Ty.asRow, Row.less, Row.union, Row.single, Row.ofCaps, Ty.reach] <;> grind
  | caps_nil => funext x; cases x <;> simp [Ty.asRow, Ty.reach, Row.ofCaps, Row.none, Caps.none]
  | caps_union C D => funext x; cases x <;> simp [Ty.asRow, Ty.reach, Row.ofCaps, Row.union]
  | rless_caps G C => funext x; cases x <;> simp [Ty.asRow, Ty.reach, Row.ofCaps, Row.less]

theorem Ty.RowForm.stage {r : Ty} (h : r.RowForm) : r.stage = none := by
  cases h <;> rfl

theorem Ty.Equiv.stage {a b : Ty} (h : Ty.Equiv a b) : a.stage = b.stage := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | rt hr he => simp only [Ty.stage, hr.asRow, he.asRow]
  | run_idem r hr | run_nil r hr | rless_none r hr => exact hr.stage.symm
  | _ => rfl

theorem Caps.union_substT (C D : Caps) (σ : Atom → Ty) :
    Caps.substT σ (fun a => C a ∨ D a) = (fun a => C.substT σ a ∨ D.substT σ a) := by
  funext a; apply propext; simp only [Caps.substT]; grind

theorem Ty.Equiv.substT {a b : Ty} (h : Ty.Equiv a b) (σ : Atom → Ty) :
    Ty.Equiv (a.substT σ) (b.substT σ) := by
  induction h generalizing σ with
  | refl => exact .refl
  | symm _ ih => exact .symm (ih σ)
  | trans _ _ ih₁ ih₂ => exact .trans (ih₁ σ) (ih₂ σ)
  | arrow _ _ ih₁ ih₂ => exact .arrow (ih₁ σ) (ih₂ σ)
  | ref _ ih => exact .ref (ih σ)
  | tapp _ _ ih₁ ih₂ => exact .tapp (ih₁ σ) (ih₂ σ)
  | rcons _ _ ih₁ ih₂ => exact .rcons (ih₁ σ) (ih₂ σ)
  | eff _ ih => exact .eff (ih σ)
  | hnd _ ih => exact .hnd (ih σ)
  | owner _ ih => exact .owner (ih σ)
  | dict _ ih => exact .dict (ih σ)
  | @all k _ _ _ ih => exact .all (ih (liftT k σ))
  | rnm _ ih => exact .rnm (ih σ)
  | run _ _ ih₁ ih₂ => exact .run (ih₁ σ) (ih₂ σ)
  | rless G _ ih => exact .rless G (ih σ)
  | rt _ _ ih₁ ih₂ => exact .rt (ih₁ σ) (ih₂ σ)
  | run_comm r s => exact .run_comm (r.substT σ) (s.substT σ)
  | run_assoc r s t => exact .run_assoc (r.substT σ) (s.substT σ) (t.substT σ)
  | run_idem r hr => exact .run_idem (r.substT σ) (hr.substT σ)
  | run_nil r hr => exact .run_nil (r.substT σ) (hr.substT σ)
  | rless_nil G => exact .rless_nil G
  | rless_none r hr => exact .rless_none (r.substT σ) (hr.substT σ)
  | rless_run G r s => exact .rless_run G (r.substT σ) (s.substT σ)
  | rless_merge G H r => exact .rless_merge G H (r.substT σ)
  | rless_keep G N a h => exact .rless_keep G N (a.substT σ) h
  | rless_drop G N a h =>
    simpa only [Ty.substT, Caps.substT_reach] using Ty.Equiv.rless_drop G N (a.substT σ) h
  | caps_nil => simpa only [Ty.substT, Caps.none_substT] using Ty.Equiv.caps_nil
  | caps_union C D =>
    simpa only [Ty.substT, Caps.union_substT] using Ty.Equiv.caps_union (C.substT σ) (D.substT σ)
  | rless_caps G C => exact .rless_caps G (C.substT σ)

theorem Ty.Equiv.ren {a b : Ty} (h : Ty.Equiv a b) (ξ : Atom → Atom) :
    Ty.Equiv (a.ren ξ) (b.ren ξ) := by
  simpa only [Ty.ren_eq] using h.substT (fun a => (ξ a).ty)

theorem Ty.Equiv.up {a b : Ty} (h : Ty.Equiv a b) (k : Nat) :
    Ty.Equiv (a.up k) (b.up k) := h.ren (Atom.up k)

end Flick
