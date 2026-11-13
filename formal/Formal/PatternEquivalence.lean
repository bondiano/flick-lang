import Formal.SubstPat

/-!
# Completed patterns at congruent field types

The pattern keeps the types of its bindings, so the same checked arm body can
be reused at an equivalent presentation of the constructor's fields.
-/

namespace Flick

theorem Ty.ArgsEquiv.of_subst (ts : List Ty) {σ τ : Atom → Ty}
    (h : ∀ a, Ty.Equiv (σ a) (τ a)) :
    Ty.ArgsEquiv (ts.map (Ty.substT σ)) (ts.map (Ty.substT τ)) := by
  induction ts with
  | nil => exact .nil
  | cons t ts ih => exact .cons (t.substT_equiv h) ih

theorem PatsTy.equiv {S : Sig} {qs xs ys Δ} (h : PatsTy S qs xs Δ) (he : Ty.ArgsEquiv xs ys) :
    PatsTy S qs ys Δ := by
  induction h generalizing ys with
  | nil => cases he; exact .nil
  | cons hp _ ih => cases he with
    | cons ht hs => exact .cons (.conv hp ht) (ih hs)

end Flick
