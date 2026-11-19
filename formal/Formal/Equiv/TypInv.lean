import Formal.Lemma3
import Formal.Elab.CoreCgHdl

/-!
# Equivalence of every run: what typing says about the focus of a thread

A typed thread steps at a term, not at a list of alternatives (`Typ.not_alts`), and the arm of a
handler holds its body parked at a hole (`Typ.parked`), as the steps that resume it require. The
reflection of the right side's steps on the left one (`Formal/Equiv/Refl.lean`) reads both from
the typing of the left thread, through its context (`ctx_typing`).
-/

namespace Flick.Equiv

open Flick.Elab (IsAlts)

variable {S : Sig}

/-- A term typed as a term is no list of alternatives. -/
theorem _root_.Flick.Typ.not_alts {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {j : J}
    (h : Typ S Ψ Θ Γ e j) : ∀ t φ, j = .ex t φ → ¬ IsAlts e := by
  induction h with
  | conv _ _ ih => exact fun _ _ _ => ih _ _ rfl
  | allI hv => exact fun _ _ _ ha => by cases hv <;> simp [IsAlts] at ha
  | errVal _ _ _ ih => exact fun _ _ _ => ih _ _ rfl
  | _ => first | (intro _ _ hj; cases hj; done) | (intro _ _ _ ha; simp [IsAlts] at ha)

/-- The arm of a handler holds its body parked at a hole. -/
theorem _root_.Flick.Typ.hctl_parked {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {j : J}
    (h : Typ S Ψ Θ Γ e j) {p ℓ κ : Nat} {tl : Bool} {E : Name} {al rf a k : Tm}
    (he : e = .hctl p ℓ κ tl E al rf a k) : ∃ Ei, OkE Ei ∧ k = plug Ei (.hole p) := by
  induction h with
  | conv _ _ ih => exact ih he
  | allI _ _ ih => exact ih he
  | errVal _ _ _ ih => exact ih he
  | hctl => cases he; exact ⟨_, ‹OkE _›, ‹_ = plug _ _›⟩
  | _ => cases he

/-- A resumed body runs with its arm parked at a hole. -/
theorem _root_.Flick.Typ.hres_parked {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {j : J}
    (h : Typ S Ψ Θ Γ e j) {p ℓ : Nat} {a b : Tm} (he : e = .hres p ℓ a b) :
    ∃ A, OkE A ∧ a = plug A (.hole p) := by
  induction h with
  | conv _ _ ih => exact ih he
  | allI _ _ ih => exact ih he
  | errVal _ _ _ ih => exact ih he
  | hres => cases he; exact ⟨_, ‹OkE _›, ‹_ = plug _ _›⟩
  | _ => cases he

/-- The arm of a kept handler holds its body parked at a hole. -/
theorem _root_.Flick.Typ.hkarm_parked {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {j : J}
    (h : Typ S Ψ Θ Γ e j) {q ℓ : Nat} {E : Name} {al a k : Tm} (he : e = .hkarm q ℓ E al a k) :
    ∃ Ei, OkE Ei ∧ k = plug Ei (.hole q) := by
  induction h with
  | conv _ _ ih => exact ih he
  | allI _ _ ih => exact ih he
  | errVal _ _ _ ih => exact ih he
  | hkarm => cases he; exact ⟨_, ‹OkE _›, ‹_ = plug _ _›⟩
  | _ => cases he

/-- A thread typed as a term: the part of it in an ordered context is typed as a term. -/
def TypedT (S : Sig) (t : Tm) : Prop := ∃ Ψ Θ ty φ, Typ S Ψ Θ [] t (.ex ty φ)

theorem TypedT.inner {t : Tm} (h : TypedT S t) {E : List Frame} {r : Tm} (hE : OkE E)
    (hr : ¬ Value r) (ht : t = plug E r) : TypedT S r := by
  obtain ⟨Ψ, Θ, ty, φ, h⟩ := h
  subst ht
  obtain ⟨K, Θ', t', φ', h', -⟩ := ctx_typing E hE hr h
  exact ⟨_, _, _, _, h'⟩

theorem TypedT.not_alts {r : Tm} (h : TypedT S r) : ¬ IsAlts r :=
  let ⟨_, _, _, _, h⟩ := h; h.not_alts _ _ rfl

end Flick.Equiv
