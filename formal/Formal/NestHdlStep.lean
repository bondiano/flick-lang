import Formal.NestHdlRt

/-!
# The steps of handlers in a context keep `⊢ Λ`

An operation runs its arm at the handler, the argument of a type without regions; the
arm resumes the body under the handler, and waits beside it; a new handler is a new
region around its body, with a new cell.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

/-- A signal of a resumed body goes into the arm, past its `inside` forms. -/
theorem Nest.hresSig {c : Cfg} {home i E p ℓ x} {A : List Frame} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hres p ℓ (plug A (.hole p)) x))) (hE : OkE E) (hA : OkE A)
    (hx : ∀ q, ¬ x.Has (.frame q)) (hv : ¬ Value (.hres p ℓ (plug A (.hole p)) x))
    (hmv : ∀ {A₀ : Nat → Prop}, A₀ p → (plug A (.hole p)).Within Ψ B anc A₀ → x.Within Ψ B anc A₀ →
      x.Within Ψ B anc (holeE A₀ A)) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hdone p (plug A x)))) anc B home :=
  hn.redex' hi hE hv (fun hW q => by
      have h₅ := plug_frame_iff (E := A) (x := x) (y := .hole p)
        (fun q => ⟨fun h => (hx q h).elim, fun h => (hole_frames h).elim⟩) q
      have h₄ := hx q
      simp only [Tm.Has] at ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1.replug hole_not_value hA (hmv hW.1 hW.2.1 hW.2.2.1)
      fun x' hx' => (hx x' hx').elim⟩

/-- A signal holds no region the place of the hole does not. -/
theorem unw_inward {q ℓ : Nat} {b : Tm} {A₀ : Nat → Prop} {A : List Frame}
    (h : (Tm.unw q ℓ b).Within Ψ B anc A₀) : (Tm.unw q ℓ b).Within Ψ B anc (holeE A₀ A) :=
  ⟨holeE_iff.mpr (.inl h.1), h.2.1, h.2.2.1, fun x hx => holeE_iff.mpr (.inl (h.2.2.2 x hx))⟩

end Flick
