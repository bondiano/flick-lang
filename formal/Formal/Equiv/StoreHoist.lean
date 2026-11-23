import Formal.Equiv.Store

/-!
# Contextual equivalence: store updates for hoisted pure lazy cells (O4)
-/

namespace Flick.Equiv

variable {W : World}

/-- The world with a new hoisted pure left cell. -/
def World.addHL (W : World) (l : Nat) (e : Tm) : World :=
  { W with HL := fun a b => W.HL a b ∨ (a = l ∧ b = e) }

/-- The world with a new hoisted pure right cell. -/
def World.addHR (W : World) (l' : Nat) (e' : Tm) : World :=
  { W with HR := fun a b => W.HR a b ∨ (a = l' ∧ b = e') }

/-- The world with a new hoisted pure cell correspondence `H l l'`. -/
def World.addH (W : World) (l l' : Nat) : World :=
  { W with H := fun a b => W.H a b ∨ (a = l ∧ b = l') }

theorem World.le_addHL (W : World) (l : Nat) (e : Tm) : W.le (W.addHL l e) :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => .inl h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le_addHR (W : World) (l' : Nat) (e' : Tm) : W.le (W.addHR l' e') :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => .inl h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le_addH (W : World) (l l' : Nat) : W.le (W.addH l l') :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => .inl h, fun _ _ _ _ h => h, Iff.rfl⟩

/-- **Allocation of a hoisted pure left cell.** -/
theorem StR.allocHL {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {e : Tm}
    (hB : W.B) (hcl : e.Closed) (hpe : PureEval W.R e) :
    StR (W.addHL μ₁.length e) (μ₁ ++ [.lazy e]) μ₂ where
  cell := by
    intro l l' k hl
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ hl
    exact ⟨d, d', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁, h₂, h₃.mono (World.le_addHL _ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := by
    intro n hn; obtain ⟨h₁, h₂⟩ := h.j₁ n hn; exact ⟨by simp; omega, h₂⟩
  j₂ := h.j₂
  own := h.own
  ownU := h.ownU
  hl := by
    rintro l e₀ (hl | ⟨rfl, rfl⟩)
    · obtain ⟨hB', hu, hnl, hnj, hcl', hpe', d, hd, hst⟩ := h.hl l e₀ hl
      have hlt := h.ltHL hl
      refine ⟨hB', ?_, hnl, hnj, hcl', hpe', d, by rw [getElem?_append_old hlt]; exact hd, hst⟩
      rintro e₁ (hl₁ | ⟨h₁, -⟩)
      · exact hu e₁ hl₁
      · omega
    · refine ⟨hB, ?_, fun l' k hl => absurd (h.lt₁ hl) (Nat.lt_irrefl _),
        fun hj => absurd (h.j₁ _ hj).1 (Nat.lt_irrefl _), hcl, hpe, .lazy e₀, by simp, .inl rfl⟩
      rintro e₁ (hl₁ | ⟨-, rfl⟩)
      · exact absurd (h.ltHL hl₁) (Nat.lt_irrefl _)
      · rfl
  hr := h.hr
  hrel := by
    intro l l' hH
    obtain ⟨hB', e₀, e₀', hl, hr, hs⟩ := h.hrel l l' hH
    exact ⟨hB', e₀, e₀', .inl hl, hr, hs.mono (World.le_addHL _ _ _)⟩
  done := by
    intro l l' v v' hd
    obtain ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done l l' v v' hd
    exact ⟨hl, hv, hv', hcv, hcv', hnv', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁, h₂,
      hvv.mono (World.le_addHL _ _ _)⟩

/-- **Allocation of a hoisted pure right cell.** -/
theorem StR.allocHR {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {e' : Tm}
    (hB : W.B) (hcl : e'.Closed) (hpe : PureEval W.R e') :
    StR (W.addHR μ₂.length e') μ₁ (μ₂ ++ [.lazy e']) where
  cell := by
    intro l l' k hl
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ hl
    exact ⟨d, d', h₁, by rw [getElem?_append_old (h.lt₂ hl)]; exact h₂, h₃.mono (World.le_addHR _ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := h.j₁
  j₂ := by
    intro n hn; obtain ⟨h₁, h₂⟩ := h.j₂ n hn; exact ⟨by simp; omega, h₂⟩
  own := h.own
  ownU := h.ownU
  hl := h.hl
  hr := by
    rintro l' e₀' (hr | ⟨rfl, rfl⟩)
    · obtain ⟨hB', hu, hnl, hnj, hcl', hpe', d', hd', hst⟩ := h.hr l' e₀' hr
      have hlt := h.ltHR hr
      refine ⟨hB', ?_, hnl, hnj, hcl', hpe', d', by rw [getElem?_append_old hlt]; exact hd', hst⟩
      rintro e₁' (hr₁ | ⟨h₁, -⟩)
      · exact hu e₁' hr₁
      · omega
    · refine ⟨hB, ?_, fun l k hl => absurd (h.lt₂ hl) (Nat.lt_irrefl _),
        fun hj => absurd (h.j₂ _ hj).1 (Nat.lt_irrefl _), hcl, hpe, .lazy e₀', by simp, .inl rfl⟩
      rintro e₁' (hr₁ | ⟨-, rfl⟩)
      · exact absurd (h.ltHR hr₁) (Nat.lt_irrefl _)
      · rfl
  hrel := by
    intro l l' hH
    obtain ⟨hB', e₀, e₀', hl, hr, hs⟩ := h.hrel l l' hH
    exact ⟨hB', e₀, e₀', hl, .inl hr, hs.mono (World.le_addHR _ _ _)⟩
  done := by
    intro l l' v v' hd
    obtain ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done l l' v v' hd
    exact ⟨hl, hv, hv', hcv, hcv', hnv', h₁, by rw [getElem?_append_old (h.lt₂ hl)]; exact h₂,
      hvv.mono (World.le_addHR _ _ _)⟩

/-- **Linking two hoisted pure cells** (many-to-one or one-to-many). -/
theorem StR.linkH {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {e e' : Tm}
    (hB : W.B) (hl : W.HL l e) (hr : W.HR l' e') (hs : Sim W .any e e') :
    StR (W.addH l l') μ₁ μ₂ where
  cell := by
    intro a a' k ha
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ ha
    exact ⟨d, d', h₁, h₂, h₃.mono (World.le_addH _ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := h.j₁
  j₂ := h.j₂
  own := h.own
  ownU := h.ownU
  hl := h.hl
  hr := h.hr
  hrel := by
    rintro a a' (hH | ⟨rfl, rfl⟩)
    · obtain ⟨hB', e₀, e₀', hl₀, hr₀, hs₀⟩ := h.hrel a a' hH
      exact ⟨hB', e₀, e₀', hl₀, hr₀, hs₀.mono (World.le_addH _ _ _)⟩
    · exact ⟨hB, e, e', hl, hr, hs.mono (World.le_addH _ _ _)⟩
  done := by
    intro a a' v v' hd
    obtain ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done a a' v v' hd
    exact ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv.mono (World.le_addH _ _ _)⟩

/-- Updating the state of a hoisted pure left cell. -/
theorem StR.setHL {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l : Nat} {e c : Tm} (hl : W.HL l e)
    (hc : c = .lazy e ∨ c = .running e ∨ ∃ v, c = .done v ∧ Value v ∧ PSteps W.R e v) :
    StR W (μ₁.set l c) μ₂ where
  cell := by
    intro a a' k ha
    have hne : a ≠ l := fun he => (h.hl l e hl).2.2.1 a' k (he ▸ ha)
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ ha
    exact ⟨d, d', by rw [List.getElem?_set_ne (Ne.symm hne)]; exact h₁, h₂, h₃⟩
  bij := h.bij
  kd := h.kd
  j₁ := fun n hn => by simpa using h.j₁ n hn
  j₂ := h.j₂
  own := h.own
  ownU := h.ownU
  hl := by
    intro a e₀ ha
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d, hd, hst⟩ := h.hl a e₀ ha
    by_cases he : a = l
    · subst he
      obtain rfl := (h.hl a e hl).2.1 e₀ ha
      exact ⟨hB, hu, hnl, hnj, hcl, hpe, c, by simp [h.ltHL hl], hc⟩
    · exact ⟨hB, hu, hnl, hnj, hcl, hpe, d, by rw [List.getElem?_set_ne (Ne.symm he)]; exact hd, hst⟩
  hr := h.hr
  hrel := h.hrel
  done := by
    intro a a' v v' hd
    obtain ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done a a' v v' hd
    have hne : a ≠ l := fun he => (h.hl l e hl).2.2.1 a' .sym (he ▸ ha)
    exact ⟨ha, hv, hv', hcv, hcv', hnv', by rw [List.getElem?_set_ne (Ne.symm hne)]; exact h₁, h₂, hvv⟩

/-- Updating the state of a hoisted pure right cell. -/
theorem StR.setHR {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l' : Nat} {e' c' : Tm} (hr : W.HR l' e')
    (hc' : c' = .lazy e' ∨ ∃ v', c' = .done v' ∧ Value v' ∧ PSteps W.R e' v') :
    StR W μ₁ (μ₂.set l' c') where
  cell := by
    intro a a' k ha
    have hne : a' ≠ l' := fun he => (h.hr l' e' hr).2.2.1 a k (he ▸ ha)
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ ha
    exact ⟨d, d', h₁, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact h₂, h₃⟩
  bij := h.bij
  kd := h.kd
  j₁ := h.j₁
  j₂ := fun n hn => by simpa using h.j₂ n hn
  own := h.own
  ownU := h.ownU
  hl := h.hl
  hr := by
    intro a' e₀' ha'
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d', hd', hst⟩ := h.hr a' e₀' ha'
    by_cases he : a' = l'
    · subst he
      obtain rfl := (h.hr a' e' hr).2.1 e₀' ha'
      exact ⟨hB, hu, hnl, hnj, hcl, hpe, c', by simp [h.ltHR hr], hc'⟩
    · exact ⟨hB, hu, hnl, hnj, hcl, hpe, d', by rw [List.getElem?_set_ne (Ne.symm he)]; exact hd', hst⟩
  hrel := h.hrel
  done := by
    intro a a' v v' hd
    obtain ⟨ha, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done a a' v v' hd
    have hne : a' ≠ l' := fun he => (h.hr l' e' hr).2.2.1 a .sym (he ▸ ha)
    exact ⟨ha, hv, hv', hcv, hcv', hnv', h₁, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact h₂, hvv⟩


end Flick.Equiv
