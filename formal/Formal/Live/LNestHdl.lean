import Formal.Live.LNestClose

/-!
# The steps of handlers keep `⊢ Λ` of run-time forms

A step of a handler replaces its redex by one with the same `inside` forms: a parked
body goes back under its handler, an arm waits beside the resumed body, and a
signal goes to the place of its handler. What goes into a body or an arm is a value,
which holds no run-time form and is at any place.
-/

namespace Flick

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem LNest.hwithVal {c : Cfg} {home i E p ℓ Ef al rf v} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf v))) (hE : OkE E) (hv : Value v) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.app rf v))) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have := hW.2.2.2.2.1.frame q
      simp only [Tm.Has] at this ⊢; grind)
    fun hW => ⟨hW.2.2.1, hW.2.2.2.1, .inl (hv.noInsF hW.2.2.2.1)⟩

theorem LNest.hctlTail {c : Cfg} {home i E p ℓ κ Ef al rf w} {Ei : List Frame} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hctl p ℓ κ true Ef al rf w (plug Ei (.hole p))))) (hE : OkE E)
    (hEi : OkE Ei) (hw : Value w) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hwith p ℓ Ef al rf (plug Ei w)))) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₂ := hW.2.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.2.1.frame q
      have h₄ := (hw.noInsF hW.2.2.2.1).frame q
      have h₅ := plug_frame_iff (E := Ei) (x := w) (y := .hole p)
        (fun q => ⟨fun h => ((hw.noInsF hW.2.2.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₂ h₃ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.1,
      hW.2.2.2.2.1.replug hole_not_value hEi (Tm.Fw.of_noIns (hw.noInsF hW.2.2.2.1))
        (fun x hx => ((hw.noInsF hW.2.2.2.1).frame x hx).elim),
      hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩

/-- A signal to the handler `p` from the hole of a body parked at a place of `p`. -/
theorem unw_fw {p ℓ : Nat} {a : Tm} {A : Nat → Prop} {Ei : List Frame} (hp : A p)
    (hA : ∀ x, A x ↔ anc p x) (ha : a.Fw Ψ anc A) (hf : ∀ q, ¬ a.Has (.frame q)) :
    (Tm.unw p ℓ a).Fw Ψ anc (holeE A Ei) :=
  ⟨holeE_iff.mpr (.inl hp), ha.congrA hA, hf, fun x hx => holeE_iff.mpr (.inl ((hA x).mpr hx))⟩

theorem LNest.hctlDisc {c : Cfg} {home i E p ℓ κ Ef al rf w} {Ei : List Frame} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hctl p ℓ κ false Ef al rf w (plug Ei (.hole p))))) (hE : OkE E)
    (hEi : OkE Ei) (hw : Value w) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ w))))) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₂ := hW.2.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.2.1.frame q
      have h₄ := (hw.noInsF hW.2.2.2.1).frame q
      have h₅ := plug_frame_iff (E := Ei) (x := .unw p ℓ w) (y := .hole p)
        (fun q => ⟨fun h => by
          simp only [Tm.Has] at h
          rcases h with h | h | h
          · cases h
          · cases h
          · exact ((hw.noInsF hW.2.2.2.1).frame _ h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₂ h₃ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.1,
      hW.2.2.2.2.1.replug hole_not_value hEi
        (unw_fw hW.1 hW.2.2.2.2.2.2.2.2.1 hW.2.2.2.1 ((hw.noInsF hW.2.2.2.1).frame))
        (fun x hx => by simp only [Tm.Has] at hx; grind [(hw.noInsF hW.2.2.2.1).frame]),
      hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩

theorem LNest.hdoneVal {c : Cfg} {home i E p v} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hdone p v))) (hE : OkE E) (hv : Value v) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E v)) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₄ := (hv.noInsF hW.2).frame q
      simp only [Tm.Has] at h₄ ⊢; grind)
    fun hW => hW.2

