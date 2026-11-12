import FormalCert.Tactics

/-!
# Checks of the certificate tactics

Goals of the shapes the certificates of generated programs met and the tactics of
`Tactics.lean` once failed on or did not end on: each is checked by every `lake build`, not
only by the run of `just model-certs`.
-/

namespace Flick.Cert.Checks

open Flick Flick.Cert

/-- A constructor's type with a function field, written out (`le_live`); its reach goes
through `split_mem`, which once split the witness of an `∃` without end. -/
example : LeLive S (Sig.ctorTy [(.arrow (.base .unit) (.rt Row.none Row.none) (.base .unit) Caps.none)]
      "K" [])
    (.arrow (.arrow (.base .unit) (.rt Row.none Row.none) (.base .unit) Caps.none) .pure (.adt "K")
      Caps.none) := by
  le_live

/-- A record whose field widens its qualifier (`le_live` through `LeLive.rcons'`). -/
example : LeLive S (.rcons "f" (.arrow (.base .unit) .pure (.base .unit) Caps.none) .rnil)
    (.rcons "f" (.arrow (.base .unit) .pure (.base .unit) (fun x => x = .r 0)) .rnil) := by
  le_live

/-- A fact whose capture row is empty at the instance (`outl` with `Caps.none`). -/
example : (Scheme.mk 3 (Facts.single (fun x => x = .v 1) (.tvar 2)) (.base .unit)).Holds Facts.none
    [(.row (fun x => x = .nm "L")), (.caps Caps.none), (.tvar 0)] := by
  holds

/-- A fact of the scheme around, after the shifted facts outside it (`outl` through `∨`). -/
example : Outl (Facts.union ((Facts.single (fun x => x = .v 1) (.tvar 0)).substT (shiftV 2))
      (Facts.single (fun x => x = .v 1) (.tvar 0))) (fun x => x = .v 1) (.tvar 0) := by
  outl

/-- The instance of a scheme shifted under a binder (`inst_eq` through `liftT`). -/
example : (Scheme.up 1 ⟨1, Facts.none,
      .rcons "go" (.arrow (.tvar 0) (.rt (fun x => x = .atm (.v 1)) Row.none) (.base .unit) Caps.none)
        .rnil⟩).inst [.base .unit] =
    .rcons "go" (.arrow (.base .unit) (.rt (fun x => x = .atm (.v 1)) Row.none) (.base .unit) Caps.none)
      .rnil := by
  inst_eq

end Flick.Cert.Checks
