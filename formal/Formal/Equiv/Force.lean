import Formal.Equiv.Cells

/-!
# Contextual equivalence: the answer to a runtime head step, and forces

`HAns` is how the right side answers a runtime head step of the left one: it catches up, takes
head steps at its top, and the results, the stores and the world are related; the cells only the
left side has stay. A force of a related cell is answered by a force (`force_sim`): of a `sym`
pair, by the same step; of a `born` pair (O1b), by the force of the other state of the cell, or by
nothing, when the left cell is running and the left thread is stuck.
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

/-- The answer of the right side to a runtime head step of the left one. -/
def HAns (W : World) (μ₂ : List Tm) (r₂ : Tm) (μ μ' : List Tm) (r' : Tm) : Prop :=
  ∃ W₁ μ₂a r₂a W' μ₂' r₂', RJ W μ₂ r₂ W₁ μ₂a r₂a ∧ HSeq μ₂a r₂a μ₂' r₂' ∧ W₁.le W' ∧
    Sim W' .any r' r₂' ∧ StR W' μ' μ₂' ∧ ∀ n, W.J₁ n → μ'[n]? = μ[n]?

theorem rcu_frame {x : Pos} {a b v : Tm} (h : Sim W x a b) (hx : x = .any) (ha : a = .raise v)
    (hv : Value v) (μ : List Tm) {F : Frame} (hF : F.ok) :
    ∃ W' μ' v', RJ W μ (F.plug b) W' μ' (F.plug (.raise v')) ∧ Value v' ∧ Sim W' .any v v' := by
  obtain ⟨W', μ', v', r, hv', s⟩ := h.rcu hx ha hv W μ (World.le.refl _)
  exact ⟨W', μ', v', r.frame hF, hv', s⟩

theorem ucu_frame {x : Pos} {a b : Tm} {p ℓ : Nat} {c : Tm} (h : Sim W x a b) (hx : x = .any)
    (ha : a = .unw p ℓ c) (μ : List Tm) {F : Frame} (hF : F.ok) :
    ∃ W' μ' ℓ' k c', RJ W μ (F.plug b) W' μ' (F.plug (.unw p ℓ' c')) ∧ W'.L ℓ ℓ' k ∧
      Sim W' .any c c' := by
  obtain ⟨W', μ', ℓ', k, c', r, hl, s⟩ := h.ucu hx ha W μ (World.le.refl _)
  exact ⟨W', μ', ℓ', k, c', r.frame hF, hl, s⟩

/-- The location under a force, after the right side catches up. -/
theorem force_loc {l : Nat} {e₂ : Tm} (h : Sim W .any (.loc l) e₂) (μ : List Tm) :
    ∃ W' μ' l' k, RJ W μ (.force e₂) W' μ' (.force (.loc l')) ∧ W'.L l l' k := by
  obtain ⟨W', μ', v', r, hv', s⟩ := h.vcu rfl .loc W μ (World.le.refl _)
  obtain ⟨l', k, rfl, hl⟩ := (s.value_any .loc hv' .top).inv_loc
  exact ⟨W', μ', l', k, r.frame (F := .force) trivial, hl⟩

theorem settleCell_running {μ : List Tm} {l : Nat} {e o : Tm} (h : l < μ.length) :
    settleCell (μ.set l (.running e)) l o = μ.set l (.done o) := by
  rcases settleCell_cases (μ.set l (.running e)) l o with ⟨_, _, e₁⟩ | ⟨hn, _⟩
  · rw [e₁, List.set_set]
  · exact absurd (by simp [h]) (hn e)

