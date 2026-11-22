import Formal.Equiv.Cells

/-!
# Contextual equivalence: pure simulation

When both sides of a `Sim W .any e e'` pair satisfy operational purity (`PureEval W.R`), neither
side allocates cells, uses handlers, or enters a lazy-creating rewrite; every pure step of the
left side is matched by zero or one pure step of the right side in the **same** world `W`
(`pure_step_sim`), so a pure run to a value on the left reaches a related value on the right
(`pure_sim_val`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

theorem PureEval.not_let_force_loc {root : Roots} {l : Nat} {b : Tm}
    (h : PureEval root (.let_ (.force (.loc l)) b)) : False := by
  let μ : List Tm := List.replicate (l + 1) (.lazy .todo)
  have hget : μ[l]? = some (.lazy .todo) := by simp [μ]
  have := (h.step_pure .refl (Step.rt (root := root) (Λ := []) (μ := μ)
    (E := [.let_ b]) (OkE.nil.cons' trivial) (.forcePending hget))).1
  cases this

/-- A pure value related to a pure term is related to a value at `top` without any catch-up. -/
theorem pure_val {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) (ha : Value a)
    (hpa : PureEval W.R a) (hpb : PureEval W.R b) : Value b ∧ Sim W .top a b := by
  induction h with
  | @app _ f f' a₀ a₀' _ h₁ h₂ ih₁ ih₂ =>
    subst hx
    have hfa : Value f ∧ Value a₀ := by cases ha with
      | ctorApp hf _ he => exact ⟨hf, he⟩
      | constApp _ he => exact ⟨.const, he⟩
    have ⟨hf', hs₁⟩ := ih₁ rfl hfa.1
      (hpa.of_frame (F := .appL a₀) trivial) (hpb.of_frame (F := .appL a₀') trivial)
    have ⟨ha', hs₂⟩ := ih₂ rfl hfa.2
      (hpa.of_frame (F := .appR f) hfa.1) (hpb.of_frame (F := .appR f') hf')
    refine ⟨?_, .app (by decide) hs₁ hs₂⟩
    cases ha with
    | ctorApp hf hK _ => exact .ctorApp hf' (hs₁.top_head rfl hf hK) ha'
    | constApp hc _ => rw [hs₁.top_const rfl]; exact .constApp hc ha'
  | @rcons _ l e e' r r' _ h₁ h₂ ih₁ ih₂ =>
    subst hx
    cases ha with
    | rcons he hr =>
      have ⟨he', hs₁⟩ := ih₁ rfl he
        (hpa.of_frame (F := .rconsL l r) trivial) (hpb.of_frame (F := .rconsL l r') trivial)
      have ⟨hr', hs₂⟩ := ih₂ rfl hr
        (hpa.of_frame (F := .rconsR l e) he) (hpb.of_frame (F := .rconsR l e') he')
      exact ⟨.rcons he' hr', .rcons (by decide) hs₁ hs₂⟩
  | dropC | o4A | o4AllocR => exact hpb.not_let_lazy.elim
  | forceC => exact hpb.not_force_lazy.elim
  | jR => exact hpb.not_settle.elim
  | dropA | dropB | forceA | jL | bornA | bornB | bornV | dead
  | o4B | o4Alloc1 | o4Bind | o4Settle => cases ha
  | bornS _ _ _ hs => rcases hs with rfl | rfl | rfl | rfl | rfl <;> cases ha
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | hloc hB hH => exact ⟨.loc, .hloc hB hH⟩
  | o4Fix1 hB hr hf hf' hce hpe he hb hnb => exact ⟨.fix, .o4Fix1 hB hr hf hf' hce hpe he hb hnb⟩
  | o4Fix2 hB hl hf hf' hce' hpe' he hb hne' hnb =>
    exact ⟨.fix, .o4Fix2 hB hl hf hf' hce' hpe' he hb hne' hnb⟩
  | _ =>
    subst hx
    first
      | (cases ha; done)
      | exact ⟨by constructor, by constructor <;> assumption⟩

/-- A pure head step of pure terms related at `top` is matched by the same pure head step in the
same world `W`. -/
theorem pure_phead {r r' r₂ : Tm} (hh : PHead r r') (h : Sim W .top r r₂)
    (hpa : PureEval W.R r) (hpb : PureEval W.R r₂) :
    ∃ r₂', PHead r₂ r₂' ∧ Sim W .any r' r₂' := by
  cases hh with
  | @beta v b hv =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    obtain ⟨b', rfl, hb, hn⟩ := h₁.inv_lam
    have hv' := h₂.top_value rfl hv
    exact ⟨_, .beta hv', hb.subst _ _ (SubR.inst (.cons (h₂.sub hv hv') .nil))⟩
  | @fix v f hv =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    have hv' := h₂.top_value rfl hv
    rcases h₁.inv_fix with ⟨f₂, rfl, hf, hn⟩ |
      ⟨l', e, e', b, b', hB, rfl, rfl, hhr, hce, hpe, he, hb, hnb⟩ |
      ⟨l, e, e', b, b', hB, rfl, rfl, hhl, hce', hpe', he, hb, hne', hnb⟩
    · exact ⟨_, .fix hv', hf.subst _ _ (SubR.inst
        (.cons (h₂.sub hv hv') (.cons (Sim.sub (Sim.fix (x := .any) hf hn) .fix .fix) .nil)))⟩
    · have hp' := hpa.step (.head OkE.nil (.fix hv))
      simp only [Tm.subst] at hp'
      exact hp'.not_let_lazy.elim
    · have hp' := hpb.step (.head OkE.nil (.fix hv'))
      simp only [Tm.subst] at hp'
      exact hp'.not_let_lazy.elim
  | @let_ v b hv =>
    obtain ⟨e', b', rfl, h₁, h₂⟩ := h.inv_let
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .let_ b) trivial) (hpb.of_frame (F := .let_ b') trivial)
    exact ⟨_, .let_ hv', h₂.subst _ _ (SubR.inst (.cons (s₁.sub hv hv') .nil))⟩
  | @letrec f b =>
    obtain ⟨f', b', rfl, h₁, hn, h₂⟩ := h.inv_letrec
    exact ⟨_, .letrec,
      h₂.subst _ _ (SubR.inst (.cons (Sim.sub (Sim.fix (x := .any) h₁ hn) .fix .fix) .nil))⟩
  | @proj v l w hv hf =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_proj
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .proj l) trivial) (hpb.of_frame (F := .proj l) trivial)
    obtain ⟨w', hw', hw⟩ := (s₁.field rfl l).some_left hf
    exact ⟨_, .proj hv' hw', hw.lower⟩
  | @update v w l v₁ hv hw hs =>
    obtain ⟨e', a', rfl, h₁, h₂⟩ := h.inv_update
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .updL l w) trivial) (hpb.of_frame (F := .updL l a') trivial)
    have ⟨hw', s₂⟩ := pure_val h₂ rfl hw
      (hpa.of_frame (F := .updR v l) hv) (hpb.of_frame (F := .updR e' l) hv')
    obtain ⟨v₂, hs', hvv⟩ := (Sim.setField s₁ s₂ l).some_left hs
    exact ⟨_, .update hv' hw' hs', hvv.lower⟩
  | @matchYes v p δ b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨b', r', rfl, hb, hr⟩ := h₂.inv_acons
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .match_ (.acons p b rest)) trivial)
      (hpb.of_frame (F := .match_ (.acons p b' r')) trivial)
    obtain ⟨δ₂, hm', hδ⟩ := s₁.matchP_sub hv' hm
      (hpa.matchVals .refl [] v p b rest δ OkE.nil hv hm (.inl rfl))
      (fun δ₂ hm₂ => hpb.matchVals .refl [] e' p b' r' δ₂ OkE.nil hv' hm₂ (.inl rfl))
    rcases hb with hb | ⟨hd | hd, -⟩
    · exact ⟨_, .matchYes hv' hm', hb.subst _ _ hδ⟩
    · exact ((hpa.step (.head OkE.nil (.matchYes hv hm))).noLzTop .refl (hd.subst _)).elim
    · exact ((hpb.step (.head OkE.nil (.matchYes hv' hm'))).noLzTop .refl (hd.subst _)).elim
  | @matchNo v p b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨b', r', rfl, hb, hr⟩ := h₂.inv_acons
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .match_ (.acons p b rest)) trivial)
      (hpb.of_frame (F := .match_ (.acons p b' r')) trivial)
    exact ⟨_, .matchNo hv' ((s₁.matchP (p := p)).none_left hm), .match_ s₁.lower hr⟩
  | @guardYes v p δ g b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨g', b', r', rfl, hg, hb, hr⟩ := h₂.inv_aguard
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .match_ (.aguard p g b rest)) trivial)
      (hpb.of_frame (F := .match_ (.aguard p g' b' r')) trivial)
    obtain ⟨δ₂, hm', hδ⟩ := s₁.matchP_sub hv' hm
      (hpa.matchVals .refl [] v p b rest δ OkE.nil hv hm (.inr (.inl ⟨g, rfl⟩)))
      (fun δ₂ hm₂ => hpb.matchVals .refl [] e' p b' r' δ₂ OkE.nil hv' hm₂ (.inr (.inl ⟨g', rfl⟩)))
    exact ⟨_, .guardYes hv' hm', .match_ (hg.subst _ _ hδ)
      (.acons (hb.subst _ _ hδ) (.acons (.match_ s₁.lower hr) .anil))⟩
  | @guardNo v p g b rest hv hm =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_match
    obtain ⟨g', b', r', rfl, hg, hb, hr⟩ := h₂.inv_aguard
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .match_ (.aguard p g b rest)) trivial)
      (hpb.of_frame (F := .match_ (.aguard p g' b' r')) trivial)
    exact ⟨_, .guardNo hv' ((s₁.matchP (p := p)).none_left hm), .match_ s₁.lower hr⟩
  | @tryVal v A hv =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_try
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .try_ A) trivial) (hpb.of_frame (F := .try_ al') trivial)
    exact ⟨_, .tryVal hv', s₁.lower⟩
  | @intEq a b =>
    obtain ⟨f', b', rfl, h₁, h₂⟩ := h.inv_app
    obtain ⟨c', a', rfl, h₃, h₄⟩ := h₁.inv_app
    rw [h₂.inv_lit, h₃.inv_const, h₄.inv_lit]
    exact ⟨_, .intEq, .lit⟩
  | @hwithVal v p ℓ E rf al hv =>
    obtain ⟨ℓ', k, al', r', e', rfl, hl, hal, hr, h₁⟩ := h.inv_hwith
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .hwith p ℓ E al rf) trivial)
      (hpb.of_frame (F := .hwith p ℓ' E al' r') trivial)
    exact ⟨_, .hwithVal hv', .app (by decide) hr s₁.lower⟩
  | @hctlTail k p w ℓ κ E rf al Ei hEi hk hw =>
    obtain ⟨ℓ', kd, κ', al', r', a', k', rfl, hl, hkap, hal, hr, h₁, hc⟩ := h.inv_hctl
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    have ⟨hw', s₁⟩ := pure_val h₁ rfl hw
      (hpa.of_frame (F := .hctlA p ℓ κ true E al rf (plug Ei (.hole p))) trivial)
      (hpb.of_frame (F := .hctlA p ℓ' κ' true E al' r' (plug Ei₂ (.hole p))) trivial)
    exact ⟨_, .hctlTail hEi₂ rfl hw', .hwith hl hal hr (hfr.plugs s₁.lower)⟩
  | @hctlDisc k p w ℓ κ E rf al Ei hEi hk hw =>
    obtain ⟨ℓ', kd, κ', al', r', a', k', rfl, hl, hkap, hal, hr, h₁, hc⟩ := h.inv_hctl
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    have ⟨hw', s₁⟩ := pure_val h₁ rfl hw
      (hpa.of_frame (F := .hctlA p ℓ κ false E al rf (plug Ei (.hole p))) trivial)
      (hpb.of_frame (F := .hctlA p ℓ' κ' false E al' r' (plug Ei₂ (.hole p))) trivial)
    exact ⟨_, .hctlDisc hEi₂ rfl hw', .hwith hl hal hr (hfr.plugs (.unw hl s₁.lower))⟩
  | @hresVal a p w ℓ A hA ha hw =>
    obtain ⟨ℓ', kd, a', b', rfl, hl, hc, h₁⟩ := h.inv_hres
    subst ha
    obtain ⟨A₂, rfl, hfr, hA₂⟩ := hc.plug_hole hA
    have ⟨hw', s₁⟩ := pure_val h₁ rfl hw
      (hpa.of_frame (F := .hresB p ℓ (plug A (.hole p))) trivial)
      (hpb.of_frame (F := .hresB p ℓ' (plug A₂ (.hole p))) trivial)
    exact ⟨_, .hresVal hA₂ rfl hw', .hdone (hfr.plugs s₁.lower)⟩
  | @hdoneVal v p hv =>
    obtain ⟨a', rfl, h₁⟩ := h.inv_hdone
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .hdone p) trivial) (hpb.of_frame (F := .hdone p) trivial)
    exact ⟨_, .hdoneVal hv', s₁.lower⟩
  | @hkeepVal v q ℓ E al hv =>
    obtain ⟨ℓ', k, al', e', rfl, hl, hal, h₁⟩ := h.inv_hkeep
    have ⟨hv', s₁⟩ := pure_val h₁ rfl hv
      (hpa.of_frame (F := .hkeep q ℓ E al) trivial)
      (hpb.of_frame (F := .hkeep q ℓ' E al') trivial)
    exact ⟨_, .hkeepVal hv', s₁.lower⟩
  | @hkarmVal k q w ℓ E al Ei hEi hk hw =>
    obtain ⟨ℓ', kd, al', a', k', rfl, hl, hal, h₁, hc⟩ := h.inv_hkarm
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    have ⟨hw', s₁⟩ := pure_val h₁ rfl hw
      (hpa.of_frame (F := .hkarmA q ℓ E al (plug Ei (.hole q))) trivial)
      (hpb.of_frame (F := .hkarmA q ℓ' E al' (plug Ei₂ (.hole q))) trivial)
    exact ⟨_, .hkarmVal hEi₂ rfl hw', .hkeep hl hal (hfr.plugs s₁.lower)⟩


end Flick.Equiv
