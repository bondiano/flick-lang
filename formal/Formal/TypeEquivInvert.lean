import Formal.TypeEquiv

/-!
# Inversion of invariant types and record fields modulo congruence
-/

namespace Flick

private theorem option_symm {a b : Option Ty} (h : Option.Rel Ty.Equiv a b) :
    Option.Rel Ty.Equiv b a := by
  cases h with
  | none => exact .none
  | some h => exact .some h.symm

private theorem option_trans {a b c : Option Ty} (h : Option.Rel Ty.Equiv a b)
    (h' : Option.Rel Ty.Equiv b c) : Option.Rel Ty.Equiv a c := by
  cases h with
  | none => cases h'; exact .none
  | some h => cases h' with | some h' => exact .some (h.trans h')

inductive Ty.UnaryHead
  | ref
  | hnd
  | owner

def Ty.unaryArg : Ty.UnaryHead → Ty → Option Ty
  | .ref, .ref a | .hnd, .hnd a | .owner, .owner a => some a
  | _, _ => none

theorem Ty.RowForm.unaryArg {r : Ty} (h : r.RowForm) (head : Ty.UnaryHead) :
    r.unaryArg head = none := by
  cases h <;> cases head <;> rfl

theorem Ty.Equiv.unaryArg {a b : Ty} (h : Ty.Equiv a b) (head : Ty.UnaryHead) :
    Option.Rel Ty.Equiv (a.unaryArg head) (b.unaryArg head) := by
  induction h with
  | refl => cases Ty.unaryArg head _ <;> constructor; exact .refl
  | symm _ ih => exact option_symm ih
  | trans _ _ ih₁ ih₂ => exact option_trans ih₁ ih₂
  | ref h | hnd h | owner h => cases head <;> first | exact .some h | exact .none
  | run_idem r hr | run_nil r hr | rless_none r hr =>
    rw [hr.unaryArg head]
    cases head <;> exact .none
  | _ => cases head <;> exact .none

theorem Ty.Equiv.ref_right {a b : Ty} (h : Ty.Equiv a (.ref b)) :
    ∃ a', a = .ref a' ∧ Ty.Equiv a' b := by
  have hu := h.unaryArg .ref
  cases a <;> simp only [Ty.unaryArg] at hu
  all_goals first
    | exact ⟨_, rfl, Option.rel_some_some.mp hu⟩
    | cases hu

theorem Ty.RowForm.field {r : Ty} (h : r.RowForm) (l : Label) : r.field l = none := by
  cases h <;> rfl

theorem Ty.Equiv.field {a b : Ty} (h : Ty.Equiv a b) (l : Label) :
    Option.Rel Ty.Equiv (a.field l) (b.field l) := by
  induction h with
  | refl => cases Ty.field _ l <;> constructor; exact .refl
  | symm _ ih => exact option_symm ih
  | trans _ _ ih₁ ih₂ => exact option_trans ih₁ ih₂
  | rcons ha _ _ ihr =>
    simp only [Ty.field]
    split
    · exact .some ha
    · exact ihr
  | run_idem r hr | run_nil r hr | rless_none r hr =>
    simp only [Ty.field, hr.field]; exact .none
  | _ => exact .none

end Flick
