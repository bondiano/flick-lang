import Formal.Rules

/-!
# The roots and the facts of a running configuration

`Σ₀` of core.md §2.6 with the bodies of the globals of λFlick⁺, its typing `RootTy`, the
order of creation of the regions of a run (`Facts.age`), and (Ctor) at an ordinary
constructor.
-/

namespace Flick

variable (S : Sig)

/-- The facts of a running configuration: a region outlives every region created
after it. A scope gives its region facts about everything outside it, which
exists before the region; with the regions not yet created renamed out, they hold
of this order (regions.md §3.4). -/
def Facts.age : Facts := fun C s => ∀ q p, C (.r q) → s.reach (.r p) → q < p

/-- The root environment `Σ₀` of core.md §2.6 and the bodies of the globals of λFlick⁺. -/
structure Roots where
  svc : Name → Option Tm
  glob : Nat → Option Tm

/-- `Σ₀` has closed values of `S.Impl`. The body of a global is closed source code,
typed at the scheme of the global; each declared global has a body. -/
structure RootTy (root : Roots) : Prop where
  svc : ∀ N v, root.svc N = some v → Value v ∧ Typ S [] Facts.none [] v (.ex (S.impl N) .pure)
  glob : ∀ g v, root.glob g = some v → (∀ x, ¬ v.Has x) ∧
    ∃ s, S.glob g = some s ∧ Typ S [] s.cs [] v (.ex s.t .pure)
  globs : ∀ g s, S.glob g = some s → (root.glob g).isSome
  /-- A root is source code: it names no run-time region. -/
  svcBelow : ∀ N v, root.svc N = some v → ∀ p, ¬ v.Has (.rgn p)

/-- An ordinary constructor at `T us`. -/
theorem Typ.ctor_plain {Ψ Θ Γ K k ts T₀ us T φ} (hK : S.ctor K = some (k, ts, T₀))
    (hres : S.res K = idTs k) (hk : us.length = k) (hT : LeLive S (Sig.ctorTy ts T₀ us) T) :
    Typ S Ψ Θ Γ (.ctor K) (.ex T φ) :=
  .ctor hK hk (S.holds_plain hK hres hk) (by rw [Sig.ctorTyR_plain hres hk]; exact hT)

end Flick
