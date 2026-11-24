import Formal.Elab.CoreRel

/-!
# λFlick⁺ as a `Target`: the lazy cells of related stores

A step of a lazy cell reads the outer form of its content (`Tm.IsCell`). Related contents have
the same outer form when no base pair has a lazy cell's form on either side (`BNoCell`): a
pending cell is related to a pending one, a running cell to a running one, and a settled cell to
a settled one. The base pairs of dictionaries have no such form (`DP.noCell`).
-/

namespace Flick.Elab

variable {B : Flick.Tm → Flick.Tm → Prop}

/-- No side of a base pair has the form of a lazy cell. -/
def BNoCell (B : Flick.Tm → Flick.Tm → Prop) : Prop := ∀ a b, B a b → ¬ a.IsCell ∧ ¬ b.IsCell

/-- A term with a type at the pure stage has no form of a lazy cell. -/
theorem _root_.Flick.Typ.not_cell_pure {S : Sig} {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ t, J = .ex t .pure → ¬ e.IsCell := by
  induction h with
  | conv _ _ ih => intro t hJ; cases hJ; exact ih _ rfl
  | errVal _ _ _ ih => intro t hJ; cases hJ; exact ih _ rfl
  | funcRef _ _ ih => intro t hJ; cases hJ; exact ih _ rfl
  | allI hv => intro t _ hc; cases hv <;> simp [Flick.Tm.IsCell] at hc
  | lazy _ _ _ hφ => intro t hJ; cases hJ; exact fun _ => hφ
  | await _ hφ _ | force _ hφ _ => intro t hJ; cases hJ; exact fun _ => Stage.use_rt_le_ne_pure hφ rfl
  | _ => intro t hJ hc; cases hJ <;> simp [Flick.Tm.IsCell] at hc

theorem DP.noCell {S : Sig} {root : Roots} : BNoCell (DP S root) := by
  rintro a b ⟨-, -, ⟨-, -, -, hb⟩, -, Θ, t, ht⟩
  exact ⟨ht.not_cell_pure _ rfl, hb⟩

theorem Cg.cell (hL : BNoCell B) {c c₂ : Flick.Tm} (h : Cg B false c c₂) : c.IsCell ↔ c₂.IsCell := by
  cases h with
  | base hb => exact ⟨fun hc => ((hL _ _ hb).1 hc).elim, fun hc => ((hL _ _ hb).2 hc).elim⟩
  | _ => simp [Flick.Tm.IsCell]

theorem Cg.lazy_left (hL : BNoCell B) {e c₂ : Flick.Tm} (h : Cg B false (.lazy e) c₂) :
    ∃ e₂, c₂ = .lazy e₂ ∧ Cg B false e e₂ := by
  cases h with
  | lazy he => exact ⟨_, rfl, he⟩
  | base hb => exact ((hL _ _ hb).1 trivial).elim

theorem Cg.done_left (hL : BNoCell B) {o c₂ : Flick.Tm} (h : Cg B false (.done o) c₂) :
    ∃ o₂, c₂ = .done o₂ ∧ Cg B false o o₂ := by
  cases h with
  | await he => exact ⟨_, rfl, he⟩
  | base hb => exact ((hL _ _ hb).1 trivial).elim

theorem Cg.lazy_right (hL : BNoCell B) {c e₂ : Flick.Tm} (h : Cg B false c (.lazy e₂)) :
    ∃ e, c = .lazy e := by
  cases h with
  | lazy => exact ⟨_, rfl⟩
  | base hb => exact ((hL _ _ hb).2 trivial).elim

theorem Cg.done_right (hL : BNoCell B) {c o₂ : Flick.Tm} (h : Cg B false c (.done o₂)) :
    ∃ o, c = .done o := by
  cases h with
  | await => exact ⟨_, rfl⟩
  | base hb => exact ((hL _ _ hb).2 trivial).elim

theorem Cg.running_left (hL : BNoCell B) {e c₂ : Flick.Tm} (h : Cg B false (.running e) c₂) :
    ∃ e₂, c₂ = .running e₂ ∧ Cg B false e e₂ := by
  cases h with
  | force he => exact ⟨_, rfl, he⟩
  | base hb => exact ((hL _ _ hb).1 trivial).elim

theorem Cg.running_right (hL : BNoCell B) {c e₂ : Flick.Tm} (h : Cg B false c (.running e₂)) :
    ∃ e, c = .running e := by
  cases h with
  | force => exact ⟨_, rfl⟩
  | base hb => exact ((hL _ _ hb).2 trivial).elim

theorem All2.getElem?' {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    {i : Nat} {b : β} (hb : l'[i]? = some b) : ∃ a, l[i]? = some a ∧ R a b := by
  induction h generalizing i with
  | nil => simp at hb
  | cons r _ ih =>
    cases i with
    | zero => simp at hb; subst hb; exact ⟨_, rfl, r⟩
    | succ i => exact ih (by simpa using hb)

/-- Related stores settle a cell alike: it is running on both sides or on neither. -/
theorem All2.settleCell (hL : BNoCell B) {μ μ₂ : List Flick.Tm}
    (h : All2 (Cg B false) μ μ₂) (l : Nat) {o o₂ : Flick.Tm} (ho : Cg B false (.done o) (.done o₂)) :
    All2 (Cg B false) (Flick.settleCell μ l o) (Flick.settleCell μ₂ l o₂) := by
  unfold Flick.settleCell
  split <;> split
  · exact h.set l ho
  · rename_i h₁ h₂
    obtain ⟨_, h₁⟩ := h₁
    obtain ⟨c₂, hc₂, r⟩ := h.getElem? h₁
    obtain ⟨e₂, rfl, -⟩ := r.running_left hL
    exact (h₂ ⟨e₂, hc₂⟩).elim
  · rename_i h₁ h₂
    obtain ⟨_, h₂⟩ := h₂
    obtain ⟨c, hc, r⟩ := h.getElem?' h₂
    obtain ⟨e, rfl⟩ := r.running_right hL
    exact (h₁ ⟨e, hc⟩).elim
  · exact h

/-- Related stores reset a cell alike, each to its own initializer. -/
theorem All2.resetCell (hL : BNoCell B) {μ μ₂ : List Flick.Tm}
    (h : All2 (Cg B false) μ μ₂) (l : Nat) :
    All2 (Cg B false) (Flick.resetCell μ l) (Flick.resetCell μ₂ l) := by
  rcases Flick.resetCell_cases μ l with ⟨e, h₁, r₁⟩ | ⟨h₁, r₁⟩ <;>
    rcases Flick.resetCell_cases μ₂ l with ⟨e₂, h₂, r₂⟩ | ⟨h₂, r₂⟩ <;> rw [r₁, r₂]
  · obtain ⟨c₂, hc₂, r⟩ := h.getElem? h₁
    rw [h₂] at hc₂; cases hc₂
    cases r with
    | force he => exact h.set l (.lazy he)
    | base hb => exact ((hL _ _ hb).1 trivial).elim
  · obtain ⟨c₂, hc₂, r⟩ := h.getElem? h₁
    obtain ⟨e₂, rfl, -⟩ := r.running_left hL
    exact (h₂ e₂ hc₂).elim
  · obtain ⟨c, hc, r⟩ := h.getElem?' h₂
    obtain ⟨e, rfl⟩ := r.running_right hL
    exact (h₁ e hc).elim
  · exact h

end Flick.Elab
