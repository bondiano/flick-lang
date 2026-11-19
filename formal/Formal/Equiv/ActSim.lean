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
  have := h.set hl (c' := c') (c := d) ⟨hi.val, hi.val', hi.cl, hi.cl', hi.noIns, hi.rel, .inl ⟨hd, hc''⟩⟩
  rwa [Flick.Elab.List.set_of_getElem? hc'] at this

/-- **A step of a special pair**: the left side steps by its redex. -/
theorem act_sim {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E₁ E₂ : List Frame} {a b t' : Tm} (hE₁ : OkE E₁) (hE₂ : OkE E₂)
    (hfr : FrL W E₁ E₂) (hact : Act W a b) (hst : StR W μ μ₂) (hcl₁ : (plug E₁ a).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ (plug E₂ b) μ'' t'' → t''.Closed)
    (hs : Step root l Λ μ (plug E₁ a) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ KAns root W τ μ₂ (plug E₂ b) μ μ' t' := by
  have keep : KeepO W τ μ μ := fun _ _ _ _ _ h => h
  cases hact with
  | @dropA e b' c c' hb hc =>
    rcases step_in (F := .let_ b') hE₁ trivial (lazy_redex e) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hW := World.le_addJ₁ W μ.length
      exact ⟨rfl, _, μ₂, _, hW, .refl,
        .inl ((hfr.mono hW).plugs (.dropB (.inr rfl) hb (hc.mono hW))), hst.allocJ₁ _,
        KeepO.append hst _⟩
  | @dropB n b' c c' hn hb hc =>
    rcases Step.simple hE₁ (let_redex .loc) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
    · cases hh
      subst hb
      rw [subst_shift]
      exact ⟨rfl, W, μ₂, _, World.le.refl _, .refl, .inl (hfr.plugs hc), hst, keep⟩
    · cases hh
  | @forceA e e' hc =>
    rcases step_in (F := .force) hE₁ trivial (lazy_redex e) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hW := World.le_addO W μ.length τ
      exact ⟨rfl, _, μ₂, _, hW, .refl,
        .inr ⟨E₁, E₂, μ.length, e, _, rfl, rfl, hE₁, hE₂, hfr.mono hW, .inr ⟨rfl, rfl⟩,
          by simp, hc.mono hW⟩, hst.allocO _ τ, KeepO.append hst _⟩
  | @bornA v v' hv hv' hvv =>
    rcases Step.simple hE₁ (lazy_redex v) trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hcv : v.Closed := lazy_closed (plug_closed hcl₁)
      have hcv' : v'.Closed := born_closed (plug_closed (hcl₂ _ _ .refl))
      have hW := World.le_addL W μ.length μ₂.length (.born v v')
      refine ⟨rfl, _, (μ₂ ++ [Tm.lazy v']).set μ₂.length (Tm.done v'), _, hW, ?_,
        .inl ((hfr.mono hW).plugs (.loc (k := .born v v') (.inr ⟨rfl, rfl, rfl⟩))),
        by rw [append_set_last]; exact hst.alloc ⟨hv, hv', hcv, hcv', hvv.value_noIns hv',
          hvv.mono hW, .inl ⟨.inl rfl, .inr rfl⟩⟩, KeepO.append hst _⟩
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
      exact .step (fun Λ => ⟨_, s₁ Λ⟩) (.step (fun Λ => ⟨_, s₂ Λ⟩) (.step (fun Λ => ⟨_, s₃ Λ⟩)
        (.step (fun Λ => ⟨_, s₄ Λ⟩) (.step (fun Λ => ⟨_, s₅ Λ⟩) .refl))))
  | @bornB v v' hv hv' hvv =>
    rcases step_in (F := .let_ (.let_ (.force (.var 0)) (.var 1))) hE₁ trivial (lazy_redex v)
      trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hcv : v.Closed := born_closed (plug_closed hcl₁)
      have hcv' : v'.Closed := lazy_closed (plug_closed (hcl₂ _ _ .refl))
      have hW := World.le_addL W μ.length μ₂.length (.born v v')
      exact ⟨rfl, _, μ₂ ++ [.lazy v'], _, hW, .step (fun _ => ⟨_, .rt hE₂ RHead.lazy⟩) .refl,
        .inl ((hfr.mono hW).plugs (.bornS (.inr ⟨rfl, rfl, rfl⟩) hcv (.inl rfl))),
        hst.alloc ⟨hv, hv', hcv, hcv', hvv.value_noIns hv', hvv.mono hW,
          .inl ⟨.inl rfl, .inl rfl⟩⟩, KeepO.append hst _⟩
  | @bornS l₀ l' v v' t hl hcv hbs =>
    obtain ⟨c, c', hc, hc', hi⟩ := hst.cell _ _ _ hl
    have hi : BornInv W v v' c c' := hi
    rcases hbs with rfl | rfl | rfl | rfl | rfl
    · rcases Step.simple hE₁ (let_redex .loc) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
      · cases hh
        rw [born_body]
        exact ⟨rfl, W, μ₂, _, World.le.refl _, .refl,
          .inl (hfr.plugs (.bornS hl hcv (.inr (.inl rfl)))), hst, keep⟩
      · cases hh
    · rcases step_in (F := .let_ (.loc l₀)) hE₁ trivial (force_redex l₀) trivial hs with
        ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
      · cases hh
      · cases hh with
        | forcePending h₀ =>
          rw [hc] at h₀; cases h₀
          rcases hi.state with ⟨hl₁ | hl₁ | hl₁, hr₁⟩ | ⟨hn, -, -⟩
          · cases hl₁
            exact ⟨rfl, W, μ₂, _, World.le.refl _, .refl,
              .inl (hfr.plugs (.bornS hl hcv (.inr (.inr (.inl rfl))))),
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
            exact ⟨rfl, W, μ₂, _, World.le.refl _, .refl,
              .inl (hfr.plugs (.bornS hl hcv (.inr (.inr (.inr (.inl rfl)))))), hst, keep⟩
          · exact (hn trivial).elim
        | forceRunning =>
          exact ⟨rfl, W, μ₂, _, World.le.refl _, .refl,
            .inl (hfr.plugs (.bornS hl hcv (.inr (.inr (.inr (.inr rfl)))))), hst, keep⟩
    · rcases step_in (F := .let_ (.loc l₀)) hE₁ trivial (settle_redex hi.val) trivial hs with
        ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
      · cases hh
      · cases hh with
        | settleVal =>
          refine ⟨rfl, W, μ₂, _, World.le.refl _, .refl,
            .inl (hfr.plugs (.bornS hl hcv (.inr (.inr (.inr (.inl rfl)))))), ?_, ?_⟩
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
        exact ⟨rfl, W, μ₂, _, World.le.refl _, .refl, .inl (hfr.plugs (.loc hl)), hst, keep⟩
      · cases hh
    · rcases step_in (F := .let_ (.loc l₀)) hE₁ trivial todo_redex trivial hs with
        ⟨_, hh, -⟩ | ⟨_, hh, -⟩ <;> cases hh
  | @bornV l₀ l' v v' hl hcv hcv' hn =>
    obtain ⟨c, c', hc, hc', hi⟩ := hst.cell _ _ _ hl
    have hi : BornInv W v _ c c' := hi
    rcases Step.simple hE₁ (settle_redex hi.val) trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh with
      | settleVal =>
        refine ⟨rfl, W, μ₂, _, World.le.refl _, .refl, .inl (hfr.plugs hi.rel), ?_, ?_⟩
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
