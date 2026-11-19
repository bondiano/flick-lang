import Formal.ValueTyping

/-!
# The result of a rule after (Errors-In)

(Errors-In) (`Typ.errVal`) types any term of an error `E` at the errors of a row that holds
`E`, as (Conv) types it at an equivalent type. A rule whose result type `t` is compared with
the type `T` it gives (`LeLive t T`) then gives `T` only through such steps: `t` is below a
type `T₀` that is `T`, or `T₀` is an error of the row of `T = Errors ε`.
-/

namespace Flick

variable {S : Sig}

/-- `t` is below `T₀` (`LeLive`), and `T₀` is `T` or one of the errors of `T = Errors ε`. -/
def LeOut (S : Sig) (t T : Ty) : Prop :=
  ∃ T₀, LeLive S t T₀ ∧
    (T₀ = T ∨ ∃ E ε, T₀.hd = .adt E ∧ Row.Sub (Row.err E T₀) ε.asRow ∧ T = .errors ε)

theorem LeOut.of {t T : Ty} (h : LeLive S t T) : LeOut S t T := ⟨T, h, .inl rfl⟩

/-- (Conv) after the rule. -/
theorem LeOut.equiv {t a b : Ty} (h : LeOut S t a) (he : Ty.Equiv a b) : LeOut S t b := by
  obtain ⟨T₀, ht, rfl | ⟨E, ε, hU, hE, rfl⟩⟩ := h
  · exact ⟨b, ht.trans (.equiv he), .inl rfl⟩
  · obtain ⟨ε', rfl, hε⟩ := he.symm.errors_right
    exact ⟨T₀, ht, .inr ⟨E, ε', hU, by rw [hε.asRow]; exact hE, rfl⟩⟩

/-- (Errors-In) after the rule: an error is the type of no row's errors, so `T₀` is `U`. -/
theorem LeOut.errIn {t U ε E} (h : LeOut S t U) (hU : U.hd = .adt E)
    (hE : Row.Sub (Row.err E U) ε.asRow) : LeOut S t (.errors ε) := by
  obtain ⟨T₀, ht, rfl | ⟨_, _, _, _, rfl⟩⟩ := h
  · exact ⟨T₀, ht, .inr ⟨E, ε, hU, hE, rfl⟩⟩
  · simp [Ty.hd] at hU

/-- A rule that gives every type above `t` gives `T`, by (Errors-In) if it has to. -/
theorem LeOut.typ {Ψ Θ Γ e t T φ} (h : LeOut S t T)
    (k : ∀ T₀, LeLive S t T₀ → Typ S Ψ Θ Γ e (.ex T₀ φ)) : Typ S Ψ Θ Γ e (.ex T φ) := by
  obtain ⟨T₀, ht, rfl | ⟨E, ε, hU, hE, rfl⟩⟩ := h
  · exact k _ ht
  · exact .errVal (k _ ht) hU hE

end Flick
