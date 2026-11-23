import Formal.Equiv.ThreadD

/-!
# Contextual equivalence: errors, signals and operations

A raised error or a signal leaves a frame (`raise_sim`, `unw_sim`) and an operation reaches its
handler (`abort_sim`, `tail_sim`, `ctl_sim`, `kept_sim`): the right side leaves the frames of its
own cells and takes the same step.
-/

namespace Flick.Equiv

open Flick.Elab (All2 raise_redex unw_redex perform_redex)

variable {W : World}

/-- A related context split at a frame that is neither a `settle` nor a `let`. -/
theorem split_at {A E E₂ : List Frame} {G : Frame} (hG : ∀ n, G ≠ .settle n)
    (hGl : ∀ b, G ≠ .let_ b) (hfr : FrL W (A ++ G :: E) E₂) (hE₂ : OkE E₂) :
    ∃ A₂ G₂ B₂, E₂ = A₂ ++ G₂ :: B₂ ∧ FrL W A A₂ ∧ FrS W G G₂ ∧ FrL W E B₂ ∧ OkE A₂ ∧
      OkE B₂ ∧ G₂.ok := by
  obtain ⟨A₂, G₂, B₂, rfl, h₁, h₂, h₃⟩ := hfr.split_left hG hGl
  exact ⟨A₂, G₂, B₂, rfl, h₁, h₂, h₃, fun F hF => hE₂ F (List.mem_append_left _ hF),
    fun F hF => hE₂ F (List.mem_append_right _ (List.mem_cons_of_mem _ hF)),
    hE₂ G₂ (List.mem_append_right _ (List.mem_cons_self ..))⟩

/-- A related context split at its head frame when that frame is not a `settle`. -/
theorem split_head_at {E E₂ : List Frame} {G : Frame} (hG : ∀ n, G ≠ .settle n)
    (hfr : FrL W (G :: E) E₂) (hE₂ : OkE E₂) :
    ∃ A₂ G₂ B₂, E₂ = A₂ ++ G₂ :: B₂ ∧ FrL W [] A₂ ∧ FrS W G G₂ ∧ FrL W E B₂ ∧ OkE A₂ ∧
      OkE B₂ ∧ G₂.ok := by
  obtain ⟨A₂, G₂, B₂, rfl, h₁, h₂, h₃⟩ := hfr.split_head hG
  exact ⟨A₂, G₂, B₂, rfl, h₁, h₂, h₃, fun F hF => hE₂ F (List.mem_append_left _ hF),
    fun F hF => hE₂ F (List.mem_append_right _ (List.mem_cons_of_mem _ hF)),
    hE₂ G₂ (List.mem_append_right _ (List.mem_cons_self ..))⟩

theorem ctl_ne_settle {F : Frame} (h : ¬ F.isCtl) : ∀ n, F ≠ .settle n := by
  rintro n rfl; exact h trivial

