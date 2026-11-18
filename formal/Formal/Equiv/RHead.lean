import Formal.Equiv.Force

/-!
# Contextual equivalence: a runtime head step that keeps the regions

A runtime head step of the left side that does not touch the regions is answered by the same
step of the right side, once it has caught up the values, errors and signals the step reads
(`rhead_sim`); a force of a `born` pair as `force_sim` says.
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

/-- The redexes of steps that touch the regions. -/
def _root_.Flick.Tm.Reg : Tm → Prop
  | .scope _ | .inside _ _ | .hdl _ _ _ _ => True
  | .app (.const .open_) _ | .app (.const .op) _ => True
  | _ => False

theorem _root_.Flick.Value.not_cell {v : Tm} (h : Value v) : ¬ v.IsCell := by
  cases h <;> simp [Tm.IsCell]

/-- An answer by one head step that keeps the stores. -/
theorem HAns.one {μ μ₂ : List Tm} {r₂ r' W₁ μ₂a r₂a r₂'}
    (hj : RJ W μ₂ r₂ W₁ μ₂a r₂a) (hs : ∀ Λ, RHead Λ μ₂a r₂a Λ μ₂a r₂') (h : Sim W₁ .any r' r₂')
    (hst : StR W μ μ₂) : HAns W μ₂ r₂ μ μ r' :=
  ⟨W₁, μ₂a, r₂a, W₁, μ₂a, r₂', hj, HSeq.one hs, World.le.refl _, h, hj.st _ hst, fun _ _ => rfl⟩

theorem HAns.pure {μ μ₂ : List Tm} {r₂ r' W₁ μ₂a r₂a r₂'}
    (hj : RJ W μ₂ r₂ W₁ μ₂a r₂a) (hs : PHead r₂a r₂') (h : Sim W₁ .any r' r₂')
    (hst : StR W μ μ₂) : HAns W μ₂ r₂ μ μ r' :=
  ⟨W₁, μ₂a, r₂a, W₁, μ₂a, r₂', hj, .pure hs .refl, World.le.refl _, h, hj.st _ hst, fun _ _ => rfl⟩

/-- **A runtime head step** that keeps the regions. -/
theorem rhead_sim {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm} {r r' r₂ : Tm}
    (hh : RHead Λ μ r Λ' μ' r') (hreg : ¬ r.Reg) (h : Sim W .top r r₂) (hst : StR W μ μ₂) :
    HAns W μ₂ r₂ μ μ' r' := by
  cases hh with
  | @refMake v _ _ hv =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    rw [h₁.inv_const]
    have hv' := h₂.top_value rfl hv
    refine ⟨W, μ₂, _, _, _, _, RJ.refl _ _, HSeq.one (fun _ => .refMake hv'), World.le_addL _ _ _ .sym,
      .loc (k := .sym) (.inr ⟨rfl, rfl, rfl⟩), hst.alloc (h₂.mono (World.le_addL _ _ _ _)),
      fun n hn => getElem?_append_old (hst.j₁ n hn).1⟩
  | refGet hc hnc =>
    obtain ⟨f', a', rfl, h₁, h₂⟩ := h.inv_app
    rw [h₁.inv_const]
    obtain ⟨l', k, rfl, hl⟩ := h₂.inv_loc
    obtain ⟨c, c', hc₁, hc', hcr⟩ := hst.cell _ _ _ hl
    rw [hc] at hc₁; cases hc₁
    cases k with
    | sym =>
      have hcr : Sim W .top _ c' := hcr
      exact HAns.one (.refl _ _) (fun _ => .refGet hc' (fun h => hnc (hcr.top_isCell.2 h))) hcr.lower hst
    | born v v' =>
      rcases hcr.state with ⟨hl₁ | hl₁ | hl₁, -⟩ | ⟨-, hnc', hs⟩
      all_goals first
        | (subst hl₁; exact (hnc trivial).elim)
        | exact HAns.one (.refl _ _) (fun _ => .refGet hc' hnc') hs.lower hst
  | refGetCell hc hic =>
    obtain ⟨f', a', rfl, h₁, h₂⟩ := h.inv_app
    rw [h₁.inv_const]
    obtain ⟨l', k, rfl, hl⟩ := h₂.inv_loc
    obtain ⟨c, c', hc₁, hc', hcr⟩ := hst.cell _ _ _ hl
    rw [hc] at hc₁; cases hc₁
    cases k with
    | sym =>
      have hcr : Sim W .top _ c' := hcr
      exact HAns.one (.refl _ _) (fun _ => .refGetCell hc' (hcr.top_isCell.1 hic)) .todo hst
    | born v v' =>
      rcases hcr.state with ⟨-, hr₁ | hr₁⟩ | ⟨hnc, -, -⟩
      · subst hr₁; exact HAns.one (.refl _ _) (fun _ => .refGetCell hc' trivial) .todo hst
      · subst hr₁; exact HAns.one (.refl _ _) (fun _ => .refGetCell hc' trivial) .todo hst
      · exact (hnc hic).elim
  | @refSet v l _ _ hv hl₀ =>
    obtain ⟨f', v', rfl, h₁, h₂⟩ := h.inv_app
    obtain ⟨c', a', rfl, h₃, h₄⟩ := h₁.inv_app
    rw [h₃.inv_const]
    obtain ⟨l', k, rfl, hl⟩ := h₄.inv_loc
    have hv' := h₂.top_value rfl hv
    have hnj : ∀ n, W.J₁ n → n ≠ l := fun n hn e => hst.not_j₁ hl (e ▸ hn)
    refine ⟨W, μ₂, _, W, _, _, RJ.refl _ _, HSeq.one (fun _ => .refSet hv' (hst.lt₂ hl)), World.le.refl _,
      .lit, hst.set hl ?_, fun n hn => getElem?_set_of_ne (hnj n hn)⟩
    cases k with
    | sym => exact h₂
    | born v₀ v₀' =>
      obtain ⟨c, c', -, -, hcr⟩ := hst.cell _ _ _ hl
      exact ⟨hcr.val, hcr.val', hcr.cl, hcr.cl', hcr.noIns, hcr.rel,
        .inr ⟨hv.not_cell, hv'.not_cell, h₂⟩⟩
  | await =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_await
    obtain ⟨W₁, μ₁, v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl .task μ₂ (F := .await) trivial
    obtain ⟨e₂, rfl, he, -⟩ := (s₁.value_any .task hv' .top).inv_task
    exact HAns.one r₁ (fun _ => .await) he hst
  | lazy =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_lazy
    refine ⟨W, μ₂, _, _, _, _, RJ.refl _ _, HSeq.one (fun _ => .lazy), World.le_addL _ _ _ .sym,
      .loc (k := .sym) (.inr ⟨rfl, rfl, rfl⟩), hst.alloc (.lazy (h₁.mono (World.le_addL _ _ _ _))),
      fun n hn => getElem?_append_old (hst.j₁ n hn).1⟩
  | forcePending hc =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_force
    exact force_sim (Λ := Λ) (.forcePending hc) h₁ hst
  | forceDone hc =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_force
    exact force_sim (Λ := Λ) (.forceDone hc) h₁ hst
  | forceRunning hc hnl hnd =>
    obtain ⟨e', rfl, h₁⟩ := h.inv_force
    exact force_sim (Λ := Λ) (.forceRunning hc hnl hnd) h₁ hst
  | @settleVal v _ _ l hv =>
    obtain ⟨l', e', rfl, hl, h₁⟩ := h.inv_settle
    obtain ⟨W₁, μ₁, v', r₁, hv', s₁⟩ := vcu_frame h₁ rfl hv μ₂ (F := .settle l') trivial
    have hst₁ := r₁.st _ hst
    have hl₁ := r₁.le.L _ _ _ hl
    exact ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .settleVal hv'), World.le.refl _, s₁,
      hst₁.settleSym hl₁ s₁,
      fun n hn => settleCell_other (fun e => hst.not_j₁ hl (e ▸ hn))⟩
  | @settleRaise v _ _ l hv =>
    obtain ⟨l', e', rfl, hl, h₁⟩ := h.inv_settle
    obtain ⟨W₁, μ₁, v', r₁, hv', s₁⟩ := rcu_frame h₁ rfl rfl hv μ₂ (F := .settle l') trivial
    have hst₁ := r₁.st _ hst
    have hl₁ := r₁.le.L _ _ _ hl
    exact ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .settleRaise hv'), World.le.refl _, .raise s₁,
      hst₁.settleSym hl₁ (.raise s₁),
      fun n hn => settleCell_other (fun e => hst.not_j₁ hl (e ▸ hn))⟩
  | @settleUnw _ _ l p ℓ a =>
    obtain ⟨l', e', rfl, hl, h₁⟩ := h.inv_settle
    obtain ⟨W₁, μ₁, ℓ', k, a', r₁, hlk, s₁⟩ := ucu_frame h₁ rfl rfl μ₂ (F := .settle l') trivial
    have hst₁ := r₁.st _ hst
    have hl₁ := r₁.le.L _ _ _ hl
    exact ⟨W₁, μ₁, _, W₁, _, _, r₁, HSeq.one (fun _ => .settleUnw), World.le.refl _, .unw hlk s₁,
      hst₁.resetSym hl₁, fun n hn => resetCell_other (fun e => hst.not_j₁ hl (e ▸ hn))⟩
  | @tryRaise v _ _ A hv =>
    obtain ⟨e', al', rfl, h₁, h₂⟩ := h.inv_try
    obtain ⟨W₁, μ₁, v', r₁, hv', s₁⟩ := rcu_frame h₁ rfl rfl hv μ₂ (F := .try_ al') trivial
    exact HAns.one r₁ (fun _ => .tryRaise hv') (.handle (s₁.value_any hv hv' .top) (h₂.mono r₁.le)) hst
  | handleYes hv hm =>
    obtain ⟨v', al', rfl, h₁, h₂⟩ := h.inv_handle
    obtain ⟨b', r', rfl, hb, hr⟩ := h₂.inv_acons
    have hv' := h₁.top_value rfl hv
    obtain ⟨δ₂, hm', hδ⟩ := h₁.matchP_sub hv' hm
    exact HAns.one (.refl _ _) (fun _ => .handleYes hv' hm') (hb.subst _ _ hδ) hst
  | handleNo hv hm =>
    obtain ⟨v', al', rfl, h₁, h₂⟩ := h.inv_handle
    obtain ⟨b', r', rfl, hb, hr⟩ := h₂.inv_acons
    have hv' := h₁.top_value rfl hv
    exact HAns.one (.refl _ _) (fun _ => .handleNo hv' ((h₁.matchP).none_left hm)) (.handle h₁ hr) hst
  | hguardYes hv hm =>
    obtain ⟨v', al', rfl, h₁, h₂⟩ := h.inv_handle
    obtain ⟨g', b', r', rfl, hg, hb, hr⟩ := h₂.inv_aguard
    have hv' := h₁.top_value rfl hv
    obtain ⟨δ₂, hm', hδ⟩ := h₁.matchP_sub hv' hm
    exact HAns.one (.refl _ _) (fun _ => .hguardYes hv' hm') (.match_ (hg.subst _ _ hδ)
      (.acons (hb.subst _ _ hδ) (.acons (.handle h₁ hr) .anil))) hst
  | hguardNo hv hm =>
    obtain ⟨v', al', rfl, h₁, h₂⟩ := h.inv_handle
    obtain ⟨g', b', r', rfl, hg, hb, hr⟩ := h₂.inv_aguard
    have hv' := h₁.top_value rfl hv
    exact HAns.one (.refl _ _) (fun _ => .hguardNo hv' ((h₁.matchP).none_left hm)) (.handle h₁ hr) hst
  | handleNil hv =>
    obtain ⟨v', al', rfl, h₁, h₂⟩ := h.inv_handle
    rw [h₂.inv_anil]
    exact HAns.one (.refl _ _) (fun _ => .handleNil (h₁.top_value rfl hv)) (.raise h₁.lower) hst
  | scopeRun hf =>
    obtain ⟨c', f', rfl, h₁, h₂⟩ := h.inv_app
    rw [h₁.inv_const]
    have hf' := h₂.top_value rfl hf
    exact HAns.one (.refl _ _) (fun _ => .scopeRun hf')
      (.scope (.app (by decide) (h₂.lower.rename _) (.var (by decide)))) hst
  | hctlRaise hEi hk hw =>
    obtain ⟨ℓ', kd, κ', al', r', a', k', rfl, hl, hkap, hal, hr, h₁, hc⟩ := h.inv_hctl
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁, w', r₁, hw', s₁⟩ :=
      rcu_frame h₁ rfl rfl hw μ₂ (F := .hctlA _ ℓ' κ' _ _ al' r' (plug Ei₂ (.hole _))) trivial
    exact HAns.one r₁ (fun _ => .hctlRaise hEi₂ rfl hw') (.hwith (r₁.le.L _ _ _ hl) (hal.mono r₁.le)
      (hr.mono r₁.le) ((hfr.mono r₁.le).plugs (.unw (r₁.le.L _ _ _ hl) (.raise s₁)))) hst
  | hctlUnw hEi hk =>
    obtain ⟨ℓ', kd, κ', al', r', a', k', rfl, hl, hkap, hal, hr, h₁, hc⟩ := h.inv_hctl
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁, ℓ₂, kd₂, a₂, r₁, hl₂, s₁⟩ :=
      ucu_frame h₁ rfl rfl μ₂ (F := .hctlA _ ℓ' κ' _ _ al' r' (plug Ei₂ (.hole _))) trivial
    exact HAns.one r₁ (fun _ => .hctlUnw hEi₂ rfl) (.hwith (r₁.le.L _ _ _ hl) (hal.mono r₁.le)
      (hr.mono r₁.le) ((hfr.mono r₁.le).plugs (.unw (r₁.le.L _ _ _ hl) (.unw hl₂ s₁)))) hst
  | hresRaise hA ha hw =>
    obtain ⟨ℓ', kd, a', b', rfl, hl, hc, h₁⟩ := h.inv_hres
    subst ha
    obtain ⟨A₂, rfl, hfr, hA₂⟩ := hc.plug_hole hA
    obtain ⟨W₁, μ₁, w', r₁, hw', s₁⟩ :=
      rcu_frame h₁ rfl rfl hw μ₂ (F := .hresB _ ℓ' (plug A₂ (.hole _))) trivial
    exact HAns.one r₁ (fun _ => .hresRaise hA₂ rfl hw') (.hdone ((hfr.mono r₁.le).plugs (.raise s₁))) hst
  | hresUnw hA ha =>
    obtain ⟨ℓ', kd, a', b', rfl, hl, hc, h₁⟩ := h.inv_hres
    subst ha
    obtain ⟨A₂, rfl, hfr, hA₂⟩ := hc.plug_hole hA
    obtain ⟨W₁, μ₁, ℓ₂, kd₂, b₂, r₁, hl₂, s₁⟩ :=
      ucu_frame h₁ rfl rfl μ₂ (F := .hresB _ ℓ' (plug A₂ (.hole _))) trivial
    exact HAns.one r₁ (fun _ => .hresUnw hA₂ rfl) (.hdone ((hfr.mono r₁.le).plugs (.unw hl₂ s₁))) hst
  | hkarmRaise hEi hk hw =>
    obtain ⟨ℓ', kd, al', a', k', rfl, hl, hal, h₁, hc⟩ := h.inv_hkarm
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁, w', r₁, hw', s₁⟩ :=
      rcu_frame h₁ rfl rfl hw μ₂ (F := .hkarmA _ ℓ' _ al' (plug Ei₂ (.hole _))) trivial
    exact HAns.one r₁ (fun _ => .hkarmRaise hEi₂ rfl hw') (.hkeep (r₁.le.L _ _ _ hl) (hal.mono r₁.le)
      ((hfr.mono r₁.le).plugs (.unw (r₁.le.L _ _ _ hl) (.raise s₁)))) hst
  | hkarmUnw hEi hk =>
    obtain ⟨ℓ', kd, al', a', k', rfl, hl, hal, h₁, hc⟩ := h.inv_hkarm
    subst hk
    obtain ⟨Ei₂, rfl, hfr, hEi₂⟩ := hc.plug_hole hEi
    obtain ⟨W₁, μ₁, ℓ₂, kd₂, a₂, r₁, hl₂, s₁⟩ :=
      ucu_frame h₁ rfl rfl μ₂ (F := .hkarmA _ ℓ' _ al' (plug Ei₂ (.hole _))) trivial
    exact HAns.one r₁ (fun _ => .hkarmUnw hEi₂ rfl) (.hkeep (r₁.le.L _ _ _ hl) (hal.mono r₁.le)
      ((hfr.mono r₁.le).plugs (.unw hl₂ s₁))) hst
  | hwithCatch =>
    obtain ⟨ℓ', kd, al', r', e', rfl, hl, hal, hr, h₁⟩ := h.inv_hwith
    obtain ⟨W₁, μ₁, ℓ₂, kd₂, a₂, r₁, hl₂, s₁⟩ :=
      ucu_frame h₁ rfl rfl μ₂ (F := .hwith _ ℓ' _ al' r') trivial
    have he : ℓ₂ = ℓ' := ((r₁.st _ hst).bij _ _ _ _ _ _ hl₂ (r₁.le.L _ _ _ hl)).1 rfl
    subst he
    exact HAns.one r₁ (fun _ => .hwithCatch) s₁ hst
  | scope | open_ | op | finRun | close | hdl => exact (hreg trivial).elim

end Flick.Equiv