theorem LNest.hresVal {c : Cfg} {home i E p ℓ w} {A : List Frame}
    (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E (.hres p ℓ (plug A (.hole p)) w)))
    (hE : OkE E) (hA : OkE A) (hw : Value w) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hdone p (plug A w)))) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₄ := (hw.noInsF hW.2.2.1).frame q
      have h₅ := plug_frame_iff (E := A) (x := w) (y := .hole p)
        (fun q => ⟨fun h => ((hw.noInsF hW.2.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1.replug hole_not_value hA (Tm.Fw.of_noIns (hw.noInsF hW.2.2.1))
      fun x hx => ((hw.noInsF hW.2.2.1).frame x hx).elim⟩

/-- A signal, or an error raised, from an arm: the parked body unwinds to the handler. -/
theorem LNest.hctlSig {c : Cfg} {home i E p ℓ κ tl Ef al rf x} {Ei : List Frame} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hctl p ℓ κ tl Ef al rf x (plug Ei (.hole p))))) (hE : OkE E)
    (hEi : OkE Ei) (hx : ∀ q, ¬ x.Has (.frame q)) (hv : ¬ Value (.hctl p ℓ κ tl Ef al rf x (plug Ei (.hole p)))) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ x))))) anc home :=
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
        (unw_fw hW.1 hW.2.2.2.2.2.2.2.2.1 hW.2.2.2.1 hx)
        (fun x' hx' => by
          simp only [Tm.Has] at hx'
          rcases hx' with h | h | h
          · cases h
          · cases h
          · exact (hx _ h).elim),
      hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩

theorem LNest.hwithCatch {c : Cfg} {home i E p ℓ Ef al rf a} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (.unw p ℓ a)))) (hE : OkE E) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E a)) anc home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₁ := hW.2.2.2.1.2.2.1 q
      have h₂ := hW.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.1.frame q
      simp only [Tm.Has] at h₂ h₃ ⊢; grind)
    fun hW => hW.2.2.2.1.2.1.congrA fun x => (hW.2.2.2.2.2.2.1 x).symm

/-- A signal of a resumed body goes into the arm, past its `inside` forms. -/
theorem LNest.hresSig {c : Cfg} {home i E p ℓ x} {A : List Frame} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hres p ℓ (plug A (.hole p)) x))) (hE : OkE E) (hA : OkE A)
    (hx : ∀ q, ¬ x.Has (.frame q)) (hv : ¬ Value (.hres p ℓ (plug A (.hole p)) x))
    (hmv : ∀ {A₀ : Nat → Prop}, A₀ p → x.Fw Ψ anc A₀ → x.Fw Ψ anc (holeE A₀ A)) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hdone p (plug A x)))) anc home :=
  hn.redex' hi hE hv (fun hW q => by
      have h₅ := plug_frame_iff (E := A) (x := x) (y := .hole p)
        (fun q => ⟨fun h => (hx q h).elim, fun h => (hole_frames h).elim⟩) q
      have h₄ := hx q
      simp only [Tm.Has] at ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1.replug hole_not_value hA (hmv hW.1 hW.2.2.1)
      fun x' hx' => (hx x' hx').elim⟩

/-- A signal holds no region the place of the hole does not. -/
theorem unw_inwardF {q ℓ : Nat} {b : Tm} {A₀ : Nat → Prop} {A : List Frame}
    (h : (Tm.unw q ℓ b).Fw Ψ anc A₀) : (Tm.unw q ℓ b).Fw Ψ anc (holeE A₀ A) :=
  ⟨holeE_iff.mpr (.inl h.1), h.2.1, h.2.2.1, fun x hx => holeE_iff.mpr (.inl (h.2.2.2 x hx))⟩

theorem unw_noFrameF {p ℓ : Nat} {a : Tm} {A : Nat → Prop} (h : (Tm.unw p ℓ a).Fw Ψ anc A) :
    ∀ q, ¬ (Tm.unw p ℓ a).Has (.frame q) := fun q hq => by
  simp only [Tm.Has] at hq
  rcases hq with hq | hq | hq
  · cases hq
  · cases hq
  · exact h.2.2.1 q hq

end Flick
