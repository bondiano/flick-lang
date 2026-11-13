import Formal.ArefMgu
import Formal.Patterns
import Formal.FreshEquivExamples
import Formal.Live.ConversionExamples

/-!
# Regression examples for type conversion

Commuting row terms under a nominal index changes the syntax but leaves the
GADT unification problem solved by identity. Completed patterns keep the types
of their bindings when their input is converted.
-/

namespace Flick

private def first : Ty := Ty.tcon "Box" [.run (.rnm "A" .rnil) (.rnm "B" .rnil)]
private def second : Ty := Ty.tcon "Box" [.run (.rnm "B" .rnil) (.rnm "A" .rnil)]

private theorem same_index : Ty.Equiv first second := .tapp .refl (.run_comm _ _)

private theorem fits_todo : Fits Ty.ident .todo :=
  .noIns (by simp [Tm.NoIns, Tm.Has]) (fun _ => rfl)

/-- Syntactic unification would incorrectly mark this closed index problem dead. -/
example (θ : Atom → Ty) : first.substT θ ≠ second.substT θ := by
  simp [first, second, Ty.tcon, Ty.substT]

/-- Congruence makes identity a most general unifier, with every other instance factored. -/
example : Mgu .todo Ty.ident first second := by
  refine ⟨fits_todo, same_index.substT _, fun θ' _ _ => ⟨θ', ?_⟩⟩
  intro a
  cases a <;> exact .refl

example : ¬ (∀ θ, Fits θ .todo → ¬ Ty.Equiv (first.substT θ) (second.substT θ)) := by
  intro h
  exact h Ty.ident fits_todo (same_index.substT _)

example (S : Sig) : PatTy S true .var (.ref second) [.ref first] :=
  .conv .var (.ref same_index)

/-- An equivalent query uses the declared fact, even though it has a different type syntax. -/
example : Outl (fun C s => C = (· = .r 0) ∧ s = first) (· = .r 0) second :=
  (Outl.fact ⟨rfl, rfl⟩).equiv same_index

/-- The binding has the checked arm's type even when the matched value uses the other form. -/
example {S : Sig} {Ψ Θ Γ v} (hv : Value v)
    (ht : Typ S Ψ Θ Γ v (.ex (.ref second) .pure)) : Inst S Ψ Θ Γ [v] [.ref first] :=
  (PatTy.conv PatTy.var (.ref same_index)).inst hv ht rfl

end Flick
