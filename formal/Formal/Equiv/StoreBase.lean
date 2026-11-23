import Formal.Equiv.Park

/-!
# Contextual equivalence: related stores

Two stores are related in a world (`StR`): the related locations are a partial bijection with
one kind each, their cells are related by their kind (`CellR`), and the cells only one side has
are its own. A `sym` pair holds cells related at `top`, so a pending, running or done cell is
related only to one in the same state: the two cells change together. A `born` pair (O1b) holds a
left cell pending, running or done with `v` and a right one pending or done with `v'` (`BornInv`).

A world grows by a related pair of new cells (`World.addL`), by a new cell of one side
(`World.addJ₁`, `World.addJ₂`), and by a new left cell a thread forces at its next step
(`World.addO`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- The cells of a `born v v'` pair (O1b). -/
structure BornInv (W : World) (v v' c c' : Tm) : Prop where
  val : Value v
  val' : Value v'
  cl : v.Closed
  cl' : v'.Closed
  noIns : v'.NoIns
  rel : Sim W .any v v'
  state : ((c = .lazy v ∨ c = .running v ∨ c = .done v) ∧ (c' = .lazy v' ∨ c' = .done v')) ∨
    (¬ c.IsCell ∧ ¬ c'.IsCell ∧ Sim W .top c c')
  flag : W.B

/-- Related cells of a kind. -/
def CellR (W : World) : Kd → Tm → Tm → Prop
  | .sym, c, c' => Sim W .top c c'
  | .born v v', c, c' => BornInv W v v' c c'

/-- Related stores. -/
structure StR (W : World) (μ₁ μ₂ : List Tm) : Prop where
  cell : ∀ l l' k, W.L l l' k → ∃ c c', μ₁[l]? = some c ∧ μ₂[l']? = some c' ∧ CellR W k c c'
  bij : W.Bij
  kd : ∀ l l₁ l₂ k₁ k₂, W.L l l₁ k₁ → W.L l l₂ k₂ → k₁ = k₂
  j₁ : ∀ n, W.J₁ n → n < μ₁.length ∧ ∀ l' k, ¬ W.L n l' k
  j₂ : ∀ n, W.J₂ n → n < μ₂.length ∧ ∀ l k, ¬ W.L l n k
  own : ∀ n τ, W.O n τ → W.J₁ n
  ownU : ∀ n τ τ', W.O n τ → W.O n τ' → τ = τ'
  hl : ∀ l e, W.HL l e →
    W.B ∧ (∀ e₀, W.HL l e₀ → e₀ = e) ∧ (∀ l' k, ¬ W.L l l' k) ∧ ¬ W.J₁ l ∧
    e.Closed ∧ PureEval W.R e ∧
    ∃ c, μ₁[l]? = some c ∧ (c = .lazy e ∨ c = .running e ∨ ∃ v, c = .done v ∧ Value v ∧ PSteps W.R e v)
  hr : ∀ l' e', W.HR l' e' →
    W.B ∧ (∀ e₀', W.HR l' e₀' → e₀' = e') ∧ (∀ l k, ¬ W.L l l' k) ∧ ¬ W.J₂ l' ∧
    e'.Closed ∧ PureEval W.R e' ∧
    ∃ c', μ₂[l']? = some c' ∧ (c' = .lazy e' ∨ ∃ v', c' = .done v' ∧ Value v' ∧ PSteps W.R e' v')
  hrel : ∀ l l', W.H l l' → W.B ∧ ∃ e e', W.HL l e ∧ W.HR l' e' ∧ Sim W .any e e'
  done : ∀ l l' v v', W.D l l' v v' →
    W.L l l' .sym ∧ Value v ∧ Value v' ∧ v.Closed ∧ v'.Closed ∧ v'.NoIns ∧
    μ₁[l]? = some (.done v) ∧ μ₂[l']? = some (.done v') ∧ Sim W .any v v'

theorem BornInv.mono {W' : World} (hW : W.le W') {v v' c c' : Tm} (h : BornInv W v v' c c') :
    BornInv W' v v' c c' := by
  obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆, h₇, h₈⟩ := h
  refine ⟨h₁, h₂, h₃, h₄, h₅, h₆.mono hW, ?_, hW.B.1 h₈⟩
  rcases h₇ with h₇ | ⟨a, b, c⟩
  · exact .inl h₇
  · exact .inr ⟨a, b, c.mono hW⟩

