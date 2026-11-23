import Formal.Equiv.PHead

/-!
# Contextual equivalence: steps at the head of the right side, and related cells

`HSeq` is a sequence of head steps of the right side at its top; the right side answers a
runtime step of the left one by it. Cells related at `top` have the same state
(`Sim.top_cell`), so `sym` pairs settle and reset together (`StR.settleSym`, `StR.resetSym`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

/-- Head steps (and pure multi-frame steps) of a term that read no region. -/
inductive HSeq (root : Roots) : List Tm → Tm → List Tm → Tm → Prop
  | refl {μ t} : HSeq root μ t μ t
  | rt {μ t μ₁ t₁ μ' t'} : (∀ Λ, RHead Λ μ t Λ μ₁ t₁) → HSeq root μ₁ t₁ μ' t' → HSeq root μ t μ' t'
  | pure {μ t t₁ μ' t'} : PHead t t₁ → HSeq root μ t₁ μ' t' → HSeq root μ t μ' t'
  | pstep {μ t t₁ μ' t'} : PStep root t t₁ → HSeq root μ t₁ μ' t' → HSeq root μ t μ' t'

theorem HSeq.one {root : Roots} {μ t μ' t'} (h : ∀ Λ, RHead Λ μ t Λ μ' t') : HSeq root μ t μ' t' :=
  .rt h .refl

theorem HSeq.of_psteps {root : Roots} {μ μ' : List Tm} {t t₁ t' : Tm}
    (hp : PSteps root t t₁) (h : HSeq root μ t₁ μ' t') : HSeq root μ t μ' t' := by
  induction hp with
  | refl => exact h
  | step s _ ih => exact .pstep s (ih h)

theorem Steps.trans' {root : Roots} {Q : Lbl → Prop} {Λ μ t Λ₁ μ₁ t₁ Λ' μ' t'}
    (h₁ : Steps root Q Λ μ t Λ₁ μ₁ t₁) (h₂ : Steps root Q Λ₁ μ₁ t₁ Λ' μ' t') :
    Steps root Q Λ μ t Λ' μ' t' := by
  induction h₁ with
  | refl => exact h₂
  | step s q _ ih => exact .step s q (ih h₂)

/-- Cells related at `top` are in the same state. -/
theorem Sim.top_cell {x : Pos} {c c' : Tm} (h : Sim W x c c') (hx : x = .top) :
    ((∃ e, c = .lazy e) ↔ ∃ e', c' = .lazy e') ∧ ((∃ e, c = .running e) ↔ ∃ e', c' = .running e') ∧
      ((∃ o, c = .done o) ↔ ∃ o', c' = .done o') := by
  cases h with
  | jL hx' | jR hx' | againF2 hx' | againR2 hx' => exact (hx' hx).elim
  | dropA | dropB | dropC | forceA | forceC | bornA | bornB | bornS | bornV | dead
  | o4A | o4B | o4Alloc1 | o4AllocR | o4Bind | o4Settle
  | doneF | doneR | againF1 | againF3 | againR1 | againR3 | letRaise | letUnw | letTodo => cases hx
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | _ => simp

theorem Sim.top_isCell {c c' : Tm} (h : Sim W .top c c') : c.IsCell ↔ c'.IsCell := by
  obtain ⟨h₁, h₂, h₃⟩ := h.top_cell rfl
  constructor
  · intro hc
    cases c <;> simp [Tm.IsCell] at hc <;> first
      | (obtain ⟨e', rfl⟩ := h₁.1 ⟨_, rfl⟩; trivial)
      | (obtain ⟨e', rfl⟩ := h₂.1 ⟨_, rfl⟩; trivial)
      | (obtain ⟨e', rfl⟩ := h₃.1 ⟨_, rfl⟩; trivial)
  · intro hc
    cases c' <;> simp [Tm.IsCell] at hc <;> first
      | (obtain ⟨e, rfl⟩ := h₁.2 ⟨_, rfl⟩; trivial)
      | (obtain ⟨e, rfl⟩ := h₂.2 ⟨_, rfl⟩; trivial)
      | (obtain ⟨e, rfl⟩ := h₃.2 ⟨_, rfl⟩; trivial)

/-- `sym` cells settle together. -/
theorem StR.settleSym {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} (hl : W.L l l' .sym)
    {o o' : Tm} (ho : Sim W .any o o') : StR W (settleCell μ₁ l o) (settleCell μ₂ l' o') := by
  obtain ⟨c, c', h₁, h₂, hc⟩ := h.cell _ _ _ hl
  have hr := ((hc : Sim W .top c c').top_cell rfl).2.1
  rcases settleCell_cases μ₁ l o with ⟨e, he, e₁⟩ | ⟨hn, e₁⟩ <;>
    rcases settleCell_cases μ₂ l' o' with ⟨e', he', e₂⟩ | ⟨hn', e₂⟩ <;> rw [e₁, e₂]
  · exact h.set hl (.await ho) fun l₀ v₀ v₀' hd => by
      have ⟨_, _, _, _, _, _, hdone, _, _⟩ := h.done l l₀ v₀ v₀' hd
      rw [hdone] at he; cases he
  · rw [h₁] at he; cases he
    obtain ⟨e', rfl⟩ := hr.1 ⟨_, rfl⟩
    exact absurd h₂ (hn' e')
  · rw [h₂] at he'; cases he'
    obtain ⟨e, rfl⟩ := hr.2 ⟨_, rfl⟩
    exact absurd h₁ (hn e)
  · exact h

/-- `sym` cells reset together. -/
theorem StR.resetSym {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} (hl : W.L l l' .sym) :
    StR W (resetCell μ₁ l) (resetCell μ₂ l') := by
  obtain ⟨c, c', h₁, h₂, hc⟩ := h.cell _ _ _ hl
  have hr := ((hc : Sim W .top c c').top_cell rfl).2.1
  rcases resetCell_cases μ₁ l with ⟨e, he, e₁⟩ | ⟨hn, e₁⟩ <;>
    rcases resetCell_cases μ₂ l' with ⟨e', he', e₂⟩ | ⟨hn', e₂⟩ <;> rw [e₁, e₂]
  · rw [h₁] at he; rw [h₂] at he'; cases he; cases he'
    obtain ⟨e₀, he₀, h'⟩ := (hc : Sim W .top _ _).inv_force
    cases he₀
    exact h.set hl (.lazy h') fun l₀ v₀ v₀' hd => by
      have ⟨_, _, _, _, _, _, hdone, _, _⟩ := h.done l l₀ v₀ v₀' hd
      rw [hdone] at h₁; cases h₁
  · rw [h₁] at he; cases he
    obtain ⟨e', rfl⟩ := hr.1 ⟨_, rfl⟩
    exact absurd h₂ (hn' e')
  · rw [h₂] at he'; cases he'
    obtain ⟨e, rfl⟩ := hr.2 ⟨_, rfl⟩
    exact absurd h₁ (hn e)
  · exact h

/-- A related location is no cell only the left side has. -/
theorem StR.not_j₁ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {k : Kd} (hl : W.L l l' k) :
    ¬ W.J₁ l := fun hn => (h.j₁ l hn).2 l' k hl

theorem StR.not_j₂ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {k : Kd} (hl : W.L l l' k) :
    ¬ W.J₂ l' := fun hn => (h.j₂ l' hn).2 l k hl

theorem getElem?_set_of_ne {μ : List Tm} {l n : Nat} {c : Tm} (h : n ≠ l) :
    (μ.set l c)[n]? = μ[n]? := List.getElem?_set_ne (Ne.symm h)

/-- Settling a related cell keeps the cells only the left side has. -/
theorem settleCell_other {μ : List Tm} {l n : Nat} {o : Tm} (h : n ≠ l) :
    (settleCell μ l o)[n]? = μ[n]? := by
  rcases settleCell_cases μ l o with ⟨_, _, e⟩ | ⟨_, e⟩ <;> rw [e]
  exact getElem?_set_of_ne h

theorem resetCell_other {μ : List Tm} {l n : Nat} (h : n ≠ l) : (resetCell μ l)[n]? = μ[n]? := by
  rcases resetCell_cases μ l with ⟨_, _, e⟩ | ⟨_, e⟩ <;> rw [e]
  exact getElem?_set_of_ne h

end Flick.Equiv
