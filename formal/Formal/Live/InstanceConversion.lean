import Formal.Live.Order

/-!
# Instantiation from an equivalent type
-/

namespace Flick

theorem LeLiveI.equiv_left {a b : Ty} (h : LeLiveI S a b) :
    ∀ {a'}, Ty.Equiv a' a → LeLiveI S a' b := by
  induction h with
  | le hl => intro a' he; exact .le ((LeLive.equiv he).trans hl)
  | inst hus _ ih =>
    intro a' he
    cases he.symm.form with
    | rows hr _ => cases hr
    | all ht => exact .inst hus (ih (ht.substT _).symm)

end Flick
