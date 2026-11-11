import FormalCert.Tactics

/-!
# Checks of the certificate tactics

Goals of the shapes the certificates of generated programs met and the tactics of
`Tactics.lean` once failed on or did not end on: each is checked by every `lake build`, not
only by the run of `just model-certs`.
-/

namespace Flick.Cert.Checks

open Flick Flick.Cert

/-- A fact whose capture row is empty at the instance (`outl` with `Caps.none`). -/
example : (Scheme.mk 3 (Facts.single (fun x => x = .v 1) (.tvar 2)) (.base .unit)).Holds Facts.none
    [(.row (fun x => x = .nm "L")), (.caps Caps.none), (.tvar 0)] := by
  holds

/-- A fact of the scheme around, after the shifted facts outside it (`outl` through `∨`). -/
example : Outl (Facts.union ((Facts.single (fun x => x = .v 1) (.tvar 0)).substT (shiftV 2))
      (Facts.single (fun x => x = .v 1) (.tvar 0))) (fun x => x = .v 1) (.tvar 0) := by
  outl

end Flick.Cert.Checks
