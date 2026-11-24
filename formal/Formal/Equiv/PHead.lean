import Formal.Equiv.Inv

/-!
# Contextual equivalence: a pure head step

A pure head step of the left side is matched by the same step of the right side, once it has
caught up the values the step reads (`phead_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

/-- The right side reaches a value in a frame. -/
theorem vcu_frame {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) (ha : Value a)
    {μ₁ : List Tm} (μ : List Tm) (hst : StR W μ₁ μ) {F : Frame} (hF : F.ok)
    (hcl : W.B → a.Closed ∧ b.Closed) :
    ∃ W' μ' b', RJ W μ (F.plug b) W' μ' (F.plug b') ∧ Value b' ∧ Sim W' .any a b' := by
  obtain ⟨W', μ', b', r, v, s⟩ := h.vcu hx ha W μ (World.le.refl _) hst hcl
  exact ⟨W', μ', b', r.frame hF, v, s⟩

/-- **A pure head step** of related terms. Neither side takes a dead arm (`Sim.aconsD`): what
the step gives has no lazy cell where a pattern reads (`hzl`, `hzr`). -/
theorem phead_sim {r r' r₂ : Tm} (hh : PHead r r') (h : Sim W .top r r₂)
    {μ₁ : List Tm} (μ : List Tm) (hst : StR W μ₁ μ)
    (hcl : W.B → r.Closed ∧ r₂.Closed)
    (hmvl : r.MatchVals) (hmvr : ∀ μ' a, AStar μ r₂ μ' a → a.MatchVals)
    (hzl : ¬ r'.LzTop .any)
    (hzr : ∀ μ' a r₂', AStar μ r₂ μ' a → PHead a r₂' → ¬ r₂'.LzTop .any) :
    ∃ W' μ' a₂ r₂', RJ W μ r₂ W' μ' a₂ ∧ PHead a₂ r₂' ∧ Sim W' .any r' r₂' := by
  cases hh with
  | @beta v b hv =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    obtain ⟨b', rfl, hb, hn⟩ := h₁.inv_lam
    have hv' := h₂.top_value rfl hv
    exact ⟨W, μ, _, _, RJ.refl _ _, .beta hv',
      hb.subst _ _ (SubR.inst (.cons (h₂.sub hv hv') .nil))⟩
  | @fix v f hv =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    have hv' := h₂.top_value rfl hv
    rcases h₁.inv_fix with ⟨f₂, rfl, hf, hn⟩ |
      ⟨l', e, e', b, b', hB, rfl, rfl, hhr, hce, hpe, he, hb, hnb⟩ |
      ⟨l, e, e', b, b', hB, rfl, rfl, hhl, hce', hpe', he, hb, hne', hnb⟩
    · exact ⟨W, μ, _, _, RJ.refl _ _, .fix hv', hf.subst _ _ (SubR.inst
        (.cons (h₂.sub hv hv') (.cons (Sim.sub (Sim.fix (x := .any) hf hn) .fix .fix) .nil)))⟩
    · have hsub := SubR.inst (.cons (h₂.sub hv hv')
        (.cons (Sim.sub (.o4Fix1 (x := .any) hB hhr rfl rfl hce hpe he hb hnb) .fix .fix) .nil))
      refine ⟨W, μ, _, _, RJ.refl _ _, .fix hv', ?_⟩
      simp only [Tm.subst, subst_shift2]
      exact .o4Alloc1 hB hhr (subst_loc_inst2 _ _ _ _) hce hpe he (hb.subst _ _ (hsub.lift 1))
        (Tm.NoIns.subst hnb (hsub.lift 1).noIns)
    · have hsub := SubR.inst (.cons (h₂.sub hv hv')
        (.cons (Sim.sub (.o4Fix2 (x := .any) hB hhl rfl rfl hce' hpe' he hb hne' hnb) .fix .fix) .nil))
      refine ⟨W, μ, _, _, RJ.refl _ _, .fix hv', ?_⟩
      simp only [Tm.subst, subst_shift2]
      exact .o4AllocR hB hhl (subst_loc_inst2 _ _ _ _) hce' hpe' he (hb.subst _ _ (hsub.lift 1))
  | @let_ v b hv =>
    obtain ⟨e', b', rfl, h₁, h₂⟩ := h.inv_let
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .let_ b') trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .let_ _) a, closed_frame_inner (F := .let_ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .let_ hv',
      (h₂.mono r₁.le).subst _ _ (SubR.inst (.cons (s₁.sub hv hv') .nil))⟩
  | @letrec f b =>
    obtain ⟨f', b', rfl, h₁, hn, h₂⟩ := h.inv_letrec
    exact ⟨W, μ, _, _, RJ.refl _ _, .letrec,
      h₂.subst _ _ (SubR.inst (.cons (Sim.sub (Sim.fix (x := .any) h₁ hn) .fix .fix) .nil))⟩
  | @proj v l w hv hf =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_proj
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .proj l) trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .proj _) a, closed_frame_inner (F := .proj _) b⟩)
    obtain ⟨w', hw', hw⟩ := ((s₁.value_any hv hv' .top).field rfl l).some_left hf
    exact ⟨W₁, μ₁', _, _, r₁, .proj hv' hw', hw.lower⟩
  | @update v w l v₁ hv hw hs =>
    obtain ⟨e', a', rfl, h₁, h₂⟩ := h.inv_update
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .updL l a') trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .updL _ _) a, closed_frame_inner (F := .updL _ _) b⟩)
    obtain ⟨W₂, μ₂, w', r₂, hw', s₂⟩ :=
      vcu_frame (h₂.mono r₁.le) rfl hw μ₁' (r₁.st _ hst) (F := .updR v' l) hv'
        (fun hB => let ⟨a, b⟩ := hcl (r₁.le.B.2 hB);
          ⟨closed_frame_inner (F := .updR _ _) a, closed_frame_inner (F := .updR _ _) b⟩)
    obtain ⟨v₂, hs', hvv⟩ := (Sim.setField ((s₁.mono r₂.le).value_any hv hv' .top)
      (s₂.value_any hw hw' .top) l).some_left hs
    exact ⟨W₂, μ₂, _, _, r₁.trans r₂, .update hv' hw' hs', hvv.lower⟩
  | @matchYes v p δ b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨b', r', rfl, hb, hr⟩ := h₂.inv_acons
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .match_ (.acons p b' r')) trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .match_ _) a, closed_frame_inner (F := .match_ _) b⟩)
    obtain ⟨δ₂, hm', hδ⟩ := (s₁.value_any hv hv' .top).matchP_sub hv' hm
      (hmvl [] v p b rest δ OkE.nil hv hm (.inl rfl))
      (fun δ₂ hm₂ => hmvr _ _ r₁.steps [] v' p b' r' δ₂ OkE.nil hv' hm₂ (.inl rfl))
    rcases hb with hb | ⟨hd | hd, -⟩
    · exact ⟨W₁, μ₁', _, _, r₁, .matchYes hv' hm', (hb.mono r₁.le).subst _ _ hδ⟩
    · exact (hzl (hd.subst _)).elim
    · exact (hzr _ _ _ r₁.steps (.matchYes hv' hm') (hd.subst _)).elim
  | @matchNo v p b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨b', r', rfl, hb, hr⟩ := h₂.inv_acons
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .match_ (.acons p b' r')) trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .match_ _) a, closed_frame_inner (F := .match_ _) b⟩)
    have hm' := ((s₁.value_any hv hv' .top).matchP (p := p)).none_left hm
    exact ⟨W₁, μ₁', _, _, r₁, .matchNo hv' hm', .match_ s₁ (hr.mono r₁.le)⟩
  | @guardYes v p δ g b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨g', b', r', rfl, hg, hb, hr⟩ := h₂.inv_aguard
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ :=
      vcu_frame h₁ rfl hv μ hst (F := .match_ (.aguard p g' b' r')) trivial
        (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .match_ _) a, closed_frame_inner (F := .match_ _) b⟩)
    obtain ⟨δ₂, hm', hδ⟩ := (s₁.value_any hv hv' .top).matchP_sub hv' hm
      (hmvl [] v p b rest δ OkE.nil hv hm (.inr (.inl ⟨g, rfl⟩)))
      (fun δ₂ hm₂ => hmvr _ _ r₁.steps [] v' p b' r' δ₂ OkE.nil hv' hm₂ (.inr (.inl ⟨g', rfl⟩)))
    exact ⟨W₁, μ₁', _, _, r₁, .guardYes hv' hm', .match_ ((hg.mono r₁.le).subst _ _ hδ)
      (.acons ((hb.mono r₁.le).subst _ _ hδ) (.acons (.match_ s₁ (hr.mono r₁.le)) .anil))⟩
  | @guardNo v p g b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨g', b', r', rfl, hg, hb, hr⟩ := h₂.inv_aguard
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ :=
      vcu_frame h₁ rfl hv μ hst (F := .match_ (.aguard p g' b' r')) trivial
        (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .match_ _) a, closed_frame_inner (F := .match_ _) b⟩)
    have hm' := ((s₁.value_any hv hv' .top).matchP (p := p)).none_left hm
    exact ⟨W₁, μ₁', _, _, r₁, .guardNo hv' hm', .match_ s₁ (hr.mono r₁.le)⟩
  | @tryVal v A hv =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_try
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .try_ al') trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .try_ _) a, closed_frame_inner (F := .try_ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .tryVal hv', s₁⟩
  | @intEq a b =>
    obtain ⟨f', b', rfl, h₁, h₂⟩ := h.inv_app
    obtain ⟨c', a', rfl, h₃, h₄⟩ := h₁.inv_app
    rw [h₂.inv_lit, h₃.inv_const, h₄.inv_lit]
    exact ⟨W, μ, _, _, RJ.refl _ _, .intEq, .lit⟩
  | @funcRef v hv =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    rw [h₁.inv_const]
    exact ⟨W, μ, _, _, RJ.refl _ _, .funcRef (h₂.top_value rfl hv), h₂.lower⟩
  | @hwithVal v p ℓ E rf al hv =>
    obtain ⟨ℓ', k, al', r', e', rfl, hl, hal, hr, h₁⟩ := h.inv_hwith
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .hwith p ℓ' E al' r') trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hwith _ _ _ _ _) a, closed_frame_inner (F := .hwith _ _ _ _ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hwithVal hv', .app (by decide) (hr.mono r₁.le) s₁⟩
  | @hctlTail k p w ℓ κ E rf al Ei hEi hk hw =>
    obtain ⟨ℓ', kd, κ', al', r', a', k', rfl, hl, hkap, hal, hr, h₁, hc⟩ := h.inv_hctl
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁', w', r₁, hw', s₁⟩ :=
      vcu_frame h₁ rfl hw μ hst (F := .hctlA p ℓ' κ' true E al' r' (plug Ei₂ (.hole p))) trivial
        (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hctlA _ _ _ _ _ _ _ _) a, closed_frame_inner (F := .hctlA _ _ _ _ _ _ _ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hctlTail hEi₂ rfl hw',
      .hwith (r₁.le.L _ _ _ hl) (hal.mono r₁.le) (hr.mono r₁.le) ((hfr.mono r₁.le).plugs s₁)⟩
  | @hctlDisc k p w ℓ κ E rf al Ei hEi hk hw =>
    obtain ⟨ℓ', kd, κ', al', r', a', k', rfl, hl, hkap, hal, hr, h₁, hc⟩ := h.inv_hctl
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁', w', r₁, hw', s₁⟩ :=
      vcu_frame h₁ rfl hw μ hst (F := .hctlA p ℓ' κ' false E al' r' (plug Ei₂ (.hole p))) trivial
        (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hctlA _ _ _ _ _ _ _ _) a, closed_frame_inner (F := .hctlA _ _ _ _ _ _ _ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hctlDisc hEi₂ rfl hw',
      .hwith (r₁.le.L _ _ _ hl) (hal.mono r₁.le) (hr.mono r₁.le)
        ((hfr.mono r₁.le).plugs (.unw (r₁.le.L _ _ _ hl) s₁))⟩
  | @hresVal a p w ℓ A hA ha hw =>
    obtain ⟨ℓ', kd, a', b', rfl, hl, hc, h₁⟩ := h.inv_hres
    subst ha
    obtain ⟨A₂, rfl, hfr, hA₂⟩ := hc.plug_hole hA
    obtain ⟨W₁, μ₁', w', r₁, hw', s₁⟩ :=
      vcu_frame h₁ rfl hw μ hst (F := .hresB p ℓ' (plug A₂ (.hole p))) trivial
        (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hresB _ _ _) a, closed_frame_inner (F := .hresB _ _ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hresVal hA₂ rfl hw', .hdone ((hfr.mono r₁.le).plugs s₁)⟩
  | @hdoneVal v p hv =>
    obtain ⟨a', rfl, h₁⟩ := h.inv_hdone
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .hdone p) trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hdone _) a, closed_frame_inner (F := .hdone _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hdoneVal hv', s₁⟩
  | @hkeepVal v q ℓ E al hv =>
    obtain ⟨ℓ', k, al', e', rfl, hl, hal, h₁⟩ := h.inv_hkeep
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ hst (F := .hkeep q ℓ' E al') trivial
      (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hkeep _ _ _ _) a, closed_frame_inner (F := .hkeep _ _ _ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hkeepVal hv', s₁⟩
  | @hkarmVal k q w ℓ E al Ei hEi hk hw =>
    obtain ⟨ℓ', kd, al', a', k', rfl, hl, hal, h₁, hc⟩ := h.inv_hkarm
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁', w', r₁, hw', s₁⟩ :=
      vcu_frame h₁ rfl hw μ hst (F := .hkarmA q ℓ' E al' (plug Ei₂ (.hole q))) trivial
        (fun hB => let ⟨a, b⟩ := hcl hB; ⟨closed_frame_inner (F := .hkarmA _ _ _ _ _) a, closed_frame_inner (F := .hkarmA _ _ _ _ _) b⟩)
    exact ⟨W₁, μ₁', _, _, r₁, .hkarmVal hEi₂ rfl hw',
      .hkeep (r₁.le.L _ _ _ hl) (hal.mono r₁.le) ((hfr.mono r₁.le).plugs s₁)⟩

end Flick.Equiv
