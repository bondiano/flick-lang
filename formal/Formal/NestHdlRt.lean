import Formal.NestHdlPure

/-!
# The run-time steps of handlers keep `⊢ Λ`

A raised error or a signal of an arm unwinds the parked body to the handler; one of a
resumed body goes into the arm. The handler catches its signal, and what the signal
holds runs at the place of the handler.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

/-- What a handler gives back and raises names no region younger than the handler. -/
theorem HCell.res {p ℓ : Nat} {a : Ty} {r ε : Row} {t : Ty} {C : Caps} (h : HCell Ψ p ℓ)
    (hl : Ψ[ℓ]? = some (.arrow a (.rt r ε) t C)) : t.YF p ∧ (Ty.errors ε).YF p := by
  obtain ⟨t₀, ht₀, hy⟩ := h
  rw [hl] at ht₀; cases ht₀
  simp only [Ty.YF, Ty.substT, Stage.substT, Ty.arrow.injEq, Stage.rt.injEq] at hy
  exact ⟨hy.2.2.1, by simp [Ty.YF, Ty.substT, hy.2.1.2]⟩

theorem Nest.hresVal {c : Cfg} {home i E p ℓ w} {A : List Frame} {Θ : Facts} {T : Ty} {φ : Stage}
    (hn : Nest Ψ c anc B home) (hi : c.at i = some (plug E (.hres p ℓ (plug A (.hole p)) w)))
    (hE : OkE E) (hA : OkE A) (hw : Value w) (ht : Typ S Ψ Θ [] w (.ex T φ))
    (hT : ∀ x, T.reach (.r x) → x < p) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hdone p (plug A w)))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₄ := (hw.noIns hW.2.2.1).frame q
      have h₅ := plug_frame_iff (E := A) (x := w) (y := .hole p)
        (fun q => ⟨fun h => ((hw.noIns hW.2.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1.replug hole_not_value hA (old_move hn.le hW.1 hW.2.1 hw ht hT hW.2.2.1)
      fun x hx => ((hw.noIns hW.2.2.1).frame x hx).elim⟩

/-- A signal, or an error raised, from an arm: the parked body unwinds to the handler. -/
theorem Nest.hctlSig {c : Cfg} {home i E p ℓ κ tl Ef al rf x} {Ei : List Frame} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hctl p ℓ κ tl Ef al rf x (plug Ei (.hole p))))) (hE : OkE E)
    (hEi : OkE Ei) (hx : ∀ q, ¬ x.Has (.frame q)) (hv : ¬ Value (.hctl p ℓ κ tl Ef al rf x (plug Ei (.hole p)))) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ x))))) anc B home :=
  hn.redex' hi hE hv (fun hW q => by
      have h₂ := hW.2.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.2.1.frame q
      have h₅ := plug_frame_iff (E := Ei) (x := .unw p ℓ x) (y := .hole p)
        (fun q => ⟨fun h => by
          simp only [Tm.Has] at h
          rcases h with h | h | h
          · cases h
          · cases h
          · exact (hx _ h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₂ h₃ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.1,
      hW.2.2.2.2.1.replug hole_not_value hEi
        (unw_within hW.1 hW.2.2.2.2.2.2.2.2.1 hW.2.2.2.1 hx)
        (fun x' hx' => by
          simp only [Tm.Has] at hx'
          rcases hx' with h | h | h
          · cases h
          · cases h
          · exact (hx _ h).elim),
      hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩

theorem Nest.hwithCatch {c : Cfg} {home i E p ℓ Ef al rf a} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (.unw p ℓ a)))) (hE : OkE E) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E a)) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₁ := hW.2.2.2.1.2.2.1 q
      have h₂ := hW.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.1.frame q
      simp only [Tm.Has] at h₂ h₃ ⊢; grind)
    fun hW => hW.2.2.2.1.2.1.congrA fun x => (hW.2.2.2.2.2.2.1 x).symm

end Flick
