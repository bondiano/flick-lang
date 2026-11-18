import Formal.LetMut.Lazy
import Formal.Elab.Witness.Sig

/-!
# The witness signature has the constructors of a `let lazy`

`LzSig` holds of the signature `Sg` of the witness program, whose `Result` is the Prelude's, so the derived rules of `Formal.LetMut.Lazy` apply to a signature that exists.
-/

namespace Flick.Elab.Witness

theorem lzSig : LzSig Sg where
  ctorErr := rfl
  ctorOk := rfl
  resErr := sg_plain rfl
  resOk := sg_plain rfl
  result K k ts h := by
    change ctorF K = _ at h
    unfold ctorF at h
    split at h <;> simp_all

end Flick.Elab.Witness
