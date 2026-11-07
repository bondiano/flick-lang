import Formal.NestHdlStep

/-!
# The steps of a kept handler keep `⊢ Λ`

A kept handler is at the place of its child, deeper than its own: the regions of the place
older than it are its ancestors, and what its arms write is among those (`HLocs`). So its arms
move to the place of the child, the argument of an operation has a type without regions, and
what its arm gives back too. An error it raises names regions older than the handler, and goes
in a signal. A child and a finalizer keep the handlers older than their owner, whose ancestors
are ancestors of the owner.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

/-- The arms of a kept handler move to a place that agrees with its own on the regions as old
as it. -/
theorem Tm.Within.moveArm {al : Tm} {q : Nat} {A : Nat → Prop} (hal : al.Within Ψ B anc (anc q))
    (hn : al.NoIns) (hlo : HLocs Ψ q al) (hle : ∀ x, anc q x → x ≤ q)
    (hA : ∀ x, x ≤ q → (anc q x ↔ A x)) : al.Within Ψ B anc A :=
  hal.move hn (C := (· ≤ q)) (fun p hp => hle p (hal.rgn0 hn hp))
    (fun l t hl ht x hx => by
      obtain ⟨t', ht', hy⟩ := hlo l hl
      rw [ht] at ht'; cases ht'
      exact Nat.le_of_lt (hy.reach hx)) hA

/-- The place of the hole of a thread agrees with the ancestors of a region at it. -/
theorem Nest.hole_anc {c : Cfg} {home i E} {e : Tm} (hn : Nest Ψ c anc B home) (hi : c.at i = some e)
    (hE : WithinE Ψ B anc (baseOf anc i) E) {q : Nat} (hq : holeE (baseOf anc i) E q) :
    ∀ x, x ≤ q → (anc q x ↔ holeE (baseOf anc i) E x) := fun x hx => by
  rw [hn.hole_chain hi hE q hq]
  exact ⟨fun h => h.1, fun h => ⟨h, hx⟩⟩

theorem Nest.hkeepVal {c : Cfg} {home i E q ℓ Ef al v} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hkeep q ℓ Ef al v))) (hE : OkE E) (hv : Value v) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E v)) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q' => by
      have h₂ := hW.2.2.2.1.frame q'
      have h₄ := (hv.noIns hW.2.2.1).frame q'
      simp only [Tm.Has] at h₂ h₄ ⊢; grind)
    fun hW => hW.2.2.1

theorem Nest.hkarmVal {c : Cfg} {home i E q ℓ Ef al w} {Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hkarm q ℓ Ef al w (plug Ei (.hole q))))) (hE : OkE E)
    (hEi : OkE Ei) (hw : Value w) (ht : Typ S Ψ Θ [] w (.ex T φ)) (hT : T.YF 0) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hkeep q ℓ Ef al (plug Ei w)))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q' => by
      have h₂ := hW.2.2.2.2.1.frame q'
      have h₄ := (hw.noIns hW.2.2.1).frame q'
      have h₅ := plug_frame_iff (E := Ei) (x := w) (y := .hole q)
        (fun q => ⟨fun h => ((hw.noIns hW.2.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q'
      simp only [Tm.Has] at h₂ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.2.1.replug hole_not_value hEi (hW.2.2.1.moveC hw ht hT)
        (fun x hx => ((hw.noIns hW.2.2.1).frame x hx).elim),
      hW.2.2.2.2.1, hW.2.2.2.2.2.2⟩

/-- What the arm of a kept handler does when it leaves its body in a signal. -/
theorem Nest.hkarmSig {c : Cfg} {home i E q ℓ Ef al x y} {Ei : List Frame} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hkarm q ℓ Ef al x (plug Ei (.hole q))))) (hE : OkE E)
    (hEi : OkE Ei) (hv : ¬ Value (.hkarm q ℓ Ef al x (plug Ei (.hole q))))
    (hxy : ∀ q', ¬ y.Has (.frame q')) (hx : ∀ q', ¬ x.Has (.frame q'))
    (hy : x.Within Ψ B anc (holeE (baseOf anc i) E) → (∀ z, anc q z → holeE (baseOf anc i) E z) →
      holeE (baseOf anc i) E q → y.Within Ψ B anc (holeE (holeE (baseOf anc i) E) Ei)) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hkeep q ℓ Ef al (plug Ei y)))) anc B home :=
  hn.redex' hi hE hv (fun hW q' => by
      have h₂ := hW.2.2.2.2.1.frame q'
      have h₄ := hx q'
      have h₅ := plug_frame_iff (E := Ei) (x := y) (y := .hole q)
        (fun q => ⟨fun h => (hxy q h).elim, fun h => (hole_frames h).elim⟩) q'
      simp only [Tm.Has] at h₂ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.2.1.replug hole_not_value hEi (hy hW.2.2.1 hW.2.2.2.2.2.2.1 hW.1)
        (fun x' hx' => (hxy x' hx').elim),
      hW.2.2.2.2.1, hW.2.2.2.2.2.2⟩