/-- **A raised error leaves a frame.** -/
theorem raise_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {F : Frame} {v t₂ : Tm} (hE : OkE E) (hF : F.ok) (hnt : ¬ F.isTry)
    (hni : ¬ F.isInside) (hnc : ¬ F.isCtl) (hv : Value v)
    (ht : Sim W .any (plug E (F.plug (.raise v))) t₂) (hst : StR W μ μ₂) (hR : W.R = root)
    (hcl₁ : (plug E (F.plug (.raise v))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ (plug E (.raise v)) := by
  have hs₀ : Step root .rt Λ μ (plug (F :: E) (.raise v)) (.cfg Λ μ (plug E (.raise v))) :=
    .raise hE hF hnt hni hnc hv
  rcases ht.dec (raise_redex hv).1 (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
      (fun _ _ h => by cases h) (fun _ _ h => by cases h)
      _ _ rfl (hE.cons' hF) μ₂ hst (fun _ => ⟨hcl₁, hcl₂ _ _ .refl⟩) with
    ⟨W', μ₂', E₀, E₁, a, b, E₂, hj, hEq, ha, hfr, hE₂, hact⟩ |
    ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs⟩ | ⟨_, _, _, _, _, _, _, _, _, h', _, _⟩
  · have hE₀ : OkE E₀ := fun G hG => (hE.cons' hF) G (hEq ▸ List.mem_append_left _ hG)
    rcases hact.eq_raise (hj.st _ hst) hv hnc hE₀ hEq ha with
      ⟨v', b₁, b₂, rfl, rfl, rfl, rfl, hv', hvv⟩ | ⟨l₀, l', e, t, rfl, hH, hhl, hpt, hct⟩
    · exact TRes.last hj (.raise hE₂ (F := .let_ b₂) trivial (by simp [Frame.isTry])
        (by simp [Frame.isInside]) (by simp [Frame.isCtl]) hv') (World.le.refl _)
        (.inl (hfr.plugs (.raise hvv))) (hj.st _ hst) keepO_refl (hrg.keep hj.le keepO_refl)
    · exfalso
      obtain ⟨-, -, -, -, -, hpe, -⟩ := (hj.st _ hst).hl l₀ e hhl
      have hR' : W'.R = root := hj.le.R.symm.trans hR
      have hnv : ¬ Value t := by
        rcases snoc_or E₀ with rfl | ⟨E₀', G, rfl⟩
        · simp only [plug] at ha; cases ha
        · rw [plug_snoc] at ha
          cases G <;> simp only [Frame.plug, reduceCtorEq, Tm.settle.injEq] at ha
          exact ha.2 ▸ not_value_plugs (raise_redex hv).1 E₀'
      rw [hEq, plug_append, ← ha] at hs₀
      have hEq' := (PureEval.step_settle (fun G hG => (hE.cons' hF) G (hEq ▸ List.mem_append_right _ hG))
        (hR' ▸ hpe) (hR' ▸ hpt) hnv hs₀).1
      cases hEq'
  · obtain ⟨e₂, rfl, h₁⟩ := hs.inv_raise
    obtain ⟨W₁, μ₁, v₂, r₁, hv₂, s₁⟩ := vcu_frame h₁ rfl hv μ₂' (hj.st _ hst) (F := .raise) trivial
      (fun _ => ⟨closed_frame_inner (F := .raise) (plug_closed (E := F :: E) hcl₁),
        closed_frame_inner (F := .raise) (plug_closed (hcl₂ _ _ hj.ksteps))⟩)
    obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB, -, hB₂, hF₂ok⟩ :=
      split_head_at (ctl_ne_settle hnc) (hfr.mono r₁.le) hE₂
    obtain ⟨μ₃, r₃⟩ := RJ.unwind_raise (W := W₁) hv₂ μ₁ hA.nil_left
    have r₄ := r₃.plug (E := F₂ :: B₂) (hB₂.cons' hF₂ok)
    rw [← plug_append] at r₄
    exact TRes.last (hj.trans ((r₁.plug hE₂).trans r₄))
      (.raise hB₂ hF₂ok (fun h => hnt ((hF₂.isTry).2 h)) (fun h => hni ((hF₂.isInside).2 h))
        (fun h => hnc ((hF₂.isCtl).2 h)) hv₂) (World.le.refl _) (.inl (hB.plugs (.raise s₁)))
      (r₃.st _ (r₁.st _ (hj.st _ hst))) keepO_refl ((hrg.keep (hj.le.trans r₁.le) keepO_refl))
  · cases h'

/-- **A signal leaves a frame.** -/
theorem unw_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {F : Frame} {p ℓ : Nat} {a t₂ : Tm} (hE : OkE E) (hF : F.ok)
    (hni : ¬ F.isInside) (hnc : ¬ F.isCtl) (hnh : ¬ F.catches p ℓ)
    (ht : Sim W .any (plug E (F.plug (.unw p ℓ a))) t₂) (hst : StR W μ μ₂) (hR : W.R = root)
    (hcl₁ : (plug E (F.plug (.unw p ℓ a))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ (plug E (.unw p ℓ a)) := by
  have hs₀ : Step root .rt Λ μ (plug (F :: E) (.unw p ℓ a)) (.cfg Λ μ (plug E (.unw p ℓ a))) :=
    .unw hE hF hni hnc hnh
  rcases ht.dec (unw_redex _ _ _).1 (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
      (fun _ _ h => by cases h) (fun _ _ h => by cases h)
      _ _ rfl (hE.cons' hF) μ₂ hst (fun _ => ⟨hcl₁, hcl₂ _ _ .refl⟩) with
    ⟨W', μ₂', E₀, E₁, a₀, b₀, E₂, hj, hEq, ha, hfr, hE₂, hact⟩ |
    ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs⟩ | ⟨_, _, _, _, _, _, _, _, _, h', _, _⟩
  · have hE₀ : OkE E₀ := fun G hG => (hE.cons' hF) G (hEq ▸ List.mem_append_left _ hG)
    rcases hact.eq_unw (hj.st _ hst) hnc hE₀ hEq ha with
      ⟨ℓ', k, u', b₁, b₂, rfl, rfl, rfl, rfl, hl, hu⟩ | ⟨l₀, l', e, t, rfl, hH, hhl, hpt, hct⟩
    · exact TRes.last hj (.unw hE₂ (F := .let_ b₂) trivial (by simp [Frame.isInside])
        (by simp [Frame.isCtl]) (by simp [Frame.catches])) (World.le.refl _)
        (.inl (hfr.plugs (.unw hl hu))) (hj.st _ hst) keepO_refl (hrg.keep hj.le keepO_refl)
    · exfalso
      obtain ⟨-, -, -, -, -, hpe, -⟩ := (hj.st _ hst).hl l₀ e hhl
      have hR' : W'.R = root := hj.le.R.symm.trans hR
      have hnv : ¬ Value t := by
        rcases snoc_or E₀ with rfl | ⟨E₀', G, rfl⟩
        · simp only [plug] at ha; cases ha
        · rw [plug_snoc] at ha
          cases G <;> simp only [Frame.plug, reduceCtorEq, Tm.settle.injEq] at ha
          exact ha.2 ▸ not_value_plugs (unw_redex _ _ _).1 E₀'
      rw [hEq, plug_append, ← ha] at hs₀
      have hEq' := (PureEval.step_settle (fun G hG => (hE.cons' hF) G (hEq ▸ List.mem_append_right _ hG))
        (hR' ▸ hpe) (hR' ▸ hpt) hnv hs₀).1
      cases hEq'
  · obtain ⟨ℓ', k, a₂, rfl, hl, h₁⟩ := hs.inv_unw
    obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB, -, hB₂, hF₂ok⟩ := split_head_at (ctl_ne_settle hnc) hfr hE₂
    obtain ⟨μ₃, r₃⟩ := RJ.unwind_unw (W := W') (p := p) (ℓ := ℓ') (a := a₂) μ₂' hA.nil_left
    have r₄ := r₃.plug (E := F₂ :: B₂) (hB₂.cons' hF₂ok)
    rw [← plug_append] at r₄
    have hst' := hj.st _ hst
    exact TRes.last (hj.trans r₄)
      (.unw hB₂ hF₂ok (fun h => hni ((hF₂.isInside).2 h)) (fun h => hnc ((hF₂.isCtl).2 h))
        (fun h => hnh ((hF₂.catches hst'.bij hl).2 h))) (World.le.refl _) (.inl (hB.plugs (.unw hl h₁)))
      (r₃.st _ hst') keepO_refl (hrg.keep hj.le keepO_refl)
  · cases h'

end Flick.Equiv
