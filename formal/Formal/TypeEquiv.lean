import Formal.RowTerms

/-!
# Congruence of syntactic types and rows

Row union is associative, commutative and idempotent. These equations are
available under invariant type constructors as equality, not as subtyping.
What a row term stands for (`Ty.asRow`) and what a stage term stands for (`Ty.asStage`)
are invariant under it; a named element keeps its argument up to it.
-/

namespace Flick

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
  | arrow h₁ h₂ h₃ => exact congrArg Row.ofCaps (Ty.Equiv.arrow h₁ h₂ h₃).reach
  | ref h => exact congrArg Row.ofCaps (Ty.Equiv.ref h).reach
  | tapp h₁ h₂ => exact congrArg Row.ofCaps (Ty.Equiv.tapp h₁ h₂).reach
  | rcons h₁ h₂ => exact congrArg Row.ofCaps (Ty.Equiv.rcons h₁ h₂).reach
  | eff h₁ h₂ h₃ => exact congrArg Row.ofCaps (Ty.Equiv.eff h₁ h₂ h₃).reach
  | errors h => exact congrArg Row.ofCaps (Ty.Equiv.errors h).reach
  | lazy h₁ h₂ h₃ => exact congrArg Row.ofCaps (Ty.Equiv.lazy h₁ h₂ h₃).reach
  | lazyCell h₁ h₂ h₃ => exact congrArg Row.ofCaps (Ty.Equiv.lazyCell h₁ h₂ h₃).reach
  | hnd h => exact congrArg Row.ofCaps (Ty.Equiv.hnd h).reach
  | owner h => exact congrArg Row.ofCaps (Ty.Equiv.owner h).reach
  | dict h => exact congrArg Row.ofCaps (Ty.Equiv.dict h).reach
  | all h => exact congrArg Row.ofCaps (Ty.Equiv.all h).reach
  | rnm h => simp only [Ty.asRow, h.reach, h.classOf]
  | run _ _ ih₁ ih₂ => simp only [Ty.asRow, ih₁, ih₂]
  | rless G _ ih => simp only [Ty.asRow, ih]
  | rt | rtob => rfl
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
    | nm N c | atm a | ob N c a | obv v G a => simp [Ty.asRow, Row.less] <;> grind
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
      simp [Ty.asRow, Row.less, Row.union, Row.inst, Row.ofCaps, Ty.reach] <;> grind
  | caps_nil => funext x; cases x <;> simp [Ty.asRow, Ty.reach, Row.ofCaps, Row.none, Caps.none]
  | caps_union C D => funext x; cases x <;> simp [Ty.asRow, Ty.reach, Row.ofCaps, Row.union]
  | rless_caps G C => funext x; cases x <;> simp [Ty.asRow, Ty.reach, Row.ofCaps, Row.less]

/-- Atoms already in a row add nothing to it. -/
theorem Ty.Equiv.run_caps_absorb (D : Caps) (r : Ty) (H : Caps) (h : ∀ x, H x → D x) :
    Ty.Equiv (.run (.run (.caps D) r) (.caps H)) (.run (.caps D) r) := by
  have hD : (fun a => D a ∨ H a) = D := by
    funext a; exact propext ⟨fun h' => h'.elim id (h a), .inl⟩
  refine .trans (.run_assoc _ _ _) (.trans (.run .refl (.run_comm _ _)) ?_)
  refine .trans (.symm (.run_assoc _ _ _)) ?_
  refine .run ?_ .refl
  have := Ty.Equiv.caps_union D H
  rwa [hD] at this

theorem Ty.RowForm.asStage {r : Ty} (h : r.RowForm) : r.asStage = .pure := by
  cases h <;> rfl

theorem Row.obs_reach {r : Row} {s s' : Ty} (h : s.reach = s'.reach) : r.obs s = r.obs s' := by
  funext x; cases x <;> simp [Row.obs, h]

theorem Ty.Equiv.asStage {a b : Ty} (h : Ty.Equiv a b) : a.asStage = b.asStage := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | rt hr he => simp only [Ty.asStage, hr.asRow, he.asRow]
  | rtob hr hs he => simp only [Ty.asStage, hr.asRow, he.asRow, Row.obs_reach hs.reach]
  | run_idem r hr | run_nil r hr | rless_none r hr => exact hr.asStage.symm
  | _ => rfl

end Flick
