import Formal.Elab.Coherence

/-!
# `CallSite`: (Site) is (Prim) at the use's own place (elaboration.md §2.3)

`CallSite` reads as `CallSite ς`, with an unwritten variable ς for the place. A given of the
caller fixes ς; without one the checker defaults ς to the literal ℓ of the use, as it
defaults a numeric literal, and `CallSite ℓ` is solved by (Prim): `prims (CallSite ℓ) = ℓ̄`.
A place that is still a variable is no primitive (`prims = none`), so:

- `site_given`: under a given, the only derivation of `CallSite ς` is the given itself;
- `site_caller`: a function constrained by it passes on what its caller gave, the closed
  dictionary of the caller's own place.

`Ent`, `Eqv`, `Coherent` and T6 are unchanged.
-/

namespace Flick.Elab

variable {P : Prog} {lt : Cn → Cn → Prop}

/-- A primitive class at a place that is no literal is derived only from a given. -/
theorem site_given (h : Coherent P lt) {Δ : List Cn} {Q : Cn} {d : Dict}
    (hc : P.primC Q.cls = true) (hp : P.prims Q = none) (he : Ent P Δ [Q] [d]) :
    Clo P Δ d Q := by
  rcases Ent.single he with hg | ⟨p, _, hp', _⟩ | ⟨C, τ, ts, ds, hq, hs, _, _, _⟩ |
      ⟨I, ins, σ, ds, hI, hcls, _, _, _⟩
  · exact hg
  · rw [hp] at hp'; cases hp'
  · subst hq
    have hn : P.structC C = false := h.primStruct (C := C) hc
    rw [hn] at hs; cases hs
  · exact (h.k6prim hc hI hcls).elim

/-- A function under the given `CallSite ς` passes on, once called at the closed place
`ς := ℓ` with the caller's dictionary `d₀`, a dictionary `≈ d₀`: its caller's place. -/
theorem site_caller (h : Coherent P lt) {Q : Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    {d d₀ : Dict} (hρ : Closes P [Q] θ ρ)
    (ho : ∀ d Q', Clo P [Q] d Q' → SlotOK P (Q'.subst θ))
    (he : Ent P [Q] [Q] [d]) (h₀ : Ent P [] [Q.subst θ] [d₀]) :
    Eqv P (d.subst ρ) d₀ := by
  obtain ⟨ds', hc, ha⟩ := closed_rep h hρ ho he
  have := closed_unique h hc h₀
  subst this
  obtain ⟨b, hb, r⟩ := All2.single ha
  cases hb
  exact r

end Flick.Elab
