import Formal.RT.Layers2
import Formal.RT.RefLin
import Formal.RT.Props

/-!
# λRT: T5 (runtime.md §6)

R1–R4, R6 and R7 hold on every path from a program, for every core, under the hypotheses
about the libraries (`Hyp`); the last part of R6 also needs Г11, the premise of Р37. R5 and
R8 are about the functions of §2 and hold outright.
-/

namespace Flick.RT

variable {T V : Type} [DecidableEq V] {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- What a use of a service requires of the past: its layer was not released. -/
def GoodUse : Ev V → List (Ev V) → Prop
  | .use θ, J => ∀ π ∈ θ, ∀ m Ly σ i, .obs π m Ly σ i ∈ J → .release m Ly σ ∉ J
  | _, _ => True

/-- **R6.** (a) A memo map builds a layer again only after releasing its build; (b) a build
is released only after the observer of every consumer has run, i.e. every consumer owner
reached (C5); (c) with Г11, no service is used after its layer's area is released. -/
def R6 (J : List (Ev V)) : Prop := Hist Good6 J ∧ Hist GoodUse J

theorem Inv6.step (h : Hyp L) (I : Inv C) (I6 : Inv6 C) (hs : Step K L C C') : Inv6 C' :=
  ⟨step_hist6 h I I6 hs, step_live6 h I I6 hs, step_obsIn I I6 hs, step_obsFin I6 hs,
   step_building I I6 hs, step_noRel I I6 hs⟩

omit [DecidableEq V] in
theorem mem_order {α : Type} {a b : α} {J : List α} (ha : a ∈ J) (hb : b ∈ J) :
    a = b ∨ (∃ J₁ J₂, J = J₁ ++ b :: J₂ ∧ a ∈ J₂) ∨ (∃ J₁ J₂, J = J₁ ++ a :: J₂ ∧ b ∈ J₂) := by
  induction J with
  | nil => cases ha
  | cons x J ih =>
    rcases List.mem_cons.1 ha with rfl | ha' <;> rcases List.mem_cons.1 hb with rfl | hb'
    · exact .inl rfl
    · exact .inr (.inr ⟨[], J, rfl, hb'⟩)
    · exact .inr (.inl ⟨[], J, rfl, ha'⟩)
    · rcases ih ha' hb' with h | ⟨J₁, J₂, rfl, h⟩ | ⟨J₁, J₂, rfl, h⟩
      · exact .inl h
      · exact .inr (.inl ⟨x :: J₁, J₂, rfl, h⟩)
      · exact .inr (.inr ⟨x :: J₁, J₂, rfl, h⟩)

omit [DecidableEq V] in
theorem use_step (I : Inv C) (I6 : Inv6 C) (hu : Hist GoodUse C.J)
    (h11 : ∀ f φ, C.Φ f = some (.run φ) → ∀ π ∈ φ.env, ∃ o, C.Λ π = some o ∧ o.st.live = true)
    (hs : Step K L C C') : Hist GoodUse C'.J := by
  cases hs <;> simp only [hist_cons, GoodUse, hu, and_true]
  case use f φ e e' hf hc hst =>
    intro π hπ m Ly σ i hob hrel
    obtain ⟨o, ho, hl⟩ := h11 f φ hf π hπ
    rcases mem_order hob hrel with h | ⟨J₁, J₂, hJ, h⟩ | ⟨J₁, J₂, hJ, h⟩
    · cases h
    · have hr := I6.hist J₁ _ J₂ hJ π i h
      have hf' := I.run_fins (π := π) (i := i) (by rw [hJ]; simp [hr])
      exact (I.stJ π o ho).2.1.1 hl hf'
    · exact I6.hist J₁ _ J₂ hJ h

theorem reach_all (h : Hyp L) {e₀ : T} (h11 : G11 K L e₀) (hr : Reach K L e₀ C) :
    Inv C ∧ Inv6 C ∧ Hist GoodUse C.J ∧ R7 C := by
  induction hr with
  | refl => exact ⟨Inv.initial e₀ K.unit, Inv6.initial e₀ K.unit,
      fun J₁ e J₂ h => by cases J₁ <;> simp [RT.init] at h, R7.init e₀ K.unit⟩
  | step hp hs ih =>
    obtain ⟨I, I6, hu, h7⟩ := ih
    exact ⟨I.step h hs, I6.step h I hs, use_step I I6 hu (h11 _ hp) hs, h7.step h hs⟩

/-- **T5 (the runtime contract).** On every path from a program `e₀` — any core, any
interleaving of §3.1 — under the hypotheses about the libraries (Г7, Г8, Г9) and Г11, the
reachable configuration satisfies R1–R4, R6 and R7; R5 and R8 hold of the functions of §2. -/
theorem T5 (h : Hyp L) {e₀ : T} (h11 : G11 K L e₀) (hr : Reach K L e₀ C) :
    R1 C.J ∧ R2 C.J ∧ R3 C ∧ R4 C ∧ R6 C.J ∧ R7 C ∧
    (∀ {T' : Type} (Has Has' : V → Prop) (arms : V → Option T') (c c' : Cause V),
      Iε Has c → Covers Has Has' arms → recover arms c = .fail c' →
      Iε Has' c' ∧ ∀ v, head c' = some v → arms v = none) := by
  obtain ⟨I, I6, hu, h7⟩ := reach_all h h11 hr
  exact ⟨I.r1, I.r2, I.r3, I.r4, ⟨I6.hist, hu⟩, h7, fun _ _ _ _ _ h1 h2 h3 => R5 h1 h2 h3⟩

end Flick.RT
