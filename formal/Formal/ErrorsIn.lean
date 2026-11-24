import Formal.ValueTyping

/-!
# The result of a rule after (Errors-In)

(Errors-In) (`Typ.errVal`) types any term of an error `E` at the errors of a row that holds
`E`, as (Conv) types it at an equivalent type. A rule whose result type `t` is compared with
the type `T` it gives (`LeLive t T`) then gives `T` only through such steps: `t` is below a
type `T₀` that is `T`, or `T₀` is an error of the row of `T = Errors ε`. (FuncRef-In)
(`Typ.funcRef`) types any term that names no local at `FuncRef τ` of its type `τ`, and wraps
either.
-/

namespace Flick

variable {S : Sig}

/-- For a rule that types the term `e`: `t` is below `T₀` (`LeLive`), and `T₀` is `T`, one of
the errors of `T = Errors ε`, or `T` is `FuncRef T₁` of such a `T₁` and `e` names no local. -/
inductive LeOut (S : Sig) (e : Tm) : Ty → Ty → Prop
  | of {t T : Ty} : LeLive S t T → LeOut S e t T
  | err {t U : Ty} {E : Name} {ε : Ty} : LeLive S t U → U.hd = .adt E →
      Row.Sub (Row.err E U) ε.asRow → LeOut S e t (.errors ε)
  /-- (FuncRef-In) after the rule. -/
  | funcRef {t T : Ty} : e.Closed → LeOut S e t T → LeOut S e t (.funcRef T)

/-- (Conv) after the rule. -/
theorem LeOut.equiv {e : Tm} {t a b : Ty} (h : LeOut S e t a) (he : Ty.Equiv a b) :
    LeOut S e t b := by
  induction h generalizing b with
  | of ht => exact .of (ht.trans (.equiv he))
  | err ht hU hE =>
    obtain ⟨ε', rfl, hε⟩ := he.symm.errors_right
    exact .err ht hU (by rw [hε.asRow]; exact hE)
  | funcRef hc _ ih =>
    obtain ⟨T', rfl, hT⟩ := he.symm.funcRef_right
    exact .funcRef hc (ih hT.symm)

/-- (Errors-In) after the rule: an error is the type of no row's errors nor a function
without captures, so `T₀` is `U`. -/
theorem LeOut.errIn {e t U ε E} (h : LeOut S e t U) (hU : U.hd = .adt E)
    (hE : Row.Sub (Row.err E U) ε.asRow) : LeOut S e t (.errors ε) := by
  cases h with
  | of ht => exact .err ht hU hE
  | err => simp [Ty.hd] at hU
  | funcRef => simp [Ty.hd] at hU

/-- A rule that gives every type above `t` gives `T`, by (Errors-In) and (FuncRef-In) if it
has to, for a term `e` that names no local when the rule's term `e₀` names none. -/
theorem LeOut.typ' {Ψ Θ Γ e₀ e t T φ} (h : LeOut S e₀ t T) (hc : e₀.Closed → e.Closed)
    (k : ∀ T₀, LeLive S t T₀ → Typ S Ψ Θ Γ e (.ex T₀ φ)) : Typ S Ψ Θ Γ e (.ex T φ) := by
  induction h with
  | of ht => exact k _ ht
  | err ht hU hE => exact .errVal (k _ ht) hU hE
  | funcRef h₀ _ ih => exact .funcRef ih (hc h₀)

/-- `LeOut.typ'` in the empty context, where every typed term names no local. -/
theorem LeOut.typ {Ψ Θ e₀ e t T φ} (h : LeOut S e₀ t T)
    (k : ∀ T₀, LeLive S t T₀ → Typ S Ψ Θ [] e (.ex T₀ φ)) : Typ S Ψ Θ [] e (.ex T φ) := by
  induction h with
  | of ht => exact k _ ht
  | err ht hU hE => exact .errVal (k _ ht) hU hE
  | funcRef _ _ ih => exact .funcRef ih ih.closed_nil

end Flick
