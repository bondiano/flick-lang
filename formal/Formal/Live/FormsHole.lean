import Formal.Live.FormsCtx

/-!
# What the frames of a context give its hole, whatever is plugged in it

The `inside` forms of a context are new at their places, its handlers are at their
regions and have their cells; unlike `FwE`, this holds also around a value.
-/

namespace Flick

section
variable (Ψ : List Ty) (anc : Nat → Nat → Prop)

/-- What a frame at a place with the regions `A` gives the regions of its hole. -/
def Frame.Fh (A : Nat → Prop) : Frame → Prop
  | .inside p => ¬ A p ∧ anc p = fun x => A x ∨ x = p
  | .hwith p ℓ .. | .hctlA p ℓ .. | .hresB p ℓ _ | .hkeep p ℓ .. | .hkarmA p ℓ .. => A p ∧ LCell Ψ anc p ℓ
  | .hdone p => A p
  | _ => True

def FhE (A : Nat → Prop) : List Frame → Prop
  | [] => True
  | F :: E => FhE A E ∧ F.Fh Ψ anc (holeE A E)

end

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem Frame.fh_of_plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.ok)
    (h : (F.plug r).Fw Ψ anc A) : F.Fh Ψ anc A ∧ r.Fw Ψ anc (F.hole A) := by
  cases F <;> simp only [Frame.Fh, Frame.hole, Frame.plug, Frame.ok, Tm.Fw] at hF h ⊢ <;> grind

theorem fh_of_plug {A : Nat → Prop} {r : Tm} :
    ∀ {E : List Frame}, OkE E → (plug E r).Fw Ψ anc A → FhE Ψ anc A E ∧ r.Fw Ψ anc (holeE A E)
  | [], _, h => ⟨trivial, h⟩
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨h₁, h₂⟩ := fh_of_plug (r := F.plug r) hE' h
    obtain ⟨h₃, h₄⟩ := Frame.fh_of_plug hF h₂
    exact ⟨⟨h₁, h₃⟩, h₄⟩

theorem FhE.chain {A : Nat → Prop} (hle : ∀ q x, anc q x → x ≤ q) :
    ∀ {E : List Frame}, FhE Ψ anc A E → Chain anc A → Chain anc (holeE A E)
  | [], _, h => h
  | F :: E, ⟨hE, hF⟩, h => by
    have h' := FhE.chain hle hE h
    cases F <;> simp only [holeE, Frame.hole] <;> try exact h'
    exact h'.inside hF.1 hF.2 (hle _)

/-- The region of a frame of a context is a region of the hole. -/
theorem FhE.reg {A : Nat → Prop} {p : Nat} :
    ∀ {E : List Frame}, FhE Ψ anc A E → ∀ F ∈ E, F.reg = some p → holeE A E p
  | [], _, _, h, _ => by simp at h
  | G :: E, ⟨hE, hG⟩, F, hF, hr => by
    rcases List.mem_cons.mp hF with rfl | hF
    · cases F <;> simp only [Frame.reg, Option.some.injEq, reduceCtorEq] at hr <;> subst hr <;>
        simp only [Frame.Fh] at hG <;>
        first
        | exact holeE_cons hG.1
        | exact holeE_cons hG
        | simp [holeE, Frame.hole]
    · exact holeE_cons (FhE.reg hE F hF hr)

/-- The handlers of a context have their cells. -/
theorem FhE.cell {A : Nat → Prop} {q ℓ : Nat} :
    ∀ {E : List Frame}, FhE Ψ anc A E → ∀ F ∈ E, F.cell = some (q, ℓ) → LCell Ψ anc q ℓ
  | [], _, _, h, _ => by simp at h
  | G :: E, ⟨hE, hG⟩, F, hF, hc => by
    rcases List.mem_cons.mp hF with rfl | hF
    · cases F <;> simp only [Frame.cell, Option.some.injEq, Prod.mk.injEq, reduceCtorEq] at hc <;>
        obtain ⟨rfl, rfl⟩ := hc <;> exact hG.2
    · exact FhE.cell hE F hF hc

end Flick
