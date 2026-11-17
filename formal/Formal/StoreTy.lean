import Formal.Rules

/-!
# Typing of the store (core.md §3.6, lazy.md §3)

A cell holds a value of its type in `Ψ`, or it is a lazy cell: `Ψ` gives it `lazyCell r ε t C`,
a type no value has (`Typ.value_not_cell`), and it holds a state of the lazy.
-/

namespace Flick

variable (S : Sig)

/-- The states of a lazy cell of `Lazy r ε t C`: `Pending e` with `e` typed as the body of a
`task`, `Running`, and `Done o` with the term of an outcome of the lazy. -/
inductive CellTy (Ψ : List Ty) (r ε : Row) (t : Ty) (C : Caps) : Tm → Prop
  | pending : Typ S Ψ Facts.age [] e (.ex t ψ) → ψ.le (Stage.use (.rt r ε) C) → e.NoIns →
      CellTy Ψ r ε t C (.lazy e)
  | running : CellTy Ψ r ε t C .running
  | ok : Value v → Typ S Ψ Facts.age [] v (.ex t .pure) → CellTy Ψ r ε t C (.done v)
  | err : Value v → Typ S Ψ Facts.age [] v (.ex (.errors ε) .pure) →
      CellTy Ψ r ε t C (.done (.raise v))
  | defect : CellTy Ψ r ε t C (.done .todo)

/-- `⊢ μ : Ψ` of core.md §3.6. -/
def StoreTy (Ψ : List Ty) (μ : List Tm) : Prop :=
  μ.length = Ψ.length ∧
    ∀ (i : Nat) v t, μ[i]? = some v → Ψ[i]? = some t →
      (Value v ∧ Typ S Ψ Facts.age [] v (.ex t .pure)) ∨
        ∃ r ε a C, t = .lazyCell r ε a C ∧ CellTy S Ψ r ε a C v

variable {S}

theorem CellTy.isCell {Ψ r ε t C c} (h : CellTy S Ψ r ε t C c) : c.IsCell := by
  cases h <;> trivial

end Flick
