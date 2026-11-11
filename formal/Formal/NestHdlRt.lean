import Formal.NestHdlPure

/-!
# The run-time steps of handlers keep `⊢ Λ`

A raised error or a signal of an arm unwinds the parked body to the handler; one of a
resumed body goes into the arm. The handler catches its signal, and what the signal
holds runs at the place of the handler.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

end Flick
