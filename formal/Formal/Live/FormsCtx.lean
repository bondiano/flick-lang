import Formal.Live.Forms

/-!
# The run-time forms of a context

A term in a context and the rest of the context share no `inside` form; the
regions of the hole of a context along a thread are a chain of ancestors; the
handlers of a context are at their regions and have their cells.
-/

namespace Flick

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem Tm.Fw.congrA {e : Tm} {A A' : Nat → Prop} (h : e.Fw Ψ anc A) (hA : ∀ x, A x ↔ A' x) :
    e.Fw Ψ anc A' := by
  have : A = A' := funext fun x => propext (hA x)
  exact this ▸ h

/-- A region of a term in a frame is no region of the rest of the frame. -/
theorem Frame.disjF {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).Fw Ψ anc A) {x : Nat} (hx : r.Has (.frame x)) :
    ¬ (F.plug (.var 0)).Has (.frame x) := by
  have hFW := (Frame.fw_of_plug hF hv h).1
  intro h'
  cases F with
  | inside p =>
    simp only [Frame.plug, Tm.Has, Tm.Fw] at h h'
    rcases h' with h' | h' | h' | h'
    · cases h'
    · cases h'
    · cases h'; exact (h.2.2.frame hx).1 (.inr rfl)
    · exact h'
  | _ =>
    simp only [Frame.plug, Tm.Has, Tm.Fw, Frame.Fw] at h h' hFW <;>
      first
      | grind [Tm.has_frame_ins]
      | (rcases h' with h' | h'
         · exact absurd (Tm.has_frame_ins h') (hFW.2.noInsF hFW.1)
         · exact h'.elim)
      | grind [Tm.has_frame_ins, Tm.NoIns]

/-- A region of a term in a context is no region of the rest of the context. -/
theorem fw_disj {A : Nat → Prop} {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).Fw Ψ anc A → ∀ {x}, r.Has (.frame x) →
      ¬ (plug E (.var 0)).Has (.frame x)
  | [], _, _, _, _, h => h
  | F :: E, hE, h, x, hx, h' => by
    obtain ⟨hF, hE'⟩ := hE.cons
    have hW := (fw_of_plug (r := F.plug r) (not_value_plug hv F) hE' h).2
    rcases plug_has_inv (r := .var 0) E h' with h' | h'
    · exact fw_disj (not_value_plug hv F) hE' h (F.plug_has hx) h'
    · exact Frame.disjF hF hv hW hx h'

/-- A term in a context replaced by one at the place of its hole whose regions are
the old term's or new. -/
theorem Tm.Fw.replug {A : Nat → Prop} {r₀ r : Tm} (hv : ¬ Value r₀) :
    ∀ {E : List Frame}, OkE E → (plug E r₀).Fw Ψ anc A → r.Fw Ψ anc (holeE A E) →
      (∀ x, r.Has (.frame x) → r₀.Has (.frame x) ∨ ¬ (plug E r₀).Has (.frame x)) →
      (plug E r).Fw Ψ anc A
  | [], _, _, hr, _ => hr
  | F :: E, hE, h, hr, hd => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨-, hW⟩ := fw_of_plug (r := F.plug r₀) (not_value_plug hv F) hE' h
    have hFW := (Frame.fw_of_plug hF hv hW).1
    refine Tm.Fw.replug (E := E) (r₀ := F.plug r₀) (not_value_plug hv F) hE' h
      (hFW.plug hr fun x hx h' => ?_) fun x hx => ?_
    · rcases hd x hx with hx | hx
      · exact Frame.disjF hF hv hW hx h'
      · exact hx (plug_has E (F.has_of_hole h'))
    · rcases F.plug_has_inv (r := .var 0) hx with hx | hx
      · exact .inl (F.has_of_hole hx)
      · exact (hd x hx).imp_left F.plug_has

/-- An `inside` form of `q` in a context: `q` is new at its place, and its ancestors
are regions of the hole or of the context. -/
theorem FwE.frameAnc {A : Nat → Prop} {q : Nat} :
    ∀ {E : List Frame}, FwE Ψ anc A E → (plug E (.var 0)).Has (.frame q) →
      ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧
        ∀ x, anc q x → holeE A E x ∨ (plug E (.var 0)).Has (.frame x)
  | [], _, h => by simp [plug, Tm.Has] at h
  | F :: E, ⟨hE, hF⟩, h => by
    have up : ∀ x, holeE A E x → holeE A (F :: E) x := fun x hx => by
      cases F <;> simp [holeE, Frame.hole, hx]
    have upA : ∀ x, A x → holeE A E x := fun x hx => holeE_iff.mpr (.inl hx)
    have part : ∀ {t : Tm}, t.Fw Ψ anc (holeE A E) → t.Has (.frame q) →
        (∀ x, t.Has (.frame x) → (F.plug (.var 0)).Has (.frame x)) →
        ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧
          ∀ x, anc q x → holeE A (F :: E) x ∨ (plug (F :: E) (.var 0)).Has (.frame x) := by
      intro t ht hq hs
      obtain ⟨h₁, h₂, h₃, h₄⟩ := ht.frame hq
      refine ⟨fun h' => h₁ (upA _ h'), h₂, fun x hx => h₃ x (upA x hx), fun x hx => ?_⟩
      exact (h₄ x hx).imp (up x) fun h => plug_has E (hs x h)
    rcases plug_has_inv (r := .var 0) E h with h | h
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := FwE.frameAnc hE h
      exact ⟨h₁, h₂, h₃, fun x hx => (h₄ x hx).imp (up x) (plug_of_hole (r := F.plug (.var 0)))⟩
    cases F with
    | inside p =>
      simp only [Frame.plug, Tm.Has, Frame.Fw] at h hF
      rcases h with h | h | h | h
      · cases h
      · cases h
      · cases h
        refine ⟨fun h' => hF.1 (upA _ h'), by rw [hF.2]; simp, fun x hx => ?_, ?_⟩
        · rw [hF.2]; exact .inl (upA x hx)
        · intro x hx; rw [hF.2] at hx; exact .inl (by simp [holeE, Frame.hole, hx])
      · simp at h
    | hctlA p ℓ κ tl E' al r k =>
      simp only [Frame.Fw] at hF
      refine part hF.2.2.2.1 ?_ fun x hx => by simp [Frame.plug, Tm.Has, hx]
      simp only [Frame.plug, Tm.Has] at h
      grind [Tm.has_frame_ins, Tm.NoIns]
    | hresB p ℓ a =>
      simp only [Frame.Fw] at hF
      refine part hF.2.1 ?_ fun x hx => by simp [Frame.plug, Tm.Has, hx]
      simp only [Frame.plug, Tm.Has] at h
      grind
    | hkeep p ℓ E' al =>
      simp only [Frame.plug, Tm.Has, Frame.Fw] at h hF
      rcases h with h | h | h | h
      · cases h
      · cases h
      · exact absurd (Tm.has_frame_ins h) hF.2.2.1
      · exact h.elim
    | hkarmA p ℓ E' al k =>
      simp only [Frame.Fw] at hF
      refine part hF.2.2.1 ?_ fun x hx => by simp [Frame.plug, Tm.Has, hx]
      simp only [Frame.plug, Tm.Has] at h
      grind [Tm.has_frame_ins, Tm.NoIns]
    | _ =>
      simp only [Frame.plug, Tm.Has, Frame.Fw] at h hF <;>
        first
        | grind [Tm.has_frame_ins, Tm.NoIns]
        | (rcases h with h | h
           · exact absurd (Tm.has_frame_ins h) (hF.2.noInsF hF.1)
           · exact h.elim)

theorem FwE.chain {A : Nat → Prop} (hle : ∀ q x, anc q x → x ≤ q) :
    ∀ {E : List Frame}, FwE Ψ anc A E → Chain anc A → Chain anc (holeE A E)
  | [], _, h => h
  | F :: E, ⟨hE, hF⟩, h => by
    have h' := FwE.chain hle hE h
    cases F <;> simp only [holeE, Frame.hole] <;> try exact h'
    exact h'.inside hF.1 hF.2 (hle _)

/-- The region of a run-time form of a handler, or of an `inside` form, in a frame. -/
def Frame.reg : Frame → Option Nat
  | .inside p | .hwith p .. | .hctlA p .. | .hresB p .. | .hdone p | .hkeep p .. | .hkarmA p .. => some p
  | _ => none

theorem holeE_cons {A : Nat → Prop} {F : Frame} {E : List Frame} {x : Nat} (h : holeE A E x) :
    holeE A (F :: E) x :=
  holeE_iff.mpr ((holeE_iff.mp h).imp id fun h => List.mem_cons_of_mem _ h)

end Flick
