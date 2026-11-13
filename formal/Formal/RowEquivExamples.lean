import Formal.IndexedRows
import Formal.TypeEquivShape
import Formal.TypeEquivInvert

namespace Flick

example (r s : Ty) : Ty.Equiv (.ref (.run r s)) (.ref (.run s r)) :=
  .ref (.run_comm r s)

example (σ : Atom → Ty) (r s : Ty) :
    Ty.Equiv ((Ty.ref (.run r s)).substT σ) ((Ty.ref (.run s r)).substT σ) :=
  (Ty.Equiv.ref (.run_comm r s)).substT σ

example (r s : Ty) : Ty.Equiv (.ref (.rnm "Store" (.run r s)))
    (.ref (.rnm "Store" (.run s r))) := .ref (.rnm (.run_comm r s))

example : ¬ Ty.Equiv (.rnm "Store" (.base .int)) (.rnm "Store" (.base .bool)) :=
  Ty.rnm_int_not_bool "Store"

example (r s : Ty) :
    Ty.Equiv ((Ty.ref (.tvar 0)).substT (fun _ => .run r s))
      ((Ty.ref (.tvar 0)).substT (fun _ => .run s r)) :=
  Ty.substT_equiv _ (fun _ => .run_comm r s)

example : ¬ Ty.Equiv (.base .int) (.ref (.base .int)) := by
  intro h; have := h.kind; contradiction

example : ¬ Ty.Equiv (.base .int) (.run (.base .int) .cnil) := by
  intro h; have := h.kind; contradiction

example (r s : Ty) : ∃ a, Ty.ref (.run r s) = .ref a ∧ Ty.Equiv a (.run s r) :=
  (Ty.Equiv.ref (.run_comm r s)).ref_right

end Flick
