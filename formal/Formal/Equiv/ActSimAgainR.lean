import Formal.Equiv.Act

/-!
# Contextual equivalence: reverse O2a steps of special pairs (`againR1`–`againR3`, `letRaise`/`letUnw`/`letTodo`)
-/

namespace Flick.Equiv

variable {W : World}

theorem act_sim_againR1 {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E₁ E₂ : List Frame} {c₀ e e' t' : Tm} {σ σ' : Nat → Tm}
    (hE₁ : OkE E₁) (hE₂ : OkE E₂) (hfr : FrL W E₁ E₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E₁ (.let_ (.force e) (c₀.subst (subVar σ)))).Closed)
    (hdone₁ : ∀ (i : Nat) (o : Tm), μ[i]? = some (.done o) → DoneCell o)
    (hdone₂ : ∀ μ'' t'', KSteps root μ₂ (plug E₂ (.let_ (.force e') (c₀.subst (subForce e' σ')))) μ'' t'' →
      ∀ (i : Nat) (o : Tm), μ''[i]? = some (.done o) → DoneCell o)
    (hnB : ¬ W.B) (hc : c₀.Src) (he : ∀ y, y ≠ .park → Sim W y e e')
    (hve : (∃ j, e = .var j) ∨ Value e)
    (hσ : ∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) (hσn : ∀ i, (σ' i).NoIns)
    (hσv : ∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) (hσv' : ∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i))
    (hs : Step root l Λ μ (plug E₁ (.let_ (.force e) (c₀.subst (subVar σ)))) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ KAns root W τ μ₂ (plug E₂ (.let_ (.force e') (c₀.subst (subForce e' σ'))))
      μ μ' (plug E₁ (.let_ (.force e) (c₀.subst (subVar σ)))) t' := by
  have keep : KeepO W τ μ μ := fun _ _ _ _ _ h => h
  have hce : e.Closed :=
    closed_frame_inner (F := .force) (closed_frame_inner (F := .let_ _) (plug_closed hcl₁))
  have hve₁ : Value e := hve.elim (fun ⟨j, hj⟩ => (hce j (hj ▸ rfl)).elim) id
  rcases step_in (F := .let_ _) hE₁ trivial (force_val_redex hve₁) trivial hs with
    ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
  · cases hh
  · cases hh with
    | forcePending hc₀ =>
      rename_i l₀ d
      rcases (he .top (by decide)).inv_loc with ⟨l', rfl, ⟨k, hl⟩ | ⟨hB', -⟩⟩
      · cases k with
        | born _ _ =>
          obtain ⟨_, _, _, _, hi⟩ := hst.cell _ _ _ hl
          exact (hnB (show BornInv W _ _ _ _ from hi).flag).elim
        | sym =>
          obtain ⟨c, c', hc₁, hc₂, hcr⟩ := hst.cell _ _ _ hl
          rw [hc₀] at hc₁; cases hc₁
          obtain ⟨d', rfl, hd'⟩ := (hcr : Sim W .top _ _).inv_lazy
          have hnd : ∀ l₁ v₁ v₁', ¬ W.D l₀ l₁ v₁ v₁' := fun l₁ v₁ v₁' hd => by
            obtain ⟨-, -, -, -, -, -, hμd, -⟩ := hst.done l₀ l₁ v₁ v₁' hd
            rw [hc₀] at hμd; cases hμd
          have hnj : ∀ n, W.J₁ n → n ≠ l₀ := fun n hn h => hst.not_j₁ hl (h ▸ hn)
          exact ⟨rfl, W, μ₂.set l' (.running d'), _, World.le.refl _,
            .inl ⟨_, _, fun _ => ⟨_, .rt (E := .let_ _ :: E₂) (hE₂.cons' trivial) (.forcePending hc₂)⟩, .refl⟩,
            .inl (hfr.plugs (.againR2 (by decide) hnB hl hd' hc hσ hσn hσv hσv')),
            hst.set hl (.force hd') hnd,
            KeepO.of_junk hst fun n hn => getElem?_set_of_ne (hnj n hn)⟩
      · exact (hnB hB').elim
    | forceDone hc₀ =>
      rename_i o l₀
      rcases (he .top (by decide)).inv_loc with ⟨l', rfl, ⟨k, hl⟩ | ⟨hB', -⟩⟩
      · cases k with
        | born _ _ =>
          obtain ⟨_, _, _, _, hi⟩ := hst.cell _ _ _ hl
          exact (hnB (show BornInv W _ _ _ _ from hi).flag).elim
        | sym =>
          obtain ⟨c, c', hc₁, hc₂, hcr⟩ := hst.cell _ _ _ hl
          rw [hc₀] at hc₁; cases hc₁
          obtain ⟨o', rfl, ho'⟩ := (hcr : Sim W .top _ _).inv_await
          have hstep : KPos root μ₂ (plug E₂ (.let_ (.force (.loc l')) (c₀.subst (subForce (.loc l') σ'))))
              μ₂ (plug E₂ (.let_ o' (c₀.subst (subForce (.loc l') σ')))) :=
            ⟨_, _, fun _ => ⟨_, .rt (E := .let_ _ :: E₂) (hE₂.cons' trivial) (.forceDone hc₂)⟩, .refl⟩
          have hd₂ := hdone₂ _ _ .refl l' o' hc₂
          rcases hdone₁ l₀ o hc₀ with ⟨hvo, hco⟩ | ⟨v, rfl, hv⟩ | rfl
          · obtain ⟨hvo', hco'⟩ := ho'.value_doneCell hnB hvo hd₂
            have hW := World.le_addD W l₀ l' o o'
            have hst' := hst.markDone hl hvo hvo' hco hco' (ho'.value_noIns hvo') hc₀ hc₂ ho'
            exact ⟨rfl, W.addD l₀ l' o o', μ₂, _, hW, .inl hstep,
              .inl ((hfr.mono hW).plugs (.againR3 (fun h => hnB (hW.B.2 h)) (.inr ⟨rfl, rfl, rfl, rfl⟩)
                hvo hvo' hco hco' (ho'.value_noIns hvo') (ho'.mono hW) hc
                (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv')),
              hst', keep⟩
          · obtain ⟨v', rfl, hv', hvv⟩ := ho'.raise_doneCell hnB hd₂
            exact ⟨rfl, W, μ₂, _, World.le.refl _, .inl hstep,
              .inl (hfr.plugs (.letRaise hnB hv hv' hvv)), hst, keep⟩
          · obtain rfl := ho'.todo_doneCell hnB hd₂
            exact ⟨rfl, W, μ₂, _, World.le.refl _, .inl hstep,
              .inl (hfr.plugs (.letTodo hnB)), hst, keep⟩
      · exact (hnB hB').elim
    | forceRunning hc₀ hnl hnd =>
      rename_i l₀ c₀'
      rcases (he .top (by decide)).inv_loc with ⟨l', rfl, ⟨k, hl⟩ | ⟨hB', -⟩⟩
      · cases k with
        | born _ _ =>
          obtain ⟨_, _, _, _, hi⟩ := hst.cell _ _ _ hl
          exact (hnB (show BornInv W _ _ _ _ from hi).flag).elim
        | sym =>
          obtain ⟨c, c', hc₁, hc₂, hcr⟩ := hst.cell _ _ _ hl
          rw [hc₀] at hc₁; cases hc₁
          have hsh := (hcr : Sim W .top _ _).top_cell rfl
          refine ⟨rfl, W, μ₂, _, World.le.refl _,
            .inl ⟨_, _, fun _ => ⟨_, .rt (E := .let_ _ :: E₂) (hE₂.cons' trivial) (.forceRunning hc₂ ?_ ?_)⟩, .refl⟩,
            .inl (hfr.plugs (.letTodo hnB)), hst, keep⟩
          · intro d hd; obtain ⟨d₀, hd₀⟩ := hsh.1.2 ⟨d, hd⟩; exact hnl d₀ hd₀
          · intro o ho; obtain ⟨o₀, ho₀⟩ := hsh.2.2.2 ⟨o, ho⟩; exact hnd o₀ ho₀
      · exact (hnB hB').elim

theorem act_sim_againR2 {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E₁ E₂ : List Frame} {l₀ l' : Nat} {e e' c₀ t' : Tm} {σ σ' : Nat → Tm}
    (hE₁ : OkE E₁) (hE₂ : OkE E₂) (hfr : FrL W E₁ E₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E₁ (.let_ (.settle l₀ e) (c₀.subst (subVar σ)))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ (plug E₂ (.let_ (.settle l' e') (c₀.subst (subForce (.loc l') σ')))) μ'' t'' →
      t''.Closed)
    (hrun : ∀ m, 0 < (plug E₁ (.let_ (.settle l₀ e) (c₀.subst (subVar σ)))).settleCount m →
      Running μ m)
    (hnB : ¬ W.B) (hl : W.L l₀ l' .sym) (hfn : Final e) (he : Sim W .any e e') (hc : c₀.Src)
    (hσ : ∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) (hσn : ∀ i, (σ' i).NoIns)
    (hσv : ∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) (hσv' : ∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i))
    (hs : Step root l Λ μ (plug E₁ (.let_ (.settle l₀ e) (c₀.subst (subVar σ)))) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ KAns root W τ μ₂ (plug E₂ (.let_ (.settle l' e') (c₀.subst (subForce (.loc l') σ'))))
      μ μ' (plug E₁ (.let_ (.settle l₀ e) (c₀.subst (subVar σ)))) t' := by
  have hrunL : Running μ l₀ := hrun l₀ (by
    rw [plug_settleCount]; simp only [Tm.settleCount, if_true]; omega)
  rcases hfn with hv | ⟨v, hv, rfl⟩ | ⟨p, ℓ, u, rfl⟩
  · rcases step_in (F := .let_ _) hE₁ trivial (settle_redex hv) trivial hs with
      ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
    · cases hh
    · cases hh with
      | settleVal =>
        obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := vcu_frame he rfl hv μ₂ hst (F := .settle l') trivial
          (fun h => (hnB h).elim)
        have r₁' := (r₁.frame (F := .let_ (c₀.subst (subForce (.loc l') σ'))) trivial).plug hE₂
        have hst₁ := r₁.st _ hst
        have hl₁ := r₁.le.L _ _ _ hl
        have hB₁ : ¬ W₁.B := fun h => hnB (r₁.le.B.2 h)
        obtain ⟨d₁, hd₁⟩ := hrunL
        obtain ⟨c, c', hc₁, hc₂, hcr⟩ := hst₁.cell _ _ _ hl₁
        rw [hd₁] at hc₁; cases hc₁
        obtain ⟨d₂, rfl⟩ := (running_iff hcr).1 ⟨d₁, rfl⟩
        have hsetL : settleCell μ l₀ e = μ.set l₀ (.done e) := by
          rcases settleCell_cases μ l₀ e with ⟨_, _, eq⟩ | ⟨hn, _⟩
          · exact eq
          · exact (hn d₁ hd₁).elim
        have hsetR : settleCell μ₁' l' v' = μ₁'.set l' (.done v') := by
          rcases settleCell_cases μ₁' l' v' with ⟨_, _, eq⟩ | ⟨hn, _⟩
          · exact eq
          · exact (hn d₂ hc₂).elim
        have hdoneL : (settleCell μ l₀ e)[l₀]? = some (.done e) := by
          rw [hsetL, List.getElem?_set_self (hst₁.lt₁ hl₁)]
        have hdoneR : (settleCell μ₁' l' v')[l']? = some (.done v') := by
          rw [hsetR, List.getElem?_set_self (hst₁.lt₂ hl₁)]
        have hce : e.Closed :=
          closed_frame_inner (F := .settle l₀) (closed_frame_inner (F := .let_ _) (plug_closed hcl₁))
        have hcv' : v'.Closed :=
          closed_frame_inner (F := .settle l')
            (closed_frame_inner (F := .let_ _) (plug_closed (hcl₂ _ _ r₁'.ksteps)))
        have hst₂ := hst₁.settleSym hl₁ s₁
        have hW := World.le_addD W₁ l₀ l' e v'
        have hst₃ := hst₂.markDone hl₁ hv hv' hce hcv' (s₁.value_noIns hv') hdoneL hdoneR s₁
        exact ⟨rfl, W₁.addD l₀ l' e v', settleCell μ₁' l' v', _, r₁.le.trans hW,
          .inl (r₁'.ksteps.pos ⟨_, _, fun _ => ⟨_, .rt (E := .let_ _ :: E₂) (hE₂.cons' trivial) (.settleVal hv')⟩, .refl⟩),
          .inl (((hfr.mono r₁.le).mono hW).plugs (.againR3 (fun h => hB₁ (hW.B.2 h))
            (.inr ⟨rfl, rfl, rfl, rfl⟩) hv hv' hce hcv' (s₁.value_noIns hv') (s₁.mono hW) hc
            (fun i y hy => ((hσ i y hy).mono r₁.le).mono hW) hσn hσv hσv')),
          hst₃, KeepO.of_junk hst fun n hn => settleCell_other (fun h => hst.not_j₁ hl (h ▸ hn))⟩
      | settleRaise => cases hv
      | settleUnw => cases hv
  · obtain ⟨rfl, rfl, rfl⟩ := step_settle_raise hE₁ hv hs
    obtain ⟨W₁, μ₁', v', r₁, hv', s₁⟩ := rcu_frame he rfl rfl hv μ₂ hst (F := .settle l') trivial
      (fun h => (hnB h).elim)
    have r₁' := (r₁.frame (F := .let_ (c₀.subst (subForce (.loc l') σ'))) trivial).plug hE₂
    have hst₁ := r₁.st _ hst
    have hl₁ := r₁.le.L _ _ _ hl
    exact ⟨rfl, W₁, settleCell μ₁' l' (.raise v'), _, r₁.le,
      .inl (r₁'.ksteps.pos ⟨_, _, fun _ => ⟨_, .rt (E := .let_ _ :: E₂) (hE₂.cons' trivial) (.settleRaise hv')⟩, .refl⟩),
      .inl ((hfr.mono r₁.le).plugs (.letRaise (fun h => hnB (r₁.le.B.2 h)) hv hv' s₁)),
      hst₁.settleSym hl₁ (.raise s₁),
      KeepO.of_junk hst fun n hn => settleCell_other (fun h => hst.not_j₁ hl (h ▸ hn))⟩
  · obtain ⟨rfl, rfl, rfl⟩ := step_settle_unw hE₁ hs
    obtain ⟨W₁, μ₁', ℓ', k, u', r₁, hlk, s₁⟩ := ucu_frame he rfl rfl μ₂ hst (F := .settle l') trivial
      (fun h => (hnB h).elim)
    have r₁' := (r₁.frame (F := .let_ (c₀.subst (subForce (.loc l') σ'))) trivial).plug hE₂
    have hst₁ := r₁.st _ hst
    have hl₁ := r₁.le.L _ _ _ hl
    exact ⟨rfl, W₁, resetCell μ₁' l', _, r₁.le,
      .inl (r₁'.ksteps.pos ⟨_, _, fun _ => ⟨_, .rt (E := .let_ _ :: E₂) (hE₂.cons' trivial) .settleUnw⟩, .refl⟩),
      .inl ((hfr.mono r₁.le).plugs (.letUnw (fun h => hnB (r₁.le.B.2 h)) hlk s₁)),
      hst₁.resetSym hl₁,
      KeepO.of_junk hst fun n hn => resetCell_other (fun h => hst.not_j₁ hl (h ▸ hn))⟩

theorem act_sim_againR3 {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E₁ E₂ : List Frame} {l₀ l' : Nat} {v v' c₀ t' : Tm} {σ σ' : Nat → Tm}
    (hE₁ : OkE E₁) (hE₂ : OkE E₂) (hfr : FrL W E₁ E₂) (hst : StR W μ μ₂)
    (hzl : ¬ t'.LzTop .any)
    (hzr : ∀ μ'' t'', KSteps root μ₂ (plug E₂ (.let_ v' (c₀.subst (subForce (.loc l') σ')))) μ'' t'' →
      ¬ t''.LzTop .any)
    (hnB : ¬ W.B) (hd : W.D l₀ l' v v') (hv : Value v) (hv' : Value v') (hcv : v.Closed)
    (hnv' : v'.NoIns) (hvv : Sim W .any v v') (hc : c₀.Src)
    (hσ : ∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) (hσn : ∀ i, (σ' i).NoIns)
    (hs : Step root l Λ μ (plug E₁ (.let_ v (c₀.subst (subVar σ)))) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ KAns root W τ μ₂ (plug E₂ (.let_ v' (c₀.subst (subForce (.loc l') σ'))))
      μ μ' (plug E₁ (.let_ v (c₀.subst (subVar σ)))) t' := by
  have keep : KeepO W τ μ μ := fun _ _ _ _ _ h => h
  rcases Step.simple hE₁ (let_redex hv) trivial hs with ⟨_, hh, rfl, rfl, rfl⟩ | ⟨_, hh, -⟩
  · cases hh
    have hstep : KPos root μ₂ (plug E₂ (.let_ v' (c₀.subst (subForce (.loc l') σ'))))
        μ₂ (plug E₂ ((c₀.subst (subForce (.loc l') σ')).subst (inst [v']))) :=
      ⟨_, _, fun _ => ⟨_, .pure hE₂ (.let_ hv')⟩, .refl⟩
    have hsubD : SubD W (subDoneV v σ) (subDoneF l' v' σ') := by
      intro i
      match i with
      | 0 => exact ⟨.doneR hnB hd hv hcv, by simp [subDoneF, Tm.NoIns, Tm.Has], .inr (.inr trivial)⟩
      | 1 => exact ⟨hvv, hnv', .inl (hvv.value_any hv hv' .top)⟩
      | k + 2 => exact ⟨hσ k .any (by decide), hσn k, .inl (hσ k .top (by decide))⟩
    have hzl' : ¬ (c₀.subst (subDoneV v σ)).LzTop .any := by
      intro h; rw [← subVar_inst] at h; exact hzl (h.plug E₁)
    have hzr' : ¬ (c₀.subst (subDoneF l' v' σ')).LzTop .any := by
      intro h; rw [← subForce_inst] at h; exact hzr _ _ hstep.ksteps (h.plug E₂)
    have hsim : Sim W .any ((c₀.subst (subVar σ)).subst (inst [v]))
        ((c₀.subst (subForce (.loc l') σ')).subst (inst [v'])) := by
      rw [subVar_inst, subForce_inst]
      exact hc.subst_any hsubD (by decide) hzl' hzr'
    exact ⟨rfl, W, μ₂, _, World.le.refl _, .inl hstep, .inl (hfr.plugs hsim), hst, keep⟩
  · cases hh

end Flick.Equiv
