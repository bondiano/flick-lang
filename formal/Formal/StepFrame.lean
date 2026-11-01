import Formal.Lemma3

/-!
# Steps under one more frame
-/

namespace Flick

variable {S : Sig}

/-- `ask_S` whose service is in neither a `with` frame nor `Σ₀`; `D` must hold `S`. -/
def AskStuck (root : Name → Option Tm) (e : Tm) (D : Row) : Prop :=
  ∃ E N, OkE E ∧ e = plug E (.app (.const (.ask N)) (.lit .unit)) ∧ lookupE E N = none ∧
    root N = none ∧ D N

/-- The non-value outcomes of Progress. -/
def NV (root : Name → Option Tm) (μ : List Tm) (e : Tm) (D : Row) : Prop :=
  (∃ v, Value v ∧ e = .raise v) ∨ (∃ l o, Step root l μ e o) ∨ AskStuck root e D

theorem Step.ofP {root μ e e'} (h : PHead e e') : Step root .pure μ e (.cfg μ e') :=
  Step.pure (E := []) OkE.nil h

theorem Step.ofR {root μ e μ' e'} (h : RHead μ e μ' e') : Step root .rt μ e (.cfg μ' e') :=
  Step.rt (E := []) OkE.nil h

theorem Step.frame {root l μ e o} (h : Step root l μ e o) {F : Frame} (hF : F.ok) :
    ∃ l' o', Step root l' μ (F.plug e) o' := by
  cases h with
  | pure hE hh =>
    have := Step.pure (root := root) (μ := μ) (hE.append hF) hh
    rw [plug_append, plug_append] at this; exact ⟨_, _, this⟩
  | rt hE hh =>
    have := Step.rt (root := root) (hE.append hF) hh
    rw [plug_append, plug_append] at this; exact ⟨_, _, this⟩
  | askWith hE hl =>
    have := Step.askWith (root := root) (μ := μ) (hE.append hF)
      (by rw [lookupE_append, hl])
    rw [plug_append, plug_append] at this; exact ⟨_, _, this⟩
  | @askRoot E N v _ hE hl hr =>
    cases hF' : F.lookup N with
    | some w =>
      have := Step.askWith (root := root) (μ := μ) (hE.append hF)
        (by rw [lookupE_append, hl]; exact hF')
      rw [plug_append, plug_append] at this; exact ⟨_, _, this⟩
    | none =>
      have := Step.askRoot (root := root) (μ := μ) (hE.append hF)
        (by rw [lookupE_append, hl]; exact hF') hr
      rw [plug_append, plug_append] at this; exact ⟨_, _, this⟩
  | raise hE hG hnt hv =>
    have := Step.raise (root := root) (μ := μ) (hE.append hF) hG hnt hv
    rw [plug_append, plug_append] at this; exact ⟨_, _, this⟩
  | todo hE =>
    have := Step.todo (root := root) (μ := μ) (hE.append hF)
    rw [plug_append] at this; exact ⟨_, _, this⟩

theorem AskStuck.frame {root μ e D D'} (h : AskStuck root e D) {F : Frame} (hF : F.ok)
    (hD : ∀ n, D n → F.dom n ∨ D' n) :
    (∃ l o, Step root l μ (F.plug e) o) ∨ AskStuck root (F.plug e) D' := by
  obtain ⟨E, N, hE, rfl, hl, hr, hN⟩ := h
  cases hF' : F.lookup N with
  | some w =>
    have := Step.askWith (root := root) (μ := μ) (hE.append hF)
      (by rw [lookupE_append, hl]; exact hF')
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | none =>
    refine .inr ⟨E ++ [F], N, hE.append hF, by rw [plug_append]; rfl,
      by rw [lookupE_append, hl]; exact hF', hr, ?_⟩
    rcases hD N hN with h | h
    · simp [Frame.dom, hF'] at h
    · exact h

/-- Lift the non-value outcomes over a frame that is not `try`. -/
theorem NV.frame {root μ e D D'} (h : NV root μ e D) {F : Frame} (hF : F.ok) (hnt : ¬ F.isTry)
    (hD : ∀ n, D n → F.dom n ∨ D' n) : NV root μ (F.plug e) D' := by
  rcases h with ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs
  · exact .inr (.inl ⟨_, _, Step.raise (E := []) OkE.nil hF hnt hv⟩)
  · exact .inr (.inl (hs.frame hF))
  · rcases hs.frame (μ := μ) hF hD with h | h
    · exact .inr (.inl h)
    · exact .inr (.inr h)

/-- The common case: the frame keeps the stage. -/
theorem NV.frame' {root μ e D} (h : NV root μ e D) {F : Frame} (hF : F.ok) (hnt : ¬ F.isTry) :
    NV root μ (F.plug e) D :=
  h.frame hF hnt fun _ h => .inr h

end Flick
