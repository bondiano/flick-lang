import Formal.Equiv.Simple

/-!
# Contextual equivalence: steps of the left side of a special pair

The left side of a special pair (`Act`) steps by its own redex while the right side waits, or,
for `lazy v` against `born v'` (O1b), runs `born v'` to its value (`act_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- The cells other threads are about to force stay pending. -/
def KeepO (W : World) (τ : Option (Nat × Nat)) (μ μ' : List Tm) : Prop :=
  ∀ n τ' e, τ' ≠ τ → W.O n τ' → μ[n]? = some (.lazy e) → μ'[n]? = some (.lazy e)

theorem KeepO.of_junk {τ : Option (Nat × Nat)} {μ μ' μ₂ : List Tm} (hst : StR W μ μ₂)
    (h : ∀ n, W.J₁ n → μ'[n]? = μ[n]?) : KeepO W τ μ μ' :=
  fun n _ _ _ ho he => (h n (hst.own _ _ ho)).trans he

theorem KeepO.append {τ : Option (Nat × Nat)} {μ μ₂ : List Tm} (hst : StR W μ μ₂) (c : Tm) :
    KeepO W τ μ (μ ++ [c]) :=
  KeepO.of_junk hst fun n hn => getElem?_append_old (hst.j₁ n hn).1

/-- The right side's answer to a step of the thread `τ` that keeps the regions. -/
def KAns (root : Roots) (W : World) (τ : Option (Nat × Nat)) (μ₂ : List Tm)
    (t₂ : Tm) (μ μ' : List Tm) (t' : Tm) : Prop :=
  ∃ W' μ₂' t₂', W.le W' ∧ KSteps root μ₂ t₂ μ₂' t₂' ∧ ThR W' μ' τ t' t₂' ∧ StR W' μ' μ₂' ∧
    KeepO W τ μ μ'

theorem lazy_redex (e : Tm) : Redex (.lazy e) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h

theorem let_redex {v b : Tm} (hv : Value v) : Redex (.let_ v b) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h; exact h.1 ▸ hv

theorem force_redex (l : Nat) : Redex (.force (.loc l)) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h; exact h ▸ .loc

theorem settle_redex {l : Nat} {v : Tm} (hv : Value v) : Redex (.settle l v) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h; exact h.2 ▸ hv

theorem todo_redex : Redex .todo := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h

/-- The left side of a special pair steps at its redex `r` in `F`. -/
theorem step_in {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm} {E₁ : List Frame}
    {F : Frame} {r t' : Tm} (hE₁ : OkE E₁) (hF : F.ok) (hr : Redex r) (hs : r.Simple)
    (h : Step root l Λ μ (plug E₁ (F.plug r)) (.cfg Λ' μ' t')) :
    (∃ r', PHead r r' ∧ Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (F.plug r')) ∨
      (∃ r', RHead Λ μ r Λ' μ' r' ∧ t' = plug E₁ (F.plug r')) :=
  Step.simple (E := F :: E₁) (hE₁.cons' hF) hr hs h

theorem born_closed {v : Tm} (h : (Tm.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))).Closed) :
    v.Closed := fun i hi => h i (by simp only [Tm.Free]; exact Or.inl hi)

theorem lazy_closed {v : Tm} (h : (Tm.lazy v).Closed) : v.Closed := fun i hi => h i hi

theorem append_set_last {μ : List Tm} {c c' : Tm} : (μ ++ [c]).set μ.length c' = μ ++ [c'] := by
  simp

end Flick.Equiv
