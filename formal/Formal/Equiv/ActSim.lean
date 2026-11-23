import Formal.Equiv.ActSimAgainF
import Formal.Equiv.ActSimAgainR

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
    fun l₀ v₀ v₀' hd' => by obtain ⟨hl₀, -⟩ := h.done l l₀ v₀ v₀' hd'; cases h.kd _ _ _ _ _ hl hl₀
  rwa [Flick.Elab.List.set_of_getElem? hc'] at this

/-- **A step of a special pair**: the left side steps by its redex. -/
theorem act_sim {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E₁ E₂ : List Frame} {a b t' : Tm} (hE₁ : OkE E₁) (hE₂ : OkE E₂)
    (hfr : FrL W E₁ E₂) (hact : Act W a b) (hst : StR W μ μ₂)
    (hR : W.R = root) (hglob : ∀ g v, root.glob g = some v → v.Src)
    (hcl₁ : (plug E₁ a).Closed) (hcl₁' : t'.Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ (plug E₂ b) μ'' t'' → t''.Closed)
    (hrun : ∀ m, 0 < (plug E₁ a).settleCount m → Running μ m)
    (hdone₁ : ∀ (i : Nat) (o : Tm), μ[i]? = some (.done o) → DoneCell o)
    (hdone₂ : ∀ μ'' t'', KSteps root μ₂ (plug E₂ b) μ'' t'' → ∀ (i : Nat) (o : Tm), μ''[i]? = some (.done o) → DoneCell o)
    (hzl : ¬ t'.LzTop .any)
    (hzr : ∀ μ'' t'', KSteps root μ₂ (plug E₂ b) μ'' t'' → ¬ t''.LzTop .any)
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
  | @o4B e e' b₀ b₀' br f' hB hbr hf' hpe hpe' he hb hne hnb =>
    subst hbr hf'
    rcases step_in (F := .let_ (.fix (b₀.rename rot))) hE₁ trivial (lazy_redex e) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hce : e.Closed := closed_let_lazy (plug_closed hcl₁)
      have hce' : e'.Closed := closed_fix_let_lazy_up2 (plug_closed (hcl₂ _ _ .refl))
      have hpe₁ : PureEval W.R e := hpe.eval hce
      have hpe₁' : PureEval W.R e' := hpe'.eval hce'
      have hW := World.le_addHL W μ.length e
      refine ⟨rfl, _, μ₂, _, hW, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
        .inl ((hfr.mono hW).plugs (.o4Bind hB ?_)), hst.allocHL hB hce hpe₁, KeepO.append hst _⟩
      simp only [Tm.subst]
      exact .o4Fix2 hB (.inr ⟨rfl, rfl⟩) (subst_rot_loc _ _) rfl hce' hpe₁'
        (he.mono hW) (hb.mono hW) hne hnb
  | @o4Alloc1 l' e e' c c' s' hB hhr hs' hce hpe he hc hnc =>
    subst hs'
    rcases step_in (F := .let_ c) hE₁ trivial (lazy_redex e) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh
      have hW₁ := World.le_addHL W μ.length e
      have hW := hW₁.trans (World.le_addH (W.addHL μ.length e) μ.length l')
      have hsub : SubR ((W.addHL μ.length e).addH μ.length l') (inst [.loc μ.length]) (inst [.loc l']) := by
        intro i; cases i with
        | zero =>
          exact ⟨fun y _ => .hloc hB (.inr ⟨rfl, rfl⟩), by simp [inst, Tm.NoIns, Tm.Has],
            .inr .loc, .inr .loc⟩
        | succ k =>
          exact ⟨fun _ hy => .var hy, by simp [inst, Tm.NoIns, Tm.Has],
            .inl ⟨k, rfl⟩, .inl ⟨k, rfl⟩⟩
      exact ⟨rfl, _, μ₂, _, hW, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
        .inl ((hfr.mono hW).plugs (.o4Bind hB ((hc.mono hW).subst _ _ hsub))),
        (hst.allocHL hB hce hpe).linkH hB (.inr ⟨rfl, rfl⟩) hhr (he.mono hW₁), KeepO.append hst _⟩
  | @o4Bind l₀ c c' hB hc =>
    rcases Step.simple hE₁ (let_redex .loc) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
    · cases hh
      exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩, .inl (hfr.plugs hc), hst, keep⟩
    · cases hh
  | @o4Settle l₀ l' e t hB hH hhl hce hct hpt =>
    obtain ⟨-, e₀, e', hhl₀, hhr, hee⟩ := hst.hrel l₀ l' hH
    obtain ⟨-, hu, -, hnj₁, -, hpe, c, hc, hstL⟩ := hst.hl l₀ e hhl
    obtain rfl := hu e₀ hhl₀
    obtain ⟨-, -, -, -, hce', hpe', c', hc', hstR⟩ := hst.hr l' e' hhr
    by_cases hvt : Value t
    · rcases Step.simple hE₁ (settle_redex hvt) trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
      · cases hh
      · cases hh with
        | settleVal =>
          obtain ⟨v', hpev', hv', hvv⟩ := pure_sim_val hst hB (hR ▸ hglob) hpe hpe' hee hpt hvt
          have hstL' : StR W (settleCell μ l₀ t) μ₂ := by
            rcases settleCell_cases μ l₀ t with ⟨_, _, e₁⟩ | ⟨_, e₁⟩ <;> rw [e₁]
            · exact hst.setHL hhl (.inr (.inr ⟨t, rfl, hvt, hpt⟩))
            · exact hst
          have hko : KeepO W τ μ (settleCell μ l₀ t) := fun n _ e' _ ho h' => by
            have hne : n ≠ l₀ := fun he => hnj₁ (he ▸ hst.own _ _ ho)
            rw [settleCell_other hne]
            exact h'
          rcases hstR with rfl | ⟨v₀', rfl, hv₀', hpev₀'⟩
          · have hstR' : StR W (settleCell μ l₀ t) (μ₂.set l' (.done v')) :=
              hstL'.setHR hhr (.inr ⟨v', rfl, hv', hpev'⟩)
            have hseq : HSeq root (μ₂.set l' (.running e')) (.settle l' e') (μ₂.set l' (.done v')) v' := by
              rw [← settleCell_running (e := e') (o := v') (hst.ltHR hhr)]
              exact HSeq.of_psteps ((hR ▸ hpev').frame (F := .settle l') trivial)
                (.rt (fun _ => .settleVal hv') .refl)
            exact ⟨rfl, W, μ₂.set l' (.done v'), plug E₂ v', World.le.refl _,
              .inl ⟨_, _, fun _ => ⟨_, .rt hE₂ (.forcePending hc')⟩, hseq.ksteps hE₂⟩,
              .inl (hfr.plugs hvv), hstR', hko⟩
          · obtain rfl := PSteps.det hpev₀' hpev' hv₀' hv'
            exact ⟨rfl, W, μ₂, plug E₂ v₀', World.le.refl _,
              .inl ⟨_, _, fun _ => ⟨_, .rt hE₂ (.forceDone hc')⟩, .refl⟩,
              .inl (hfr.plugs hvv), hstL', hko⟩
        | settleRaise => exact absurd hvt (by intro h; cases h)
        | settleUnw => exact absurd hvt (by intro h; cases h)
    · obtain ⟨rfl, rfl, rfl, t₁, hp₁, rfl⟩ :=
        PureEval.step_settle hE₁ (hR ▸ hpe) (hR ▸ hpt) hvt hs
      have hct₁ : t₁.Closed := closed_frame_inner (F := .settle l₀) (plug_closed hcl₁')
      exact ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, Zero.of_B hB⟩,
        .inl (hfr.plugs (.o4Settle hB hH hhl hce hct₁ (hpt.trans (.single (hR.symm ▸ hp₁))))),
        hst, keep⟩
  | doneF _ hd _ _ _ =>
    exact act_sim_doneF hE₁ hfr hst hd hs
  | againF1 hnB hc he hve _ hσ hσn hσv hσv' =>
    exact act_sim_againF1 hE₁ hE₂ hfr hst hcl₁ hdone₁ hdone₂ hnB hc he hve hσ hσn hσv hσv' hs
  | againF2 hnB hl hfn he hc hσ hσn hσv hσv' =>
    exact act_sim_againF2 hE₁ hE₂ hfr hst hcl₁ hcl₂ hrun hnB hl hfn he hc hσ hσn hσv hσv' hs
  | againF3 hnB hd hv hv' _ hcv' hnv' hvv hc hσ hσn _ _ =>
    exact act_sim_againF3 hE₁ hE₂ hfr hst hzl hzr hnB hd hv hv' hcv' hnv' hvv hc hσ hσn hs
  | againR1 hnB hc he hve _ hσ hσn hσv hσv' =>
    exact act_sim_againR1 hE₁ hE₂ hfr hst hcl₁ hdone₁ hdone₂ hnB hc he hve hσ hσn hσv hσv' hs
  | againR2 hnB hl hfn he hc hσ hσn hσv hσv' =>
    exact act_sim_againR2 hE₁ hE₂ hfr hst hcl₁ hcl₂ hrun hnB hl hfn he hc hσ hσn hσv hσv' hs
  | againR3 hnB hd hv hv' hcv _ hnv' hvv hc hσ hσn _ _ =>
    exact act_sim_againR3 hE₁ hE₂ hfr hst hzl hzr hnB hd hv hv' hcv hnv' hvv hc hσ hσn hs
  | @letRaise v v' b₁ b₂ _ hv hv' hvv =>
    obtain ⟨rfl, rfl, rfl⟩ := step_let_raise hE₁ hv hs
    exact ⟨rfl, W, μ₂, _, World.le.refl _,
      .inl ⟨_, _, fun _ => ⟨_, .raise hE₂ (F := .let_ b₂) trivial (by simp [Frame.isTry])
        (by simp [Frame.isInside]) (by simp [Frame.isCtl]) hv'⟩, .refl⟩,
      .inl (hfr.plugs (.raise hvv)), hst, keep⟩
  | @letUnw p ℓ ℓ' k u u' b₁ b₂ hnB hl hu =>
    obtain ⟨rfl, rfl, rfl⟩ := step_let_unw hE₁ hs
    exact ⟨rfl, W, μ₂, _, World.le.refl _,
      .inl ⟨_, _, fun _ => ⟨_, .unw hE₂ (F := .let_ b₂) trivial (by simp [Frame.isInside])
        (by simp [Frame.isCtl]) (by simp [Frame.catches])⟩, .refl⟩,
      .inl (hfr.plugs (.unw hl hu)), hst, keep⟩
  | letTodo =>
    rcases step_in (F := .let_ _) hE₁ trivial todo_redex trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, -⟩ <;> cases hh

end Flick.Equiv
