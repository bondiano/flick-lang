import Formal.Equiv.StoreBase

/-!
# Contextual equivalence: store updates on related stores
-/

namespace Flick.Equiv

variable {W : World}

/-- **Allocation of a left cell** a thread forces at its next step. -/
theorem StR.allocO {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) (c : Tm) (τ : Option (Nat × Nat)) :
    StR (W.addO μ₁.length τ) (μ₁ ++ [c]) μ₂ where
  cell := by
    intro l l' k hl
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ hl
    exact ⟨d, d', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁, h₂, h₃.mono (World.le_addO _ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := by
    rintro n (hn | rfl)
    · obtain ⟨h₁, h₂⟩ := h.j₁ n hn; exact ⟨by simp; omega, h₂⟩
    · exact ⟨by simp, fun l' k hl => absurd (h.lt₁ hl) (Nat.lt_irrefl _)⟩
  j₂ := h.j₂
  own := by
    rintro n τ' (ho | ⟨rfl, -⟩)
    · exact .inl (h.own n τ' ho)
    · exact .inr rfl
  ownU := by
    rintro n τ₁ τ₂ (h₁ | ⟨rfl, rfl⟩) (h₂ | ⟨h₃, rfl⟩)
    · exact h.ownU _ _ _ h₁ h₂
    · exact absurd (h₃ ▸ (h.j₁ _ (h.own _ _ h₁)).1) (Nat.lt_irrefl _)
    · exact absurd (h.j₁ _ (h.own _ _ h₂)).1 (Nat.lt_irrefl _)
    · rfl
  hl := by
    intro l e hl
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d, hd, hst⟩ := h.hl l e hl
    have hlt := h.ltHL hl
    refine ⟨hB, hu, hnl, ?_, hcl, hpe, d, by rw [getElem?_append_old hlt]; exact hd, hst⟩
    rintro (h' | rfl)
    · exact hnj h'
    · omega
  hr := h.hr
  hrel := by
    intro l l' hH
    obtain ⟨hB, e, e', hl, hr, hs⟩ := h.hrel l l' hH
    exact ⟨hB, e, e', hl, hr, hs.mono (World.le_addO _ _ _)⟩
  done := by
    intro l l' v v' hd
    obtain ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done l l' v v' hd
    exact ⟨hl, hv, hv', hcv, hcv', hnv', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁, h₂,
      hvv.mono (World.le_addO _ _ _)⟩

/-- **Allocation of a right cell** only the right side has. -/
theorem StR.allocJ₂ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) (c : Tm) :
    StR (W.addJ₂ μ₂.length) μ₁ (μ₂ ++ [c]) where
  cell := by
    intro l l' k hl
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ hl
    exact ⟨d, d', h₁, by rw [getElem?_append_old (h.lt₂ hl)]; exact h₂, h₃.mono (World.le_addJ₂ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := h.j₁
  j₂ := by
    rintro n (hn | rfl)
    · obtain ⟨h₁, h₂⟩ := h.j₂ n hn; exact ⟨by simp; omega, h₂⟩
    · exact ⟨by simp, fun l k hl => absurd (h.lt₂ hl) (Nat.lt_irrefl _)⟩
  own := h.own
  ownU := h.ownU
  hl := h.hl
  hr := by
    intro l' e' hr
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d', hd', hst⟩ := h.hr l' e' hr
    have hlt := h.ltHR hr
    refine ⟨hB, hu, hnl, ?_, hcl, hpe, d', by rw [getElem?_append_old hlt]; exact hd', hst⟩
    rintro (h' | rfl)
    · exact hnj h'
    · omega
  hrel := by
    intro l l' hH
    obtain ⟨hB, e, e', hl, hr, hs⟩ := h.hrel l l' hH
    exact ⟨hB, e, e', hl, hr, hs.mono (World.le_addJ₂ _ _)⟩
  done := by
    intro l l' v v' hd
    obtain ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done l l' v v' hd
    exact ⟨hl, hv, hv', hcv, hcv', hnv', h₁, by rw [getElem?_append_old (h.lt₂ hl)]; exact h₂,
      hvv.mono (World.le_addJ₂ _ _)⟩

/-- A new content of a related pair of cells (never overwriting a cell recorded in `W.D`). -/
theorem StR.set {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {k : Kd} (hl : W.L l l' k)
    {c c' : Tm} (hc : CellR W k c c') (hnd : ∀ l₀ v₀ v₀', ¬ W.D l l₀ v₀ v₀') :
    StR W (μ₁.set l c) (μ₂.set l' c') where
  cell := by
    intro a a' k' ha
    by_cases he : a = l
    · subst he
      have he' : a' = l' := (h.bij _ _ _ _ _ _ ha hl).1 rfl
      subst he'
      obtain rfl := h.kd _ _ _ _ _ ha hl
      exact ⟨c, c', by simp [h.lt₁ hl], by simp [h.lt₂ hl], hc⟩
    · have he' : a' ≠ l' := fun e => he ((h.bij _ _ _ _ _ _ ha hl).2 e)
      obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ ha
      exact ⟨d, d', by rw [List.getElem?_set_ne (Ne.symm he)]; exact h₁,
        by rw [List.getElem?_set_ne (Ne.symm he')]; exact h₂, h₃⟩
  bij := h.bij
  kd := h.kd
  j₁ := fun n hn => by simpa using h.j₁ n hn
  j₂ := fun n hn => by simpa using h.j₂ n hn
  own := h.own
  ownU := h.ownU
  hl := by
    intro a e ha
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d, hd, hst⟩ := h.hl a e ha
    have hne : a ≠ l := fun he => hnl l' k (he ▸ hl)
    exact ⟨hB, hu, hnl, hnj, hcl, hpe, d, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hd, hst⟩
  hr := by
    intro a' e' ha'
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d', hd', hst⟩ := h.hr a' e' ha'
    have hne : a' ≠ l' := fun he => hnl l k (he ▸ hl)
    exact ⟨hB, hu, hnl, hnj, hcl, hpe, d', by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hd', hst⟩
  hrel := h.hrel
  done := by
    intro a a' v v' hd
    obtain ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done a a' v v' hd
    have hne : a ≠ l := fun he => hnd a' v v' (he ▸ hd)
    have hne' : a' ≠ l' := fun he => hne ((h.bij _ _ _ _ _ _ ha hl).2 he)
    exact ⟨ha, hv, hv', hcv, hcv', hnv', by rw [List.getElem?_set_ne (Ne.symm hne)]; exact h₁,
      by rw [List.getElem?_set_ne (Ne.symm hne')]; exact h₂, hvv⟩

/-- Record a settled `sym` cell holding related closed values in `W.D`. -/
theorem StR.markDone {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {v v' : Tm}
    (hl : W.L l l' .sym) (hv : Value v) (hv' : Value v') (hcv : v.Closed) (hcv' : v'.Closed)
    (hnv' : v'.NoIns) (h₁ : μ₁[l]? = some (.done v)) (h₂ : μ₂[l']? = some (.done v'))
    (hvv : Sim W .any v v') :
    StR (W.addD l l' v v') μ₁ μ₂ where
  cell := by
    intro a a' k ha
    obtain ⟨d, d', h₁', h₂', h₃'⟩ := h.cell _ _ _ ha
    exact ⟨d, d', h₁', h₂', h₃'.mono (World.le_addD _ _ _ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := h.j₁
  j₂ := h.j₂
  own := h.own
  ownU := h.ownU
  hl := h.hl
  hr := h.hr
  hrel := by
    intro a a' hH
    obtain ⟨hB, e, e', hl₀, hr₀, hs₀⟩ := h.hrel a a' hH
    exact ⟨hB, e, e', hl₀, hr₀, hs₀.mono (World.le_addD _ _ _ _ _)⟩
  done := by
    rintro a a' x x' (hd | ⟨rfl, rfl, rfl, rfl⟩)
    · obtain ⟨ha, hv₀, hv₀', hcv₀, hcv₀', hnv₀', h₁', h₂', hvv₀⟩ := h.done a a' x x' hd
      exact ⟨ha, hv₀, hv₀', hcv₀, hcv₀', hnv₀', h₁', h₂', hvv₀.mono (World.le_addD _ _ _ _ _)⟩
    · exact ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv.mono (World.le_addD _ _ _ _ _)⟩

/-- A new content of a left cell only the left side has. -/
theorem StR.setJ₁ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {n : Nat} (hn : W.J₁ n) (c : Tm) :
    StR W (μ₁.set n c) μ₂ where
  cell := by
    intro a a' k ha
    have he : a ≠ n := fun e => (h.j₁ n hn).2 a' k (e ▸ ha)
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ ha
    exact ⟨d, d', by rw [List.getElem?_set_ne (Ne.symm he)]; exact h₁, h₂, h₃⟩
  bij := h.bij
  kd := h.kd
  j₁ := fun n hn => by simpa using h.j₁ n hn
  j₂ := h.j₂
  own := h.own
  ownU := h.ownU
  hl := by
    intro a e ha
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d, hd, hst⟩ := h.hl a e ha
    have hne : a ≠ n := fun he => hnj (he ▸ hn)
    exact ⟨hB, hu, hnl, hnj, hcl, hpe, d, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hd, hst⟩
  hr := h.hr
  hrel := h.hrel
  done := by
    intro a a' v v' hd
    obtain ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done a a' v v' hd
    have he : a ≠ n := fun e => (h.j₁ n hn).2 a' .sym (e ▸ ha)
    exact ⟨ha, hv, hv', hcv, hcv', hnv', by rw [List.getElem?_set_ne (Ne.symm he)]; exact h₁, h₂, hvv⟩

/-- A new content of a right cell only the right side has. -/
theorem StR.setJ₂ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {n : Nat} (hn : W.J₂ n) (c : Tm) :
    StR W μ₁ (μ₂.set n c) where
  cell := by
    intro a a' k ha
    have he : a' ≠ n := fun e => (h.j₂ n hn).2 a k (e ▸ ha)
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ ha
    exact ⟨d, d', h₁, by rw [List.getElem?_set_ne (Ne.symm he)]; exact h₂, h₃⟩
  bij := h.bij
  kd := h.kd
  j₁ := h.j₁
  j₂ := fun n hn => by simpa using h.j₂ n hn
  own := h.own
  ownU := h.ownU
  hl := h.hl
  hr := by
    intro a' e' ha'
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d', hd', hst⟩ := h.hr a' e' ha'
    have hne : a' ≠ n := fun he => hnj (he ▸ hn)
    exact ⟨hB, hu, hnl, hnj, hcl, hpe, d', by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hd', hst⟩
  hrel := h.hrel
  done := by
    intro a a' v v' hd
    obtain ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done a a' v v' hd
    have he : a' ≠ n := fun e => (h.j₂ n hn).2 a .sym (e ▸ ha)
    exact ⟨ha, hv, hv', hcv, hcv', hnv', h₁, by rw [List.getElem?_set_ne (Ne.symm he)]; exact h₂, hvv⟩

theorem settleCell_cases (μ : List Tm) (l : Nat) (o : Tm) :
    (∃ e, μ[l]? = some (.running e) ∧ settleCell μ l o = μ.set l (.done o)) ∨
      ((∀ e, μ[l]? ≠ some (.running e)) ∧ settleCell μ l o = μ) := by
  unfold settleCell
  split
  · rename_i h; obtain ⟨e, he⟩ := h; exact .inl ⟨e, he, rfl⟩
  · rename_i h; exact .inr ⟨fun e he => h ⟨e, he⟩, rfl⟩

theorem StR.settleJ₁ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {n : Nat} (hn : W.J₁ n) (o : Tm) :
    StR W (settleCell μ₁ n o) μ₂ := by
  rcases settleCell_cases μ₁ n o with ⟨_, _, he⟩ | ⟨_, he⟩ <;> rw [he]
  · exact h.setJ₁ hn _
  · exact h

theorem StR.resetJ₁ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {n : Nat} (hn : W.J₁ n) :
    StR W (resetCell μ₁ n) μ₂ := by
  rcases resetCell_cases μ₁ n with ⟨_, _, he⟩ | ⟨_, he⟩ <;> rw [he]
  · exact h.setJ₁ hn _
  · exact h

theorem StR.settleJ₂ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {n : Nat} (hn : W.J₂ n) (o : Tm) :
    StR W μ₁ (settleCell μ₂ n o) := by
  rcases settleCell_cases μ₂ n o with ⟨_, _, he⟩ | ⟨_, he⟩ <;> rw [he]
  · exact h.setJ₂ hn _
  · exact h

theorem StR.resetJ₂ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {n : Nat} (hn : W.J₂ n) :
    StR W μ₁ (resetCell μ₂ n) := by
  rcases resetCell_cases μ₂ n with ⟨_, _, he⟩ | ⟨_, he⟩ <;> rw [he]
  · exact h.setJ₂ hn _
  · exact h

/-- A cell related at `top` is running exactly when the other is. -/
theorem running_iff {c c' : Tm} (h : Sim W .top c c') :
    (∃ e, c = .running e) ↔ ∃ e', c' = .running e' := by
  constructor
  · rintro ⟨e, rfl⟩
    generalize hx : Pos.top = x at h
    cases h with
    | force _ => exact ⟨_, rfl⟩
    | jR hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  · rintro ⟨e', rfl⟩
    generalize hx : Pos.top = x at h
    cases h with
    | force _ => exact ⟨_, rfl⟩
    | jL hx' => exact (hx' hx.symm).elim
    | dead => cases hx
    | _ => first | cases hx | rename_i h; exact (h rfl).elim

end Flick.Equiv