/-- A force of a related cell. -/
theorem force_sim {Λ : List Rgn} {μ μ' μ₂ : List Tm} {l : Nat} {r' e₂ : Tm}
    (hh : RHead Λ μ (.force (.loc l)) Λ μ' r') (h : Sim W .any (.loc l) e₂) (hst : StR W μ μ₂) :
    HAns W μ₂ (.force e₂) μ μ' r' := by
  obtain ⟨W₁, μ₁, l', k, r₁, hlk⟩ := force_loc h μ₂
  have hst₁ := r₁.st _ hst
  have hnj : ∀ n, W.J₁ n → n ≠ l := fun n hn e => hst₁.not_j₁ hlk (e ▸ r₁.le.J₁ _ hn)
  obtain ⟨c, c', hc, hc', hcr⟩ := hst₁.cell _ _ _ hlk
  have hl' : l' < μ₁.length := hst₁.lt₂ hlk
  cases k with
  | sym =>
    have hcr : Sim W₁ .top c c' := hcr
    cases hh with
    | forcePending hc₀ =>
      rw [hc₀] at hc; cases hc
      obtain ⟨e', rfl, he'⟩ := hcr.inv_lazy
      exact ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .forcePending hc'), World.le.refl _,
        .settle hlk he', hst₁.set hlk (.force he'), fun n hn => getElem?_set_of_ne (hnj n hn)⟩
    | forceDone hc₀ =>
      rw [hc₀] at hc; cases hc
      obtain ⟨o', rfl, ho'⟩ := hcr.inv_await
      exact ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .forceDone hc'), World.le.refl _, ho', hst₁,
        fun _ _ => rfl⟩
    | forceRunning hc₀ hnl hnd =>
      rw [hc₀] at hc; cases hc
      have hsh := hcr.top_cell rfl
      refine ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .forceRunning hc' ?_ ?_), World.le.refl _, .todo,
        hst₁, fun _ _ => rfl⟩
      · intro e he; obtain ⟨e₀, he₀⟩ := hsh.1.2 ⟨e, he⟩; exact hnl e₀ he₀
      · intro o ho; obtain ⟨o₀, ho₀⟩ := hsh.2.2.2 ⟨o, ho⟩; exact hnd o₀ ho₀
  | born v v' =>
    obtain ⟨hv, hv', hcl, hcl', hn', hvv, hs⟩ := hcr
    have hinv : ∀ {c c' : Tm}, ((c = .lazy v ∨ c = .running v ∨ c = .done v) ∧
        (c' = .lazy v' ∨ c' = .done v')) → CellR W₁ (.born v v') c c' :=
      fun hcs => ⟨hv, hv', hcl, hcl', hn', hvv, .inl hcs⟩
    -- the right side reaches `v'`, its cell done
    have reach : (c' = .lazy v' ∨ c' = .done v') →
        ∃ μ₂', HSeq μ₁ (.force (.loc l')) μ₂' v' ∧ μ₂' = μ₁.set l' (.done v') := by
      intro hc''
      rcases hc'' with rfl | rfl
      · refine ⟨_, .rt (fun _ => .forcePending hc') (.rt (fun _ => .settleVal hv') .refl),
          settleCell_running hl'⟩
      · exact ⟨_, HSeq.one (fun _ => .forceDone hc'), (Flick.Elab.List.set_of_getElem? hc').symm⟩
    cases hh with
    | forcePending hc₀ =>
      rw [hc₀] at hc; cases hc
      rcases hs with ⟨hl₁ | hl₁ | hl₁, hr₁⟩ | ⟨hnc, -, -⟩
      · cases hl₁
        obtain ⟨μ₂', hq, rfl⟩ := reach hr₁
        exact ⟨W₁, μ₁, _, W₁, _, _, r₁, hq, World.le.refl _, .bornV hlk hcl hcl' hn',
          hst₁.set hlk (hinv ⟨.inr (.inl rfl), .inr rfl⟩), fun n hn => getElem?_set_of_ne (hnj n hn)⟩
      · cases hl₁
      · cases hl₁
      · exact (hnc trivial).elim
    | forceDone hc₀ =>
      rw [hc₀] at hc; cases hc
      rcases hs with ⟨hl₁ | hl₁ | hl₁, hr₁⟩ | ⟨hnc, -, -⟩
      · cases hl₁
      · cases hl₁
      · cases hl₁
        obtain ⟨μ₂', hq, rfl⟩ := reach hr₁
        have hμ : μ = μ.set l _ := (Flick.Elab.List.set_of_getElem? hc₀).symm
        refine ⟨W₁, μ₁, _, W₁, _, _, r₁, hq, World.le.refl _, hvv, ?_, fun _ _ => rfl⟩
        rw [hμ]; exact hst₁.set hlk (hinv ⟨.inr (.inr rfl), .inr rfl⟩)
      · exact (hnc trivial).elim
    | forceRunning hc₀ hnl hnd =>
      rw [hc₀] at hc; cases hc
      rcases hs with ⟨hl₁ | hl₁ | hl₁, hr₁⟩ | ⟨hnc, hnc', -⟩
      · exact (hnl _ hl₁).elim
      · exact ⟨W₁, μ₁, _, W₁, _, _, r₁, .refl, World.le.refl _,
          .dead (by simp [Tm.NoIns, Tm.Has]), hst₁, fun _ _ => rfl⟩
      · exact (hnd _ hl₁).elim
      · refine ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .forceRunning hc' ?_ ?_), World.le.refl _, .todo,
          hst₁, fun _ _ => rfl⟩
        · rintro e rfl; exact hnc' trivial
        · rintro o rfl; exact hnc' trivial

end Flick.Equiv
