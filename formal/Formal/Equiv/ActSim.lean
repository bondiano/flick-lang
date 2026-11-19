import Formal.Equiv.Act

/-!
# Contextual equivalence: a step of a special pair

`act_sim`: the left side of a special pair steps by its redex; the right side waits, or, for
`lazy v` against `born v'` (O1b), allocates its cell and runs `born v'` to it.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem born_body (m : Nat) :
    (Tm.let_ (.force (.var 0)) (.var 1)).subst (inst [.loc m]) = .let_ (.force (.loc m)) (.loc m) := by
  simp [Tm.subst, inst, liftS, Tm.rename]

/-- The left cell of a `born` pair changes, the right one stays. -/
theorem StR.setBorn {μ μ₂ : List Tm} (h : StR W μ μ₂) {l l' : Nat} {v v' c c' d : Tm}
    (hl : W.L l l' (.born v v')) (hc' : μ₂[l']? = some c') (hi : BornInv W v v' c c')
    (hd : d = .lazy v ∨ d = .running v ∨ d = .done v)
    (hc'' : c' = .lazy v' ∨ c' = .done v') : StR W (μ.set l d) μ₂ := by
  have := h.set hl (c' := c') (c := d) ⟨hi.val, hi.val', hi.cl, hi.cl', hi.noIns, hi.rel, .inl ⟨hd, hc''⟩, hi.flag⟩
  rwa [Flick.Elab.List.set_of_getElem? hc'] at this

/-- **A step of a special pair**: the left side steps by its redex. -/
theorem act_sim {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E₁ E₂ : List Frame} {a b t' : Tm} (hE₁ : OkE E₁) (hE₂ : OkE E₂)
    (hfr : FrL W E₁ E₂) (hact : Act W a b) (hst : StR W μ μ₂) (hcl₁ : (plug E₁ a).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ (plug E₂ b) μ'' t'' → t''.Closed)
    (hs : Step root l Λ μ (plug E₁ a) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ KAns root W τ μ₂ (plug E₂ b) μ μ' (plug E₁ a) t' := by
  have keep : KeepO W τ μ μ := fun _ _ _ _ _ h => h
  cases hact with
  | @dropA e b' c c' hb hc =>
    rcases step_in (F := .let_ b') hE₁ trivial (lazy_redex e) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hW := World.le_addJ₁ W μ.length
      have hr := (hfr.mono hW).plugs (.dropB (.inr rfl) hb (hc.mono hW))
      refine ⟨rfl, _, μ₂, _, hW, .inr ⟨rfl, rfl, Zero.sim hr (hst.allocJ₁ _) hW ?_ fun _ _ _ h => h⟩,
        .inl hr, hst.allocJ₁ _, KeepO.append hst _⟩
      exact pot_plug_lt E₁ (by simp only [Frame.plug, Tm.pot]; omega)
  | @dropB n b' c c' hn hb hc =>
    rcases Step.simple hE₁ (let_redex .loc) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
    · cases hh
      subst hb
      rw [subst_shift]
      refine ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.sim (hfr.plugs hc) hst
        (World.le.refl _) ?_ fun _ _ _ h => h⟩, .inl (hfr.plugs hc), hst, keep⟩
      exact pot_plug_lt E₁ (by simp only [Tm.pot, Tm.pot_rename]; omega)
    · cases hh
  | @forceA e e' hc =>
    rcases step_in (F := .force) hE₁ trivial (lazy_redex e) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hW := World.le_addO W μ.length τ
      refine ⟨rfl, _, μ₂, _, hW, .inr ⟨rfl, rfl, fun _ => ⟨?_, ?_⟩⟩,
        .inr ⟨E₁, E₂, μ.length, e, _, rfl, rfl, hE₁, hE₂, hfr.mono hW, .inr ⟨rfl, rfl⟩,
          by simp, hc.mono hW⟩, hst.allocO _ τ, KeepO.append hst _⟩
      · have hx : xpot (W.addO μ.length τ) (μ ++ [.lazy e]) (plug E₁ (.force (.loc μ.length))) =
            e.pot := xpot_eq (q := (E₁, μ.length, e)) ⟨hE₁, rfl, .inr rfl, by simp⟩
        have := pot_plug E₁ (.force (.loc μ.length)) (.force (.lazy e))
        simp only [pot, hx, Frame.plug, Tm.pot] at this ⊢
        omega
      · rintro n τ' hτ (h | ⟨-, rfl⟩)
        · exact h
        · exact (hτ rfl).elim
  | @bornA v v' hB hv hv' hvv =>
    rcases Step.simple hE₁ (lazy_redex v) trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hcv : v.Closed := lazy_closed (plug_closed hcl₁)
      have hcv' : v'.Closed := born_closed (plug_closed (hcl₂ _ _ .refl))
      have hW := World.le_addL W μ.length μ₂.length (.born v v')
      refine ⟨rfl, _, (μ₂ ++ [Tm.lazy v']).set μ₂.length (Tm.done v'), _, hW, ?_,
        .inl ((hfr.mono hW).plugs (.loc (k := .born v v') (.inr ⟨rfl, rfl, rfl⟩))),
        by rw [append_set_last]; exact hst.alloc ⟨hv, hv', hcv, hcv', hvv.value_noIns hv',
          hvv.mono hW, .inl ⟨.inl rfl, .inr rfl⟩, hB⟩, KeepO.append hst _⟩
      have hl : (μ₂ ++ [Tm.lazy v'])[μ₂.length]? = some (.lazy v') := by simp
      have s₁ := fun Λ => Step.rt (root := root) (Λ := Λ) (μ := μ₂)
        (E := .let_ (.let_ (.force (.var 0)) (.var 1)) :: E₂) (hE₂.cons' trivial) (.lazy (e := v'))
      have s₂ := fun Λ => Step.pure (root := root) (Λ := Λ) (μ := μ₂ ++ [Tm.lazy v']) hE₂
        (PHead.let_ (v := .loc μ₂.length) (b := .let_ (.force (.var 0)) (.var 1)) .loc)
      rw [born_body] at s₂
      have s₃ := fun Λ => Step.rt (root := root) (Λ := Λ) (E := .let_ (.loc μ₂.length) :: E₂)
        (hE₂.cons' trivial) (.forcePending hl)
      have s₄ := fun Λ => Step.rt (root := root) (Λ := Λ) (E := .let_ (.loc μ₂.length) :: E₂)
        (μ := (μ₂ ++ [Tm.lazy v']).set μ₂.length (Tm.running v')) (hE₂.cons' trivial)
        (.settleVal (l := μ₂.length) hv')
      rw [settleCell_running (by simp)] at s₄
      have s₅ := fun Λ => Step.pure (root := root) (Λ := Λ) (μ := (μ₂ ++ [Tm.lazy v']).set μ₂.length (Tm.done v')) hE₂
        (PHead.let_ (v := v') (b := .loc μ₂.length) hv')
      exact .inl ⟨_, _, fun Λ => ⟨_, s₁ Λ⟩, .step (fun Λ => ⟨_, s₂ Λ⟩) (.step (fun Λ => ⟨_, s₃ Λ⟩)
        (.step (fun Λ => ⟨_, s₄ Λ⟩) (.step (fun Λ => ⟨_, s₅ Λ⟩) .refl)))⟩
  | @bornB v v' hB hv hv' hvv =>
    rcases step_in (F := .let_ (.let_ (.force (.var 0)) (.var 1))) hE₁ trivial (lazy_redex v)
      trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hcv : v.Closed := born_closed (plug_closed hcl₁)
      have hcv' : v'.Closed := lazy_closed (plug_closed (hcl₂ _ _ .refl))
      have hW := World.le_addL W μ.length μ₂.length (.born v v')
      exact ⟨rfl, _, μ₂ ++ [.lazy v'], _, hW, .inl ⟨_, _, fun _ => ⟨_, .rt hE₂ RHead.lazy⟩, .refl⟩,
        .inl ((hfr.mono hW).plugs (.bornS hB (.inr ⟨rfl, rfl, rfl⟩) hcv (.inl rfl))),
        hst.alloc ⟨hv, hv', hcv, hcv', hvv.value_noIns hv', hvv.mono hW,
          .inl ⟨.inl rfl, .inl rfl⟩, hB⟩, KeepO.append hst _⟩
  | @bornS l₀ l' v v' t hB hl hcv hbs =>
    obtain ⟨c, c', hc, hc', hi⟩ := hst.cell _ _ _ hl
    have hi : BornInv W v v' c c' := hi
    rcases hbs with rfl | rfl | rfl | rfl | rfl
    · rcases Step.simple hE₁ (let_redex .loc) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
      · cases hh
        rw [born_body]
        exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
          .inl (hfr.plugs (.bornS hB hl hcv (.inr (.inl rfl)))), hst, keep⟩
      · cases hh
    · rcases step_in (F := .let_ (.loc l₀)) hE₁ trivial (force_redex l₀) trivial hs with
        ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
      · cases hh
      · cases hh with
        | forcePending h₀ =>
          rw [hc] at h₀; cases h₀
          rcases hi.state with ⟨hl₁ | hl₁ | hl₁, hr₁⟩ | ⟨hn, -, -⟩
          · cases hl₁
            exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
              .inl (hfr.plugs (.bornS hB hl hcv (.inr (.inr (.inl rfl))))),
              hst.setBorn hl hc' hi (.inr (.inl rfl)) hr₁,
              fun n _ e _ ho h' => by
                rw [getElem?_set_of_ne (fun e => (hst.j₁ _ (hst.own _ _ ho)).2 _ _ (e ▸ hl))]
                exact h'⟩
          · cases hl₁
          · cases hl₁
          · exact (hn trivial).elim
        | forceDone h₀ =>
          rw [hc] at h₀; cases h₀
          rcases hi.state with ⟨hl₁ | hl₁ | hl₁, -⟩ | ⟨hn, -, -⟩
          · cases hl₁
          · cases hl₁
          · cases hl₁
            exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
              .inl (hfr.plugs (.bornS hB hl hcv (.inr (.inr (.inr (.inl rfl)))))), hst, keep⟩
          · exact (hn trivial).elim
        | forceRunning =>
          exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
            .inl (hfr.plugs (.bornS hB hl hcv (.inr (.inr (.inr (.inr rfl)))))), hst, keep⟩
    · rcases step_in (F := .let_ (.loc l₀)) hE₁ trivial (settle_redex hi.val) trivial hs with
        ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
      · cases hh
      · cases hh with
        | settleVal =>
          refine ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
            .inl (hfr.plugs (.bornS hB hl hcv (.inr (.inr (.inr (.inl rfl)))))), ?_, ?_⟩
          · rcases settleCell_cases μ l₀ v with ⟨e, he, e₁⟩ | ⟨_, e₁⟩ <;> rw [e₁]
            · rw [hc] at he; cases he
              rcases hi.state with ⟨-, hr₁⟩ | ⟨hn, -, -⟩
              · exact hst.setBorn hl hc' hi (.inr (.inr rfl)) hr₁
              · exact (hn trivial).elim
            · exact hst
          · intro n _ e _ ho h'
            rw [settleCell_other (fun e => (hst.j₁ _ (hst.own _ _ ho)).2 _ _ (e ▸ hl))]
            exact h'
        | settleRaise hw => exact absurd hi.val (by intro h; cases h)
        | settleUnw => exact absurd hi.val (by intro h; cases h)
    · rcases Step.simple hE₁ (let_redex hi.val) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
      · cases hh
        exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩, .inl (hfr.plugs (.loc hl)), hst, keep⟩
      · cases hh
    · rcases step_in (F := .let_ (.loc l₀)) hE₁ trivial todo_redex trivial hs with
        ⟨_, hh, -⟩ | ⟨_, hh, -⟩ <;> cases hh
  | @bornV l₀ l' v v' hB hl hcv hcv' hn =>
    obtain ⟨c, c', hc, hc', hi⟩ := hst.cell _ _ _ hl
    have hi : BornInv W v _ c c' := hi
    rcases Step.simple hE₁ (settle_redex hi.val) trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh with
      | settleVal =>
        refine ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩, .inl (hfr.plugs hi.rel), ?_, ?_⟩
        · rcases settleCell_cases μ l₀ v with ⟨e, he, e₁⟩ | ⟨_, e₁⟩ <;> rw [e₁]
          · rw [hc] at he; cases he
            rcases hi.state with ⟨-, hr₁⟩ | ⟨hn', -, -⟩
            · exact hst.setBorn hl hc' hi (.inr (.inr rfl)) hr₁
            · exact (hn' trivial).elim
          · exact hst
        · intro n _ e _ ho h'
          rw [settleCell_other (fun e => (hst.j₁ _ (hst.own _ _ ho)).2 _ _ (e ▸ hl))]
          exact h'
      | settleRaise hw => exact absurd hi.val (by intro h; cases h)
      | settleUnw => exact absurd hi.val (by intro h; cases h)
  | dead =>
    rcases Step.simple hE₁ todo_redex trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, -⟩ <;> cases hh

end Flick.Equiv
