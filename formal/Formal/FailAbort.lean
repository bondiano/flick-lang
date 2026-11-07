import Formal.Frames
import Formal.Semantics

/-!
# `Fail` and `abort`

`Fail.fail v` is `raise v`, and its arm in a `try` the `handle` of that `try`.
An `abort` operation is `unw`, and its arm runs when the signal reaches the
frame of its handler. Across frames with no `scope` and no arm of a handler
between, the two go the same way: each frame lets the signal through, the
regions and the store do not change, and the arm receives the value. They only
differ in what stops them: `raise` stops at any `try`, and `unw` passes it.
-/

namespace Flick

private theorem steps_trans {root : Roots} {Q : Lbl → Prop} {Λ Λ₁ Λ₂ : List Rgn}
    {μ μ₁ μ₂ : List Tm} {e e₁ e₂ : Tm} (h₁ : Steps root Q Λ μ e Λ₁ μ₁ e₁)
    (h₂ : Steps root Q Λ₁ μ₁ e₁ Λ₂ μ₂ e₂) : Steps root Q Λ μ e Λ₂ μ₂ e₂ := by
  induction h₁ with
  | refl => exact h₂
  | step s q _ ih => exact .step s q (ih h₂)

private theorem okE_append {A B : List Frame} (hA : OkE A) (hB : OkE B) : OkE (A ++ B) := by
  intro F hF
  rcases List.mem_append.1 hF with h | h
  · exact hA F h
  · exact hB F h

/-- A raised error crosses frames that are no `try`, leaving regions and store alone. -/
theorem raise_crosses {root : Roots} {Q : Lbl → Prop} (hQ : Q .rt) {Λ : List Rgn} {μ : List Tm}
    {E Ei : List Frame} {v : Tm} (hE : OkE E) (hv : Value v)
    (hi : ∀ F ∈ Ei, F.ok ∧ ¬ F.isTry ∧ ¬ F.isInside ∧ ¬ F.isCtl) :
    Steps root Q Λ μ (plug E (plug Ei (.raise v))) Λ μ (plug E (.raise v)) := by
  induction Ei with
  | nil => exact .refl
  | cons F Ei ih =>
    have hF := hi F (by simp)
    have rest := fun G hG => hi G (List.mem_cons_of_mem F hG)
    have hOk : OkE (Ei ++ E) := okE_append (fun G hG => (rest G hG).1) hE
    have step : Step root .rt Λ μ (plug (Ei ++ E) (F.plug (.raise v)))
        (.cfg Λ μ (plug (Ei ++ E) (.raise v))) := .raise hOk hF.1 hF.2.1 hF.2.2.1 hF.2.2.2 hv
    simp only [plug_append] at step
    exact .step step hQ (ih rest)

/-- The signal of an `abort` crosses frames that are not its handler, leaving regions and
store alone. -/
theorem unw_crosses {root : Roots} {Q : Lbl → Prop} (hQ : Q .rt) {Λ : List Rgn} {μ : List Tm}
    {E Ei : List Frame} {p ℓ : Nat} {a : Tm} (hE : OkE E)
    (hi : ∀ F ∈ Ei, F.ok ∧ ¬ F.isInside ∧ ¬ F.isCtl ∧ ¬ F.catches p ℓ) :
    Steps root Q Λ μ (plug E (plug Ei (.unw p ℓ a))) Λ μ (plug E (.unw p ℓ a)) := by
  induction Ei with
  | nil => exact .refl
  | cons F Ei ih =>
    have hF := hi F (by simp)
    have rest := fun G hG => hi G (List.mem_cons_of_mem F hG)
    have hOk : OkE (Ei ++ E) := okE_append (fun G hG => (rest G hG).1) hE
    have step : Step root .rt Λ μ (plug (Ei ++ E) (F.plug (.unw p ℓ a)))
        (.cfg Λ μ (plug (Ei ++ E) (.unw p ℓ a))) := .unw hOk hF.1 hF.2.1 hF.2.2.1 hF.2.2.2
    simp only [plug_append] at step
    exact .step step hQ (ih rest)

/-- `Fail.fail v` reaches the arms of the nearest `try`, as `raise`. -/
theorem fail_reaches_arms {root : Roots} {Q : Lbl → Prop} (hQ : Q .rt) {Λ : List Rgn}
    {μ : List Tm} {E Ei : List Frame} {v A : Tm} (hE : OkE E) (hv : Value v)
    (hi : ∀ F ∈ Ei, F.ok ∧ ¬ F.isTry ∧ ¬ F.isInside ∧ ¬ F.isCtl) :
    Steps root Q Λ μ (plug E (.try_ (plug Ei (.raise v)) A)) Λ μ (plug E (.handle v A)) := by
  have hOk : OkE (Frame.try_ A :: E) := by
    intro F hF
    rcases List.mem_cons.1 hF with rfl | h
    · trivial
    · exact hE F h
  have crossing : Steps root Q Λ μ (plug E (.try_ (plug Ei (.raise v)) A)) Λ μ
      (plug E (.try_ (.raise v) A)) := raise_crosses hQ hOk hv hi
  have reach : Steps root Q Λ μ (plug E (.try_ (.raise v) A)) Λ μ (plug E (.handle v A)) :=
    .step (.rt hE (.tryRaise hv)) hQ .refl
  exact steps_trans crossing reach

/-- An `abort` reaches the arm of its handler the same way: its value is the handler's. -/
theorem abort_reaches_arm {root : Roots} {Q : Lbl → Prop} (hQ : Q .rt) {Λ : List Rgn}
    {μ : List Tm} {E Ei : List Frame} {p ℓ : Nat} {Ef : Name} {al r a : Tm} (hE : OkE E)
    (hi : ∀ F ∈ Ei, F.ok ∧ ¬ F.isInside ∧ ¬ F.isCtl ∧ ¬ F.catches p ℓ) :
    Steps root Q Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.unw p ℓ a)))) Λ μ (plug E a) := by
  have hOk : OkE (Frame.hwith p ℓ Ef al r :: E) := by
    intro F hF
    rcases List.mem_cons.1 hF with rfl | h
    · trivial
    · exact hE F h
  have crossing : Steps root Q Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.unw p ℓ a)))) Λ μ
      (plug E (.hwith p ℓ Ef al r (.unw p ℓ a))) := unw_crosses hQ hOk hi
  have reach : Steps root Q Λ μ (plug E (.hwith p ℓ Ef al r (.unw p ℓ a))) Λ μ (plug E a) :=
    .step (.rt hE .hwithCatch) hQ .refl
  exact steps_trans crossing reach

end Flick
