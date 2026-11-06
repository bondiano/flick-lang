import Formal.NestHdl

/-!
# The pure steps of handlers keep `⊢ Λ`

The value of a body leaves its handler; the value of an arm goes back into the parked
body, or the parked body unwinds to its handler; the value of a resumed body goes
into the arm, past the `inside` forms the arm opened, which are younger than what the
handler gives back.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem plug_frame_iff {E : List Frame} {x y : Tm} (h : ∀ q, x.Has (.frame q) ↔ y.Has (.frame q)) (q : Nat) :
    (plug E x).Has (.frame q) ↔ (plug E y).Has (.frame q) := by
  rw [plug_has_iff, plug_has_iff (x := y), h]

theorem hole_frames {p q : Nat} : ¬ (Tm.hole p).Has (.frame q) := by simp [Tm.Has]

theorem Nest.hwithVal {c : Cfg} {home i E p ℓ Ef al rf v} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf v))) (hE : OkE E) (hv : Value v) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.app rf v))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have := hW.2.2.2.2.1.frame q
      simp only [Tm.Has] at this ⊢; grind)
    fun hW => ⟨hW.2.2.1, hW.2.2.2.1, .inl (hv.noIns hW.2.2.2.1)⟩

theorem Nest.hctlTail {c : Cfg} {home i E p ℓ κ Ef al rf w} {Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hctl p ℓ κ true Ef al rf w (plug Ei (.hole p))))) (hE : OkE E)
    (hEi : OkE Ei) (hw : Value w) (ht : Typ S Ψ Θ [] w (.ex T φ)) (hT : T.YF 0) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hwith p ℓ Ef al rf (plug Ei w)))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₁ := hW.2.1
      have h₂ := hW.2.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.2.1.frame q
      have h₄ := (hw.noIns hW.2.2.2.1).frame q
      have h₅ := plug_frame_iff (E := Ei) (x := w) (y := .hole p)
        (fun q => ⟨fun h => ((hw.noIns hW.2.2.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₂ h₃ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.1,
      hW.2.2.2.2.1.replug hole_not_value hEi (hW.2.2.2.1.moveC hw ht hT)
        (fun x hx => ((hw.noIns hW.2.2.2.1).frame x hx).elim),
      hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩

/-- A signal to the handler `p` from the hole of a body parked at a place of `p`. -/
theorem unw_within {p ℓ : Nat} {a : Tm} {A : Nat → Prop} {Ei : List Frame} (hp : A p)
    (hA : ∀ x, A x ↔ anc p x) (ha : a.Within Ψ B anc A) (hf : ∀ q, ¬ a.Has (.frame q)) :
    (Tm.unw p ℓ a).Within Ψ B anc (holeE A Ei) :=
  ⟨holeE_iff.mpr (.inl hp), ha.congrA hA, hf, fun x hx => holeE_iff.mpr (.inl ((hA x).mpr hx))⟩

theorem Nest.hctlDisc {c : Cfg} {home i E p ℓ κ Ef al rf w} {Ei : List Frame} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hctl p ℓ κ false Ef al rf w (plug Ei (.hole p))))) (hE : OkE E)
    (hEi : OkE Ei) (hw : Value w) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ w))))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₂ := hW.2.2.2.2.2.1.frame q
      have h₃ := hW.2.2.2.2.2.2.1.frame q
      have h₄ := (hw.noIns hW.2.2.2.1).frame q
      have h₅ := plug_frame_iff (E := Ei) (x := .unw p ℓ w) (y := .hole p)
        (fun q => ⟨fun h => by
          simp only [Tm.Has] at h
          rcases h with h | h | h
          · cases h
          · cases h
          · exact ((hw.noIns hW.2.2.2.1).frame _ h).elim, fun h => (hole_frames h).elim⟩) q
      simp only [Tm.Has] at h₂ h₃ h₄ ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1, hW.2.2.1,
      hW.2.2.2.2.1.replug hole_not_value hEi
        (unw_within hW.1 hW.2.2.2.2.2.2.2.2.1 hW.2.2.2.1 ((hw.noIns hW.2.2.2.1).frame))
        (fun x hx => by simp only [Tm.Has] at hx; grind [(hw.noIns hW.2.2.2.1).frame]),
      hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩

/-- An `inside` form of a term at a place of `p` is younger than `p`. -/
theorem frame_young {e : Tm} {A : Nat → Prop} {p x : Nat} (hle : ∀ q x, anc q x → x ≤ q)
    (h : e.Within Ψ B anc A) (hp : A p) (hx : e.Has (.frame x)) : p < x := by
  obtain ⟨h₁, -, h₃, -⟩ := h.frame hx
  have := hle x p (h₃ p hp)
  rcases Nat.lt_or_eq_of_le this with h' | rfl
  · exact h'
  · exact (h₁ hp).elim

/-- What the handler `p` gives back, of a type older than `p`, goes past the `inside` forms
an arm opened at the place of `p`. -/
theorem old_move {Θ : Facts} {p : Nat} {w : Tm} {T : Ty} {φ : Stage} {A : Nat → Prop} {A' : List Frame}
    (hle : ∀ q x, anc q x → x ≤ q) (hp : A p) (ha : (plug A' (.hole p)).Within Ψ B anc A)
    (hw : Value w) (ht : Typ S Ψ Θ [] w (.ex T φ)) (hT : ∀ x, T.reach (.r x) → x < p)
    (hW : w.Within Ψ B anc A) : w.Within Ψ B anc (holeE A A') :=
  hW.moveT hw ht fun x hx => ⟨fun h => holeE_iff.mpr (.inl h), fun h => by
    rcases holeE_iff.mp h with h | h
    · exact h
    · exact absurd (hT x hx) (Nat.not_lt_of_le (Nat.le_of_lt
        (frame_young hle ha hp (plug_frame h))))⟩

theorem Nest.hdoneVal {c : Cfg} {home i E p v} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hdone p v))) (hE : OkE E) (hv : Value v) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E v)) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      have h₄ := (hv.noIns hW.2).frame q
      simp only [Tm.Has] at h₄ ⊢; grind)
    fun hW => hW.2

end Flick
