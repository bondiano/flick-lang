import Formal.Equiv.ThreadF

/-!
# Contextual equivalence: kept handlers and `resume`

`kept_sim`: an operation of a child reaches a handler kept from where it started; `resume_sim`:
`resume` runs the parked body under its handler. The right side takes the same step: its parked
body is related frame by frame, ordered (`Sim.plug_hole`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel perform_redex kont_redex)

variable {W : World}

theorem RgsR.live {μ : List Tm} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn} (h : RgsR W μ τ Λ Λ₂)
    {q : Nat} (hq : Live Λ q) : Live Λ₂ q := by
  obtain ⟨g, hg, hl⟩ := hq
  obtain ⟨g₂, hg₂, hr⟩ := h.get hg
  exact ⟨g₂, hg₂, hr.live ▸ hl⟩

/-- **An operation a kept handler answers.** -/
theorem kept_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E Ei : List Frame} {Ef o : Name} {b v al t₂ : Tm} {q ℓℓ : Nat} (hE : OkE E)
    (hEi : OkE Ei) (hh : ∀ G ∈ Ei, G.handles Ef o = none) (ha : al.armOf o = some b) (hv : Value v)
    (hq : Live Λ q)
    (ht : Sim W .any (plug E (.hkeep q ℓℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v))))
      t₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E (.hkeep q ℓℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v)))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hzl : ¬ (plug E (.hkarm q ℓℓ Ef al (b.subst (inst [v])) (plug Ei (.hole q)))).LzTop .any)
    (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ
      (plug E (.hkarm q ℓℓ Ef al (b.subst (inst [v])) (plug Ei (.hole q)))) := by
  have hs₀ := Step.performKept (root := root) (μ := μ) (ℓ := ℓℓ) hE hEi hh ha hv hq
  rw [show plug E (.hkeep q ℓℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v))) =
    plug (Ei ++ .hkeep q ℓℓ Ef al :: E) (.app (.const (.perform Ef o .tail)) v) by
      rw [plug_append]; rfl] at ht hcl₁ hs₀
  refine focus_cases (hEi.append' (hE.cons' (F := .hkeep q ℓℓ Ef al) trivial)) (perform_redex hv).1
    (perform_shape hv)
    (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg hs₀ ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨v₂, rfl, hv₂, h₂⟩ := perform_right hv hs
  obtain ⟨A₂, G₂, B₂, rfl, hA, hG, hB, hA₂, hB₂, -⟩ := split_at (fun _ h => by cases h) hfr hE₂
  cases hG with
  | @hkeep _ _ ℓ' _ _ _ al₂ hl hal =>
    obtain ⟨b₂, hb₂, hbb⟩ := armOf_right hal ha
    have hs₂ := Step.performKept (root := root) (Λ := Λ₂) (μ := μ₂') (ℓ := ℓ')
      hB₂ hA₂ (hA.handlesE hh) hb₂ hv₂ (hrg.live hq)
    rw [show plug B₂ (.hkeep q ℓ' Ef al₂ (plug A₂ (.app (.const (.perform Ef o .tail)) v₂))) =
      plug (A₂ ++ .hkeep q ℓ' Ef al₂ :: B₂) (.app (.const (.perform Ef o .tail)) v₂) by
        rw [plug_append]; rfl] at hs₂
    rcases hbb with hbb | ⟨hd | hd, -⟩
    rotate_left
    · exact (hzl ((Tm.LzTop.hkarm (hd.subst _)).plug E)).elim
    · exact (hzr _ _ _ ⟨_, _, hj.ksteps, .inr ⟨_, hs₂⟩⟩ ((Tm.LzTop.hkarm (hd.subst _)).plug B₂)).elim
    exact TRes.last hj hs₂ (World.le.refl _) (.inl (hB.plugs (.hkarm hl hal
      (hbb.subst _ _ (SubR.inst (.cons (h₂.sub hv hv₂) .nil))) (hA.plugsP hEi hA₂ trivial .hole))))
      (hj.st _ hst) keepO_refl (hrg.keep hj.le keepO_refl)

/-- **`resume`** runs the parked body. -/
theorem resume_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E A Ei : List Frame} {Ef : Name} {u r al t₂ : Tm} {p ℓ ℓh : Nat} (hE : OkE E) (hA : OkE A)
    (hEi : OkE Ei) (hn : ∀ G ∈ A, ¬ G.resumes p) (hu : Value u)
    (ht : Sim W .any (plug E (.hctl p ℓh ℓ false Ef al r (plug A (.app (.kont p ℓ) u))
      (plug Ei (.hole p)))) t₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E (.hctl p ℓh ℓ false Ef al r (plug A (.app (.kont p ℓ) u))
      (plug Ei (.hole p)))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ
      (plug E (.hres p ℓh (plug A (.hole p)) (.hwith p ℓh Ef al r (plug Ei u)))) := by
  have hs₀ := Step.resume (root := root) (Λ := Λ) (μ := μ) (r := r) (Ef := Ef) (al := al)
    (ℓh := ℓh) (ℓ := ℓ) hE hA hEi rfl hn hu
  rw [show plug E (.hctl p ℓh ℓ false Ef al r (plug A (.app (.kont p ℓ) u)) (plug Ei (.hole p))) =
    plug (A ++ .hctlA p ℓh ℓ false Ef al r (plug Ei (.hole p)) :: E) (.app (.kont p ℓ) u) by
      rw [plug_append]; rfl] at ht hcl₁ hs₀
  refine focus_cases (hA.append' (hE.cons' (F := .hctlA p ℓh ℓ false Ef al r _) trivial))
    (kont_redex hu).1 (fun _ _ h => Redex.app (kont_redex hu) h) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg hs₀ ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨k₂, u₂, rfl, h₁, h₂⟩ := hs.inv_app
  obtain ⟨ℓ₂, kd, rfl, hl₂⟩ := h₁.inv_kont
  have hu₂ := h₂.top_value rfl hu
  obtain ⟨A₂, G₂, B₂, rfl, hfA, hG, hB, hA₂, hB₂, -⟩ := split_at (fun _ h => by cases h) hfr hE₂
  have hst' := hj.st _ hst
  cases hG with
  | @hctlA _ _ ℓh' _ _ κ' _ _ _ al₂ _ r₂' _ k₂ hlh hk hal hr hc =>
    obtain ⟨Ei₂, rfl, hfE, hEi₂⟩ := hc.plug_hole hEi
    have hκ : κ' = ℓ₂ := by
      rcases hk with ⟨h, -⟩ | ⟨-, kk, hκ⟩
      · cases h
      · exact (hst'.bij _ _ _ _ _ _ hκ hl₂).1 rfl
    subst hκ
    have hs₂ := Step.resume (root := root) (Λ := Λ₂) (μ := μ₂') (r := r₂') (Ef := Ef) (al := al₂)
      (ℓh := ℓh') (ℓ := κ') hB₂ hA₂ hEi₂ rfl (hfA.resumesE hn) hu₂
    rw [show plug B₂ (.hctl p ℓh' κ' false Ef al₂ r₂' (plug A₂ (.app (.kont p κ') u₂))
        (plug Ei₂ (.hole p))) =
      plug (A₂ ++ .hctlA p ℓh' κ' false Ef al₂ r₂' (plug Ei₂ (.hole p)) :: B₂) (.app (.kont p κ') u₂) by
        rw [plug_append]; rfl] at hs₂
    exact TRes.last hj hs₂ (World.le.refl _)
      (.inl (hB.plugs (.hres hlh (hfA.plugsP hA hA₂ trivial .hole)
        (.hwith hlh hal hr (hfE.plugs h₂.lower))))) hst' keepO_refl (hrg.keep hj.le keepO_refl)

end Flick.Equiv
