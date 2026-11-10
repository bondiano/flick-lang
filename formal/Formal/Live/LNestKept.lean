import Formal.Live.LNestHdlStep

/-!
# The steps of a kept handler keep `⊢ Λ` of run-time forms

A kept handler is at the place of its child, deeper than its own; its arm runs beside the
body it parks, and a signal of the arm goes to the handler of the child's place.
-/

namespace Flick

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem LNest.hkeepVal {c : Cfg} {home i E q ℓ Ef al v} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hkeep q ℓ Ef al v))) (hE : OkE E) (hv : Value v) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E v)) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q' => by
      have h₂ := hW.2.2.2.1.frame q'
      have h₄ := (hv.noInsF hW.2.2.1).frame q'
      simp only [Tm.Has] at h₂ h₄ ⊢; grind)
    fun hW => hW.2.2.1

theorem LNest.hkarmVal {c : Cfg} {home i E q ℓ Ef al w} {Ei : List Frame} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hkarm q ℓ Ef al w (plug Ei (.hole q))))) (hE : OkE E)
    (hEi : OkE Ei) (hw : Value w) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hkeep q ℓ Ef al (plug Ei w)))) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q' => by
      have h₂ := hW.2.2.2.2.1.frame q'
      have h₄ := (hw.noInsF hW.2.2.1).frame q'
      have h₅ := plug_frame_iff (E := Ei) (x := w) (y := .hole q)
        (fun q => ⟨fun h => ((hw.noInsF hW.2.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q'
      simp only [Tm.Has] at h₂ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.2.1.replug hole_not_value hEi (Tm.Fw.of_noIns (hw.noInsF hW.2.2.1))
        (fun x hx => ((hw.noInsF hW.2.2.1).frame x hx).elim),
      hW.2.2.2.2.1, hW.2.2.2.2.2.2⟩

/-- What the arm of a kept handler does when it leaves its body in a signal. -/
theorem LNest.hkarmSig {c : Cfg} {home i E q ℓ Ef al x y} {Ei : List Frame} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hkarm q ℓ Ef al x (plug Ei (.hole q))))) (hE : OkE E)
    (hEi : OkE Ei) (hv : ¬ Value (.hkarm q ℓ Ef al x (plug Ei (.hole q))))
    (hxy : ∀ q', ¬ y.Has (.frame q')) (hx : ∀ q', ¬ x.Has (.frame q'))
    (hy : x.Fw Ψ anc (holeE (baseOf anc i) E) → (∀ z, anc q z → holeE (baseOf anc i) E z) →
      holeE (baseOf anc i) E q → y.Fw Ψ anc (holeE (holeE (baseOf anc i) E) Ei)) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hkeep q ℓ Ef al (plug Ei y)))) anc home :=
  hn.redex' hi hE hv (fun hW q' => by
      have h₂ := hW.2.2.2.2.1.frame q'
      have h₄ := hx q'
      have h₅ := plug_frame_iff (E := Ei) (x := y) (y := .hole q)
        (fun q => ⟨fun h => (hxy q h).elim, fun h => (hole_frames h).elim⟩) q'
      simp only [Tm.Has] at h₂ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.2.1.replug hole_not_value hEi (hy hW.2.2.1 hW.2.2.2.2.2.2.1 hW.1)
        (fun x' hx' => (hxy x' hx').elim),
      hW.2.2.2.2.1, hW.2.2.2.2.2.2⟩

theorem LNest.performKept {c : Cfg} {home i E q ℓ Ef al o b v} {Ei : List Frame}
    (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hkeep q ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hkarm q ℓ Ef al (b.subst (inst [v])) (plug Ei (.hole q))))) anc home := by
  refine hn.redex' hi hE (fun h => by cases h) (fun hW q' => ?_) fun hW => ?_
  · have hn₁ := arm_noIns (δ := [v]) hW.2.2.2.1 ha (by simpa using perform_noIns hEi hv hW.2.2.1)
    have h₂ := hW.2.2.2.1.frame q'
    have h₄ := hn₁.frame q'
    have h₅ := plug_frame_iff (E := Ei) (x := .hole q) (y := .app (.const (.perform Ef o .tail)) v)
      (fun q => ⟨fun h => (hole_frames h).elim, fun h =>
        (perform_framesF hv (fw_of_plug perform_not_value hEi hW.2.2.1).2 q h).elim⟩) q'
    simp only [Tm.Has] at h₂ h₄ ⊢; grind
  · have hn₁ := arm_noIns (δ := [v]) hW.2.2.2.1 ha (by simpa using perform_noIns hEi hv hW.2.2.1)
    exact ⟨hW.1, hW.2.1, Tm.Fw.of_noIns hn₁, hW.2.2.1.replug perform_not_value hEi
      (holeE_iff.mpr (.inl hW.1)) (fun _ hx => (hole_frames hx).elim), hW.2.2.2.1,
      fun x hx => (hn₁.frame x hx).elim, hW.2.2.2.2.1, hW.2.2.2.2.2⟩

end Flick
