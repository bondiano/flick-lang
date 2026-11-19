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

theorem World.le_addL (W : World) (l l' : Nat) (k : Kd) : W.le (W.addL l l' k) :=
  ⟨fun _ _ _ h => .inl h, fun _ h => h, fun _ h => h, fun _ _ h => h, Iff.rfl⟩

theorem World.le_addJ₁ (W : World) (n : Nat) : W.le (W.addJ₁ n) :=
  ⟨fun _ _ _ h => h, fun _ h => .inl h, fun _ h => h, fun _ _ h => h, Iff.rfl⟩

theorem World.le_addJ₂ (W : World) (n : Nat) : W.le (W.addJ₂ n) :=
  ⟨fun _ _ _ h => h, fun _ h => h, fun _ h => .inl h, fun _ _ h => h, Iff.rfl⟩

theorem World.le_addO (W : World) (n : Nat) (τ : Option (Nat × Nat)) : W.le (W.addO n τ) :=
  ⟨fun _ _ _ h => h, fun _ h => .inl h, fun _ h => h, fun _ _ h => .inl h, Iff.rfl⟩

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

/-- A new content of a related pair of cells. -/
theorem StR.set {μ₁ μ₂ : List Tm} (h : StR W μ₁ μ₂) {l l' : Nat} {k : Kd} (hl : W.L l l' k)
    {c c' : Tm} (hc : CellR W k c c') : StR W (μ₁.set l c) (μ₂.set l' c') where
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