theorem Nest.performKept {c : Cfg} {home i E q ℓ Ef al o b v} {Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hkeep q ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v)
    (ht : Typ S Ψ Θ [] v (.ex T φ)) (hT : T.YF 0) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hkarm q ℓ Ef al (b.subst (inst [v])) (plug Ei (.hole q))))) anc B home := by
  obtain ⟨hWE, hrW⟩ := within_of_plug (fun h => by cases h) hE (hn.thread i _ hi).1
  have hmv := hrW.2.1.moveArm hrW.2.2.2.1 hrW.2.2.2.2.2.2 (hn.le q) (hn.hole_anc hi hWE hrW.1)
  obtain ⟨hn₁, hW₁⟩ := arm_inst hmv hrW.2.2.2.1 ha hEi hrW.2.2.1 perform_not_value hv ht hT [v] (by simp)
  refine hn.redex' hi hE (fun h => by cases h) (fun hW q' => ?_) fun hW => ?_
  · have h₂ := hW.2.2.2.1.frame q'
    have h₄ := hn₁.frame q'
    have h₅ := plug_frame_iff (E := Ei) (x := .hole q) (y := .app (.const (.perform Ef o .tail)) v)
      (fun q => ⟨fun h => (hole_frames h).elim, fun h =>
        (perform_frames hv (within_of_plug perform_not_value hEi hW.2.2.1).2 q h).elim⟩) q'
    simp only [Tm.Has] at h₂ h₄ ⊢; grind
  · exact ⟨hW.1, hW.2.1, hW₁, hW.2.2.1.replug perform_not_value hEi (holeE_iff.mpr (.inl hW.1))
      (fun _ hx => (hole_frames hx).elim), hW.2.2.2.1, fun x hx => (hn₁.frame x hx).elim,
      hW.2.2.2.2.1, hW.2.2.2.2.2⟩

/-- A child or a finalizer in the handlers of its context older than its owner, at the place of
the owner: every region of the context as old as the owner is an ancestor of it. -/
theorem withs_within {base A : Nat → Prop} {p : Nat} {x : Tm} (hle : ∀ q y, anc q y → y ≤ q) :
    ∀ {E : List Frame}, WithinE Ψ B anc base E → (∀ y, holeE base E y → y ≤ p → A y) →
      x.Within Ψ B anc A → (∀ q, ¬ x.Has (.frame q)) →
      (withs p E x).Within Ψ B anc A ∧ ∀ q, ¬ (withs p E x).Has (.frame q)
  | [], _, _, hx, hf => ⟨hx, hf⟩
  | F :: E, ⟨hE, hF⟩, hup, hx, hf => by
    have up : ∀ y, holeE base E y → holeE base (F :: E) y := fun y hy =>
      holeE_iff.mpr ((holeE_iff.mp hy).imp id fun h => List.mem_cons_of_mem _ h)
    simp only [withs]
    refine withs_within hle hE (fun y hy => hup y (up y hy)) ?_ ?_
    · cases F with
      | hwith q ℓ E' al rf =>
        simp only [Frame.wrap]
        split
        · rename_i hqp
          obtain ⟨hq, hal, -, hn, -, hA', hC, hLo⟩ := hF
          exact ⟨hup q (up q hq) hqp, hal.congrA hA', hx, hn,
            fun y hy => hup y (up y ((hA' y).mpr hy)) (Nat.le_trans (hle q y hy) hqp), hC, hLo⟩
        · exact hx
      | hkeep q ℓ E' al =>
        simp only [Frame.wrap]
        split
        · rename_i hqp
          obtain ⟨hq, hal, hn, hs, hC, hLo⟩ := hF
          exact ⟨hup q (up q hq) hqp, hal, hx, hn,
            fun y hy => hup y (up y (hs y hy)) (Nat.le_trans (hle q y hy) hqp), hC, hLo⟩
        · exact hx
      | _ => exact hx
    · intro q'
      cases F with
      | hwith q ℓ E' al rf =>
        simp only [Frame.wrap]
        split
        · rintro (h | h | h | h)
          · cases h
          · cases h
          · exact absurd (Tm.has_frame_ins h) hF.2.2.2.1
          · exact hf q' h
        · exact hf q'
      | hkeep q ℓ E' al =>
        simp only [Frame.wrap]
        split
        · rintro (h | h | h | h)
          · cases h
          · cases h
          · exact absurd (Tm.has_frame_ins h) hF.2.2.1
          · exact hf q' h
        · exact hf q'
      | _ => exact hf q'

end Flick
