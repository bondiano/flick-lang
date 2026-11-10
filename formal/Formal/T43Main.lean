import Formal.T43

/-!
# T4.3 and T1 with `op_H` (regions.md §5.3)
-/

namespace Flick

variable {S : Sig}

/-- The region of the handler of an operation is written in the context. -/
theorem handlerOf_has {r : Tm} {Ef o : Name} {c : OpClass} {q : Nat} :
    ∀ {E : List Frame}, handlerOf E Ef o c = some q → (plug E r).Has (.rgn q)
  | [], h => by simp [handlerOf] at h
  | F :: E, h => by
    simp only [handlerOf] at h
    cases hF : F.handles Ef o c with
    | some p =>
      rw [hF] at h; cases h
      cases F with
      | hwith p' ℓ' E' al rf =>
        simp only [Frame.handles] at hF
        split at hF
        · cases hF
          exact plug_has E (by simp [Frame.plug, Tm.Has])
        · cases hF
      | hkeep q' ℓ' E' al =>
        simp only [Frame.handles] at hF
        split at hF
        · cases hF
          exact plug_has E (by simp [Frame.plug, Tm.Has])
        · cases hF
      | _ => simp [Frame.handles] at hF
    | none => rw [hF] at h; exact handlerOf_has (r := F.plug r) h

/-- The effects with an operation that is not `tail`, as a row. -/
def Row.nonTail (S : Sig) : Row
  | .nm N => S.HasNonTail N
  | _ => False

end Flick
