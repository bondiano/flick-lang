import Formal.Equiv.TRes

/-!
# Contextual equivalence: a head step of a thread

A thread about to force its own new cell forces it (`oneshot_sim`). A pure or runtime head step
that keeps the regions is answered after the right side catches up around the focus: by the left
side of a special pair (`dec_act`), by the same head step (`pure_sim`, `rt_sim`), or, when the
focus is the frame of a left cell, by nothing (`rt_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

theorem KeepO.mono {W' : World} {τ : Option (Nat × Nat)} {μ μ' : List Tm} (hW : W.le W')
    (h : KeepO W' τ μ μ') : KeepO W τ μ μ' :=
  fun n τ' e hτ ho he => h n τ' e hτ (hW.O _ _ ho) he

theorem _root_.Flick.Redex.app {r f a : Tm} (h : Redex r) (he : r = .app f a) : Value f ∧ Value a := by
  subst he
  have hf := h.2 (.appL a) f trivial rfl
  exact ⟨hf, h.2 (.appR f) a hf rfl⟩

/-- A special pair holds the focus. -/
theorem dec_act {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E : List Frame} {r t₂ t' : Tm} (hE : OkE E) (hst : StR W μ μ₂)
    (hcl₁ : (plug E r).Closed) (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed)
    (hs : Step root l Λ μ (plug E r) (.cfg Λ' μ' t'))
    (hd : ∃ W' μ₂' E₀ E₁ a b E₂, RJ W μ₂ t₂ W' μ₂' (plug E₂ b) ∧ E = E₀ ++ E₁ ∧
      a = plug E₀ r ∧ FrL W' E₁ E₂ ∧ OkE E₂ ∧ Act W' a b) :
    Λ' = Λ ∧ KAns root W τ μ₂ t₂ μ μ' (plug E r) t' := by
  obtain ⟨W', μ₂', E₀, E₁, a, b, E₂, hj, rfl, rfl, hfr, hE₂, hact⟩ := hd
  rw [plug_append] at hs hcl₁ ⊢
  have hk₀ : KSteps root μ₂ t₂ μ₂' (plug E₂ (b)) := hj.ksteps
  obtain ⟨rfl, W'', μ₂'', t₂'', hW, hk, ht, hst', hko⟩ :=
    act_sim (τ := τ) (fun G hG => hE G (List.mem_append_right _ hG)) hE₂ hfr hact (hj.st _ hst) hcl₁
      (fun μ'' t'' h => hcl₂ μ'' t'' (hk₀.trans h)) hs
  refine ⟨rfl, W'', μ₂'', t₂'', hj.le.trans hW, ?_, ht, hst', hko.mono hj.le⟩
  rcases hk with hk | ⟨rfl, rfl, hz⟩
  · exact .inl (hk₀.pos hk)
  · rcases hk₀.split with ⟨rfl, rfl⟩ | hk₀
    · exact .inr ⟨rfl, rfl, hz.pre hj⟩
    · exact .inl hk₀

/-- **A thread about to force its own new cell** forces it; the right side waits. -/
theorem oneshot_sim {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {t t₂ t' : Tm} (h : OneShot W μ τ t t₂) (hst : StR W μ μ₂)
    (hs : Step root l Λ μ t (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ KAns root W τ μ₂ t₂ μ μ' t t' := by
  obtain ⟨E, E', n, e, e', rfl, rfl, hE, hE', hfr, ho, hc, hee⟩ := h
  have hn := hst.own _ _ ho
  rcases Step.simple hE (force_redex n) trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, rfl⟩
  · cases hh
  · cases hh with
    | forcePending hc' =>
      rw [hc] at hc'; cases hc'
      have hr := hfr.plugs (.jL (by decide) hn hee)
      refine ⟨rfl, W, μ₂, _, World.le.refl _, .inr ⟨rfl, rfl, fun hB => ⟨?_, fun _ _ _ h => h⟩⟩,
        .inl hr, hst.setJ₁ hn _, ?_⟩
      · have h₁ := Sim.xpot hr (hst.setJ₁ hn (.running e)) hB
        have h₂ : xpot W μ (plug E (.force (.loc n))) = e.pot :=
          xpot_eq (q := (E, n, e)) ⟨hE, rfl, hn, hc⟩
        have h₃ := pot_plug E (.settle n e) (.force (.loc n))
        simp only [pot, h₁, h₂, Tm.pot] at h₃ ⊢
        omega
      intro n' τ' e' hτ ho' he
      have : n' ≠ n := fun e => hτ (hst.ownU _ _ _ (e ▸ ho') ho)
      rw [getElem?_set_of_ne this]; exact he
    | forceDone hc' => rw [hc] at hc'; cases hc'
    | forceRunning hc' hnl => rw [hc] at hc'; cases hc'; exact (hnl _ rfl).elim

/-- **A pure head step** of a thread. -/
theorem pure_sim {root : Roots} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {r r' t₂ : Tm} (hE : OkE E) (hh : PHead r r')
    (ht : Sim W .any (plug E r) t₂) (hst : StR W μ μ₂) (hcl₁ : (plug E r).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed)
    (hzl : ¬ (plug E r').LzTop .any)
    (hzr : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → ¬ t''.LzTop .any) :
    KAns root W τ μ₂ t₂ μ μ (plug E r) (plug E r') := by
  have hr := hh.redex
  rcases ht.dec hr.1 (fun _ _ he => Redex.app hr he) (fun _ _ _ he => by subst he; cases hh)
      (fun ha => by cases hh <;> exact ha) _ _ rfl hE μ₂ with hd | ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs⟩ |
    ⟨W', μ₂', E₂, n, a, b, hj, hfr, hE₂, rfl, hn, hs⟩
  · exact (dec_act (τ := τ) (Λ := []) hE hst hcl₁ hcl₂ (.pure hE hh) hd).2
  · obtain ⟨W₁, μ₁, a₂, r₂', hj₁, hh₂, hs₂⟩ := phead_sim hh hs μ₂' (fun h => hzl (h.plug E))
      (fun _ _ _ ha hp hz => hzr _ _ (hj.ksteps.trans ((ha.plug hE₂).ksteps.trans
        (.step (fun _ => ⟨_, .pure hE₂ hp⟩) .refl))) (hz.plug E₂))
    exact ⟨W₁, μ₁, plug E₂ r₂', hj.le.trans hj₁.le,
      .inl (hj.ksteps.pos ((hj₁.plug hE₂).ksteps.pos ⟨_, _, fun _ => ⟨_, .pure hE₂ hh₂⟩, .refl⟩)),
      .inl ((hfr.mono hj₁.le).plugs hs₂), hj₁.st _ (hj.st _ hst), fun _ _ _ _ _ h => h⟩
  · cases hh

/-- The runtime head steps that keep the regions keep them. -/
theorem _root_.Flick.RHead.keep {Λ Λ' : List Rgn} {μ μ' : List Tm} {r r' : Tm} (hh : RHead Λ μ r Λ' μ' r')
    (hreg : ¬ r.Reg) : Λ' = Λ := by
  cases hh <;> first | rfl | exact (hreg trivial).elim

/-- **A runtime head step** of a thread that keeps the regions. -/
theorem rt_sim {root : Roots} {Λ Λ' : List Rgn} {μ μ' μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {r r' t₂ : Tm} (hE : OkE E) (hh : RHead Λ μ r Λ' μ' r') (hreg : ¬ r.Reg)
    (ht : Sim W .any (plug E r) t₂) (hst : StR W μ μ₂) (hcl₁ : (plug E r).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed)
    (hzl : ¬ (plug E r').LzTop .any)
    (hzr : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → ¬ t''.LzTop .any) :
    Λ' = Λ ∧ KAns root W τ μ₂ t₂ μ μ' (plug E r) (plug E r') := by
  have hΛ := hh.keep hreg
  have happ : ∀ f a, r = .app f a → Value f ∧ Value a := by
    intro f a he
    rcases hh.redex' with h | ⟨F, v, -, -, rfl⟩ | ⟨F, q, ℓ, c, -, rfl⟩
    · exact Redex.app h he
    · cases F <;> simp [Frame.plug] at he <;> cases hh <;> (rename_i hv; cases hv)
    · cases F <;> simp [Frame.plug] at he <;> cases hh <;> (rename_i hv; cases hv)
  rcases ht.dec hh.not_value happ (fun _ _ _ he => by subst he; cases hh)
      (fun ha => by cases hh <;> exact ha) _ _ rfl hE μ₂ with hd | ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs⟩ |
    ⟨W', μ₂', E₂, n, a, b, hj, hfr, hE₂, rfl, hn, hs⟩
  · exact dec_act hE hst hcl₁ hcl₂ (.rt hE hh) hd
  · obtain ⟨W₁, μ₁, r₁, W'', μ₂'', r₂', hj₁, hq, hW, hs₂, hst', hjk⟩ :=
      rhead_sim hh hreg hs (hj.st _ hst) (fun h => hzl (h.plug E))
        (fun _ hr hz => hzr _ _ (hj.ksteps.trans (.step (fun Λ => ⟨_, .rt hE₂ (hr Λ)⟩) .refl))
          (hz.plug E₂))
    refine ⟨hΛ, W'', μ₂'', plug E₂ r₂', hj.le.trans (hj₁.le.trans hW), ?_,
      .inl ((hfr.mono (hj₁.le.trans hW)).plugs hs₂), hst', ?_⟩
    · rcases hq.kpos (root := root) hE₂ with hq | ⟨hB, rfl, rfl⟩
      · exact .inl (hj.ksteps.pos ((hj₁.plug hE₂).ksteps.pos hq))
      · exact (hj.ksteps.trans (hj₁.plug hE₂).ksteps).ans ((hj.le.trans hj₁.le).B.2 hB)
    exact KeepO.of_junk hst fun n hn' => hjk n (hj.le.J₁ _ hn')
  · have hst₁ := hj.st _ hst
    have hko : ∀ {μ' : List Tm}, (∀ m e, μ[m]? = some (Tm.lazy e) → μ'[m]? = some (Tm.lazy e)) →
        KeepO W τ μ μ' := fun h _ _ _ _ _ he => h _ _ he
    -- the frame of a left cell settles: the right side does not step after it caught up
    have ans : ∀ {μ' : List Tm} {a' : Tm}, Sim W' .any (plug E a') (plug E₂ b) →
        StR W' μ' μ₂' → a'.pot < (Tm.settle n a).pot →
        KPos root μ₂ t₂ μ₂' (plug E₂ b) ∨ (μ₂' = μ₂ ∧ plug E₂ b = t₂ ∧
          Zero W W' τ μ μ' (plug E (.settle n a)) (plug E a')) := fun hr hs' hlt => by
      rcases hj.ksteps.split with ⟨rfl, e⟩ | hk
      · exact .inr ⟨rfl, e, Zero.sim hr hs' hj.le (pot_plug_lt E hlt) fun _ _ _ h => hj.o _ _ h⟩
      · exact .inl hk
    cases hh with
    | settleVal hv =>
      refine ⟨hΛ, W', μ₂', _, hj.le, ans (hfr.plugs hs) (hst₁.settleJ₁ hn _) (by simp only [Tm.pot]; omega),
        .inl (hfr.plugs hs), hst₁.settleJ₁ hn _,
        hko fun m e he => ?_⟩
      by_cases hm : m = n
      · subst hm; rcases settleCell_cases μ m _ with ⟨e', he', -⟩ | ⟨-, e₁⟩
        · rw [he] at he'; cases he'
        · rw [e₁]; exact he
      · rw [settleCell_other hm]; exact he
    | settleRaise hv =>
      refine ⟨hΛ, W', μ₂', _, hj.le, ans (hfr.plugs hs) (hst₁.settleJ₁ hn _) (by simp only [Tm.pot]; omega),
        .inl (hfr.plugs hs), hst₁.settleJ₁ hn _,
        hko fun m e he => ?_⟩
      by_cases hm : m = n
      · subst hm; rcases settleCell_cases μ m (.raise _) with ⟨e', he', -⟩ | ⟨-, e₁⟩
        · rw [he] at he'; cases he'
        · rw [e₁]; exact he
      · rw [settleCell_other hm]; exact he
    | settleUnw =>
      refine ⟨hΛ, W', μ₂', _, hj.le, ans (hfr.plugs hs) (hst₁.resetJ₁ hn) (by simp only [Tm.pot]; omega),
        .inl (hfr.plugs hs), hst₁.resetJ₁ hn,
        hko fun m e he => ?_⟩
      by_cases hm : m = n
      · subst hm; rcases resetCell_cases μ m with ⟨e', he', -⟩ | ⟨-, e₁⟩
        · rw [he] at he'; cases he'
        · rw [e₁]; exact he
      · rw [resetCell_other hm]; exact he

end Flick.Equiv
