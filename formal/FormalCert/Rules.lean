import FormalCert.Tactics

/-!
# Derived rules of a certificate

Each is a theorem about `Typ` (`Formal/Rules.lean`), shaped so a certificate states the instance
it takes and proves the side conditions with the tactics of `Tactics.lean`.
-/

namespace Flick.Cert

variable {S : Sig} {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme}

/-- (Var) at an instance the certificate writes out: `σ.inst ts` is `T'`, a subtype of `T`. -/
theorem typ_var {i : Nat} {σ : Scheme} {ts : List Ty} {T' T : Ty} {φ : Stage}
    (h : Γ[i]? = some σ) (hk : ts.length = σ.k) (hH : σ.Holds Θ ts)
    (he : σ.inst ts = T') (hl : LeLive T' T) : Typ S Ψ Θ Γ (.var i) (.ex T φ) :=
  .var h hk hH (he ▸ .le hl)

/-- (Glob) at an instance the certificate writes out. -/
theorem typ_glob {g : Nat} {σ : Scheme} {ts : List Ty} {T' T : Ty} {φ : Stage}
    (h : S.glob g = some σ) (hk : ts.length = σ.k) (hH : σ.Holds Θ ts)
    (he : σ.inst ts = T') (hl : LeLive T' T) : Typ S Ψ Θ Γ (.glob g) (.ex T φ) :=
  .glob h hk hH (he ▸ hl)

/-- A scheme without facts holds at every instance. -/
theorem holds_none {k : Nat} {t : Ty} (ts : List Ty) : (Scheme.mk k Facts.none t).Holds Θ ts :=
  fun _ _ h => h.elim

end Flick.Cert
