import Formal.RT.Reach

/-!
# λRT: `Ref.modify` is linearizable (R7, runtime.md §3.7)

`cell ℓ J` replays the journal: the value the last `make`/`modify` of `ℓ` left. R7 says each
`modify` read exactly that value, so the history of `ℓ` is sequential, and the journal's
value is the memory's. Without Г8 the model has `Local.modRead` and `Step.modWrite`, which
read and write in two steps, and R7 fails.
-/

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- One event's effect on the value of `ℓ`. -/
def cellStep (l : Nat) (e : Ev V) (r : Option V) : Option V :=
  match e with
  | .make l' v => if l' = l then some v else r
  | .modify l' _ _ v' => if l' = l then some v' else r
  | _ => r

/-- The value of `ℓ` after the journal (newest first). -/
def cell (l : Nat) (J : List (Ev V)) : Option V := J.foldr (cellStep l) none

/-- What a `Ref` event requires of the past. -/
def GoodRef : Ev V → List (Ev V) → Prop
  | .make l _, J => cell l J = none
  | .modify l v _ _, J => cell l J = some v
  | _, _ => True

/-- **R7.** Every `modify ℓ` read the value of the latest `make`/`modify` of `ℓ`, every
`make` took a fresh `ℓ`, and the memory is the replay of the journal. -/
def R7 (C : Conf T V) : Prop :=
  Hist GoodRef C.J ∧ (∀ l, cell l C.J = C.μ l) ∧ ∀ l, C.n ≤ l → C.μ l = none

theorem R7.init (e₀ : T) (unit : V) : R7 (init e₀ unit) :=
  ⟨fun J₁ e J₂ h => by cases J₁ <;> simp [RT.init] at h, fun _ => rfl, fun _ _ => rfl⟩

set_option maxHeartbeats 1000000 in
theorem R7.step [DecidableEq V] (h : Hyp L) (hr : R7 C) (hs : Step K L C C') : R7 C' := by
  obtain ⟨hJ, hc, hf⟩ := hr
  have g8 := h.g8
  cases hs
  case make f φ e e' v hf' hc' hst =>
    refine ⟨hist_cons.2 ⟨?_, hJ⟩, fun l => ?_, fun l hl => ?_⟩
    · show cell C.n C.J = none; rw [hc]; exact hf _ (Nat.le_refl _)
    · show cellStep l (.make C.n v) (cell l C.J) = upd C.μ C.n (some v) l
      by_cases e : l = C.n
      · subst e; simp [cellStep]
      · simp [cellStep, upd, e, Ne.symm e, hc]
    · have : l ≠ C.n := by simp only at hl; omega
      simp only [upd, this, if_false]; exact hf l (by simp only at hl; omega)
  case modify f φ e e' l g v hf' hc' hst hμ =>
    refine ⟨hist_cons.2 ⟨?_, hJ⟩, fun l' => ?_, fun l' hl => ?_⟩
    · show cell l C.J = some v; rw [hc]; exact hμ
    · show cellStep l' (.modify l v (g v).1 (g v).2) (cell l' C.J) = upd C.μ l (some (g v).2) l'
      by_cases e : l' = l
      · subst e; simp [cellStep]
      · simp [cellStep, upd, e, Ne.symm e, hc]
    · simp only [upd]; split
      · subst_vars; rw [hf _ hl] at hμ; cases hμ
      · exact hf _ hl
  case modWrite => simp_all
  all_goals
    refine ⟨?_, fun l => ?_, fun l hl => ?_⟩ <;>
    simp only [hist_cons, GoodRef, true_and] <;>
    first
    | exact hJ
    | exact hc l
    | exact hf l hl
    | exact hf l (by simp only at hl; omega)

end Flick.RT