theorem CellR.mono {W' : World} (hW : W.le W') {k : Kd} {c c' : Tm} (h : CellR W k c c') :
    CellR W' k c c' := by
  cases k with
  | sym => exact Sim.mono hW h
  | born v v' => exact BornInv.mono hW h

theorem StR.lt₁ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {k : Kd} (hl : W.L l l' k) :
    l < μ₁.length := by
  obtain ⟨c, c', h₁, -, -⟩ := h.cell _ _ _ hl
  exact (List.getElem?_eq_some_iff.mp h₁).1

theorem StR.lt₂ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {k : Kd} (hl : W.L l l' k) :
    l' < μ₂.length := by
  obtain ⟨c, c', -, h₂, -⟩ := h.cell _ _ _ hl
  exact (List.getElem?_eq_some_iff.mp h₂).1

/-- The world with a new related pair. -/
def World.addL (W : World) (l l' : Nat) (k : Kd) : World :=
  { W with L := fun a b k' => W.L a b k' ∨ (a = l ∧ b = l' ∧ k' = k) }

/-- The world with a new left cell. -/
def World.addJ₁ (W : World) (n : Nat) : World := { W with J₁ := fun a => W.J₁ a ∨ a = n }

/-- The world with a new right cell. -/
def World.addJ₂ (W : World) (n : Nat) : World := { W with J₂ := fun a => W.J₂ a ∨ a = n }

/-- The world with a new left cell the thread `τ` forces at its next step. -/
def World.addO (W : World) (n : Nat) (τ : Option (Nat × Nat)) : World :=
  { W with J₁ := fun a => W.J₁ a ∨ a = n, O := fun a τ' => W.O a τ' ∨ (a = n ∧ τ' = τ) }

/-- The world with a new done `sym` cell recorded for O2a. -/
def World.addD (W : World) (l l' : Nat) (v v' : Tm) : World :=
  { W with D := fun a b x y => W.D a b x y ∨ (a = l ∧ b = l' ∧ x = v ∧ y = v') }

theorem World.le_addL (W : World) (l l' : Nat) (k : Kd) : W.le (W.addL l l' k) :=
  ⟨rfl, fun _ _ _ h => .inl h, fun _ h => h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le_addJ₁ (W : World) (n : Nat) : W.le (W.addJ₁ n) :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => .inl h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le_addJ₂ (W : World) (n : Nat) : W.le (W.addJ₂ n) :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => h, fun _ h => .inl h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le_addO (W : World) (n : Nat) (τ : Option (Nat × Nat)) : W.le (W.addO n τ) :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => .inl h, fun _ h => h, fun _ _ h => .inl h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le_addD (W : World) (l l' : Nat) (v v' : Tm) : W.le (W.addD l l' v v') :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => .inl h, Iff.rfl⟩

theorem StR.ltHL {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l : Nat} {e : Tm} (hl : W.HL l e) :
    l < μ₁.length := by
  obtain ⟨-, -, -, -, -, -, c, hc, -⟩ := h.hl l e hl
  exact (List.getElem?_eq_some_iff.mp hc).1

theorem StR.ltHR {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l' : Nat} {e' : Tm} (hr : W.HR l' e') :
    l' < μ₂.length := by
  obtain ⟨-, -, -, -, -, -, c', hc', -⟩ := h.hr l' e' hr
  exact (List.getElem?_eq_some_iff.mp hc').1

theorem getElem?_append_old {μ : List Tm} {c : Tm} {l : Nat} (h : l < μ.length) :
    (μ ++ [c])[l]? = μ[l]? := List.getElem?_append_left h

/-- **Allocation of a related pair** of new cells. -/
theorem StR.alloc {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {k : Kd} {c c' : Tm}
    (hc : CellR (W.addL μ₁.length μ₂.length k) k c c') :
    StR (W.addL μ₁.length μ₂.length k) (μ₁ ++ [c]) (μ₂ ++ [c']) where
  cell := by
    rintro l l' k' (hl | ⟨rfl, rfl, rfl⟩)
    · obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ hl
      refine ⟨d, d', ?_, ?_, h₃.mono (World.le_addL _ _ _ _)⟩
      · rw [getElem?_append_old (h.lt₁ hl)]; exact h₁
      · rw [getElem?_append_old (h.lt₂ hl)]; exact h₂
    · exact ⟨c, c', by simp, by simp, hc⟩
  bij := by
    rintro a a' b b' k₁ k₂ (ha | ⟨rfl, rfl, rfl⟩) (hb | ⟨rfl, rfl, rfl⟩)
    · exact h.bij _ _ _ _ _ _ ha hb
    · have := h.lt₁ ha; have := h.lt₂ ha; constructor <;> intro <;> omega
    · have := h.lt₁ hb; have := h.lt₂ hb; constructor <;> intro <;> omega
    · exact ⟨fun _ => rfl, fun _ => rfl⟩
  kd := by
    rintro l l₁ l₂ k₁ k₂ (h₁ | ⟨rfl, rfl, rfl⟩) (h₂ | ⟨h₃, -, rfl⟩)
    · exact h.kd _ _ _ _ _ h₁ h₂
    · exact absurd (h₃ ▸ h.lt₁ h₁) (Nat.lt_irrefl _)
    · exact absurd (h.lt₁ h₂) (Nat.lt_irrefl _)
    · rfl
  j₁ := by
    intro n hn
    obtain ⟨h₁, h₂⟩ := h.j₁ n hn
    refine ⟨by simp; omega, ?_⟩
    rintro l' k' (h' | ⟨rfl, -, -⟩)
    · exact h₂ _ _ h'
    · omega
  j₂ := by
    intro n hn
    obtain ⟨h₁, h₂⟩ := h.j₂ n hn
    refine ⟨by simp; omega, ?_⟩
    rintro l k' (h' | ⟨-, rfl, -⟩)
    · exact h₂ _ _ h'
    · omega
  own := h.own
  ownU := h.ownU
  hl := by
    intro l e hl
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d, hd, hst⟩ := h.hl l e hl
    have hlt := h.ltHL hl
    refine ⟨hB, hu, ?_, hnj, hcl, hpe, d, by rw [getElem?_append_old hlt]; exact hd, hst⟩
    rintro l' k' (h' | ⟨rfl, -, -⟩)
    · exact hnl _ _ h'
    · omega
  hr := by
    intro l' e' hr
    obtain ⟨hB, hu, hnl, hnj, hcl, hpe, d', hd', hst⟩ := h.hr l' e' hr
    have hlt := h.ltHR hr
    refine ⟨hB, hu, ?_, hnj, hcl, hpe, d', by rw [getElem?_append_old hlt]; exact hd', hst⟩
    rintro l k' (h' | ⟨-, rfl, -⟩)
    · exact hnl _ _ h'
    · omega
  hrel := by
    intro l l' hH
    obtain ⟨hB, e, e', hl, hr, hs⟩ := h.hrel l l' hH
    exact ⟨hB, e, e', hl, hr, hs.mono (World.le_addL _ _ _ _)⟩
  done := by
    intro l l' v v' hd
    obtain ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done l l' v v' hd
    exact ⟨.inl hl, hv, hv', hcv, hcv', hnv', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁,
      by rw [getElem?_append_old (h.lt₂ hl)]; exact h₂, hvv.mono (World.le_addL _ _ _ _)⟩

/-- **Allocation of a left cell** only the left side has. -/
theorem StR.allocJ₁ {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) (c : Tm) :
    StR (W.addJ₁ μ₁.length) (μ₁ ++ [c]) μ₂ where
  cell := by
    intro l l' k hl
    obtain ⟨d, d', h₁, h₂, h₃⟩ := h.cell _ _ _ hl
    exact ⟨d, d', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁, h₂, h₃.mono (World.le_addJ₁ _ _)⟩
  bij := h.bij
  kd := h.kd
  j₁ := by
    rintro n (hn | rfl)
    · obtain ⟨h₁, h₂⟩ := h.j₁ n hn; exact ⟨by simp; omega, h₂⟩
    · exact ⟨by simp, fun l' k hl => absurd (h.lt₁ hl) (Nat.lt_irrefl _)⟩
  j₂ := h.j₂
  own := fun n τ ho => .inl (h.own n τ ho)
  ownU := h.ownU
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
    exact ⟨hB, e, e', hl, hr, hs.mono (World.le_addJ₁ _ _)⟩
  done := by
    intro l l' v v' hd
    obtain ⟨hl, hv, hv', hcv, hcv', hnv', h₁, h₂, hvv⟩ := h.done l l' v v' hd
    exact ⟨hl, hv, hv', hcv, hcv', hnv', by rw [getElem?_append_old (h.lt₁ hl)]; exact h₁, h₂,
      hvv.mono (World.le_addJ₁ _ _)⟩


end Flick.Equiv
