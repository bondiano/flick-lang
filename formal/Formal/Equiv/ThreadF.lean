import Formal.Equiv.ThreadE

/-!
# Contextual equivalence: operations and `resume`

An operation reaches the nearest handler with an arm for it (`abort_sim`, `tail_sim`, `ctl_sim`,
`kept_sim`), and `resume` runs the parked body (`resume_sim`): the right side, related frame by
frame, takes the same step.
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel perform_redex kont_redex)

variable {W : World}

theorem perform_shape {Ef o : Name} {c : OpClass} {v : Tm} (hv : Value v) :
    (∀ f a, Tm.app (.const (.perform Ef o c)) v = .app f a → Value f ∧ Value a) :=
  fun _ _ h => Redex.app (perform_redex hv) h

/-- The right side of an operation, caught up. -/
theorem perform_right {W' : World} {Ef o : Name} {c : OpClass} {v r₂ : Tm} (hv : Value v)
    (hs : Sim W' .top (.app (.const (.perform Ef o c)) v) r₂) :
    ∃ v₂, r₂ = .app (.const (.perform Ef o c)) v₂ ∧ Value v₂ ∧ Sim W' .top v v₂ := by
  obtain ⟨c₂, v₂, rfl, h₁, h₂⟩ := hs.inv_app
  obtain rfl := h₁.inv_const
  exact ⟨v₂, rfl, h₂.top_value rfl hv, h₂⟩

theorem armOf_right {W' : World} {al al₂ b : Tm} {o : Name} (h : Sim W' .top al al₂)
    (ha : al.armOf o = some b) : ∃ b₂, al₂.armOf o = some b₂ ∧ ArmR W' b b₂ :=
  (armOf_eq (o := o) h rfl).some_left ha

/-- **An operation a handler aborts.** -/
theorem abort_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E Ei : List Frame} {Ef o : Name} {b v r al t₂ : Tm} {pp ℓℓ : Nat} (hE : OkE E)
    (hEi : OkE Ei) (hh : ∀ G ∈ Ei, G.handles Ef o = none) (ha : al.armOf o = some b) (hv : Value v)
    (ht : Sim W .any (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .abort)) v))))
      t₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .abort)) v)))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hzl : ¬ (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.unw pp ℓℓ (b.subst (inst [v])))))).LzTop .any)
    (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ
      (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.unw pp ℓℓ (b.subst (inst [v])))))) := by
  have hs₀ := Step.performAbort (root := root) (Λ := Λ) (μ := μ) (p := pp) (ℓ := ℓℓ) (r := r)
    hE hEi hh ha hv
  rw [show plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .abort)) v))) =
    plug (Ei ++ .hwith pp ℓℓ Ef al r :: E) (.app (.const (.perform Ef o .abort)) v) by
      rw [plug_append]; rfl] at ht hcl₁ hs₀
  refine focus_cases (hEi.append' (hE.cons' (F := .hwith pp ℓℓ Ef al r) trivial)) (perform_redex hv).1
    (perform_shape hv)
    (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg hs₀ ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨v₂, rfl, hv₂, h₂⟩ := perform_right hv hs
  obtain ⟨A₂, G₂, B₂, rfl, hA, hG, hB, hA₂, hB₂, -⟩ := split_at (fun _ h => by cases h) hfr hE₂
  cases hG with
  | @hwith _ _ ℓ' _ _ _ al₂ _ r₂' hl hal hr =>
    obtain ⟨b₂, hb₂, hbb⟩ := armOf_right hal ha
    have hs₂ := Step.performAbort (root := root) (Λ := Λ₂) (μ := μ₂') (p := pp) (ℓ := ℓ') (r := r₂')
      hB₂ hA₂ (hA.handlesE hh) hb₂ hv₂
    rw [show plug B₂ (.hwith pp ℓ' Ef al₂ r₂' (plug A₂ (.app (.const (.perform Ef o .abort)) v₂))) =
      plug (A₂ ++ .hwith pp ℓ' Ef al₂ r₂' :: B₂) (.app (.const (.perform Ef o .abort)) v₂) by
        rw [plug_append]; rfl] at hs₂
    rcases hbb with hbb | ⟨hd | hd, -⟩
    rotate_left
    · exact (hzl ((Tm.LzTop.hwith ((Tm.LzTop.unw (hd.subst _)).plug Ei)).plug E)).elim
    · exact (hzr _ _ _ ⟨_, _, hj.ksteps, .inr ⟨_, hs₂⟩⟩ ((Tm.LzTop.hwith ((Tm.LzTop.unw (hd.subst _)).plug A₂)).plug B₂)).elim
    exact TRes.last hj hs₂ (World.le.refl _) (.inl (hB.plugs (.hwith hl hal hr (hA.plugs
      (.unw hl (hbb.subst _ _ (SubR.inst (.cons (h₂.sub hv hv₂) .nil))))))))
      (hj.st _ hst) keepO_refl (hrg.keep hj.le keepO_refl)

/-- **An operation a `tail` arm answers.** -/
theorem tail_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E Ei : List Frame} {Ef o : Name} {b v r al t₂ : Tm} {pp ℓℓ : Nat} (hE : OkE E)
    (hEi : OkE Ei) (hh : ∀ G ∈ Ei, G.handles Ef o = none) (ha : al.armOf o = some b) (hv : Value v)
    (ht : Sim W .any (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .tail)) v))))
      t₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .tail)) v)))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hzl : ¬ (plug E (.hctl pp ℓℓ 0 true Ef al r (b.subst (inst [v])) (plug Ei (.hole pp)))).LzTop .any)
    (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ
      (plug E (.hctl pp ℓℓ 0 true Ef al r (b.subst (inst [v])) (plug Ei (.hole pp)))) := by
  have hs₀ := Step.performTail (root := root) (Λ := Λ) (μ := μ) (p := pp) (ℓ := ℓℓ) (r := r)
    hE hEi hh ha hv
  rw [show plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .tail)) v))) =
    plug (Ei ++ .hwith pp ℓℓ Ef al r :: E) (.app (.const (.perform Ef o .tail)) v) by
      rw [plug_append]; rfl] at ht hcl₁ hs₀
  refine focus_cases (hEi.append' (hE.cons' (F := .hwith pp ℓℓ Ef al r) trivial)) (perform_redex hv).1
    (perform_shape hv)
    (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg hs₀ ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨v₂, rfl, hv₂, h₂⟩ := perform_right hv hs
  obtain ⟨A₂, G₂, B₂, rfl, hA, hG, hB, hA₂, hB₂, -⟩ := split_at (fun _ h => by cases h) hfr hE₂
  cases hG with
  | @hwith _ _ ℓ' _ _ _ al₂ _ r₂' hl hal hr =>
    obtain ⟨b₂, hb₂, hbb⟩ := armOf_right hal ha
    have hs₂ := Step.performTail (root := root) (Λ := Λ₂) (μ := μ₂') (p := pp) (ℓ := ℓ') (r := r₂')
      hB₂ hA₂ (hA.handlesE hh) hb₂ hv₂
    rw [show plug B₂ (.hwith pp ℓ' Ef al₂ r₂' (plug A₂ (.app (.const (.perform Ef o .tail)) v₂))) =
      plug (A₂ ++ .hwith pp ℓ' Ef al₂ r₂' :: B₂) (.app (.const (.perform Ef o .tail)) v₂) by
        rw [plug_append]; rfl] at hs₂
    rcases hbb with hbb | ⟨hd | hd, -⟩
    rotate_left
    · exact (hzl ((Tm.LzTop.hctl (hd.subst _)).plug E)).elim
    · exact (hzr _ _ _ ⟨_, _, hj.ksteps, .inr ⟨_, hs₂⟩⟩ ((Tm.LzTop.hctl (hd.subst _)).plug B₂)).elim
    exact TRes.last hj hs₂ (World.le.refl _) (.inl (hB.plugs (.hctl hl (.inl ⟨rfl, rfl⟩) hal hr
      (hbb.subst _ _ (SubR.inst (.cons (h₂.sub hv hv₂) .nil))) (hA.plugsP hEi hA₂ trivial .hole))))
      (hj.st _ hst) keepO_refl (hrg.keep hj.le keepO_refl)

/-- **An operation a `control` arm answers**, with a new cell for `resume`. -/
theorem ctl_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E Ei : List Frame} {Ef o : Name} {b v r al t₂ : Tm} {pp ℓℓ : Nat} (hE : OkE E)
    (hEi : OkE Ei) (hh : ∀ G ∈ Ei, G.handles Ef o = none) (ha : al.armOf o = some b) (hv : Value v)
    (ht : Sim W .any (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .control)) v))))
      t₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .control)) v)))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hzl : ¬ (plug E (.hctl pp ℓℓ μ.length false Ef al r (b.subst (inst [v, .kont pp μ.length]))
        (plug Ei (.hole pp)))).LzTop .any)
    (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ (μ ++ [.lam .todo])
      (plug E (.hctl pp ℓℓ μ.length false Ef al r (b.subst (inst [v, .kont pp μ.length]))
        (plug Ei (.hole pp)))) := by
  have hs₀ := Step.performCtl (root := root) (Λ := Λ) (μ := μ) (p := pp) (ℓ := ℓℓ) (r := r)
    hE hEi hh ha hv
  rw [show plug E (.hwith pp ℓℓ Ef al r (plug Ei (.app (.const (.perform Ef o .control)) v))) =
    plug (Ei ++ .hwith pp ℓℓ Ef al r :: E) (.app (.const (.perform Ef o .control)) v) by
      rw [plug_append]; rfl] at ht hcl₁ hs₀
  refine focus_cases (hEi.append' (hE.cons' (F := .hwith pp ℓℓ Ef al r) trivial)) (perform_redex hv).1
    (perform_shape hv)
    (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg hs₀ ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨v₂, rfl, hv₂, h₂⟩ := perform_right hv hs
  obtain ⟨A₂, G₂, B₂, rfl, hA, hG, hB, hA₂, hB₂, -⟩ := split_at (fun _ h => by cases h) hfr hE₂
  cases hG with
  | @hwith _ _ ℓ' _ _ _ al₂ _ r₂' hl hal hr =>
    obtain ⟨b₂, hb₂, hbb⟩ := armOf_right hal ha
    have hs₂ := Step.performCtl (root := root) (Λ := Λ₂) (μ := μ₂') (p := pp) (ℓ := ℓ') (r := r₂')
      hB₂ hA₂ (hA.handlesE hh) hb₂ hv₂
    rw [show plug B₂ (.hwith pp ℓ' Ef al₂ r₂' (plug A₂ (.app (.const (.perform Ef o .control)) v₂))) =
      plug (A₂ ++ .hwith pp ℓ' Ef al₂ r₂' :: B₂) (.app (.const (.perform Ef o .control)) v₂) by
        rw [plug_append]; rfl] at hs₂
    rcases hbb with hbb | ⟨hd | hd, -⟩
    rotate_left
    · exact (hzl ((Tm.LzTop.hctl (hd.subst _)).plug E)).elim
    · exact (hzr _ _ _ ⟨_, _, hj.ksteps, .inr ⟨_, hs₂⟩⟩ ((Tm.LzTop.hctl (hd.subst _)).plug B₂)).elim
    have hst' := hj.st _ hst
    have hW := World.le_addL W' μ.length μ₂'.length .sym
    have hκ : (World.addL W' μ.length μ₂'.length .sym).L μ.length μ₂'.length .sym := .inr ⟨rfl, rfl, rfl⟩
    exact TRes.last hj hs₂ hW (.inl ((hB.mono hW).plugs (.hctl (hW.L _ _ _ hl) (.inr ⟨rfl, _, hκ⟩)
      (hal.mono hW) (hr.mono hW) ((hbb.mono hW).subst _ _ (SubR.inst (.cons ((h₂.mono hW).sub hv hv₂)
        (.cons (Sim.sub (Sim.kont (x := .any) hκ) .kont .kont) .nil))))
      ((hA.mono hW).plugsP hEi hA₂ trivial .hole))))
      (hst'.alloc (.lam .todo (by simp [Tm.NoIns, Tm.Has]))) (KeepO.append hst _)
      ((hrg.keep hj.le keepO_refl).keep hW (KeepO.append hst' _))

end Flick.Equiv
