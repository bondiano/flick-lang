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

/-- A related context split at a frame that is no cell's frame. -/
theorem split_at {A E E₂ : List Frame} {G : Frame} (hG : ∀ n, G ≠ .settle n)
    (hfr : FrL W (A ++ G :: E) E₂) (hE₂ : OkE E₂) :
    ∃ A₂ G₂ B₂, E₂ = A₂ ++ G₂ :: B₂ ∧ FrL W A A₂ ∧ FrS W G G₂ ∧ FrL W E B₂ ∧ OkE A₂ ∧
      OkE B₂ ∧ G₂.ok := by
  obtain ⟨A₂, G₂, B₂, rfl, h₁, h₂, h₃⟩ := hfr.split_left hG
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
  refine focus_cases (E := F :: E) (r := .raise v) (hE.cons' hF) (raise_redex hv).1
    (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hR hcl₁
    hcl₂ hrg (.raise hE hF hnt hni hnc hv) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨e₂, rfl, h₁⟩ := hs.inv_raise
  obtain ⟨W₁, μ₁, v₂, r₁, hv₂, s₁⟩ := vcu_frame h₁ rfl hv μ₂' (F := .raise) trivial
    (fun _ => ⟨closed_frame_inner (F := .raise) (plug_closed (E := F :: E) hcl₁),
      closed_frame_inner (F := .raise) (plug_closed (hcl₂ _ _ hj.ksteps))⟩)
  obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB, -, hB₂, hF₂ok⟩ :=
    split_at (A := []) (ctl_ne_settle hnc) (hfr.mono r₁.le) hE₂
  obtain ⟨μ₃, r₃⟩ := RJ.unwind_raise (W := W₁) hv₂ μ₁ hA.nil_left
  have r₄ := r₃.plug (E := F₂ :: B₂) (hB₂.cons' hF₂ok)
  rw [← plug_append] at r₄
  exact TRes.last (hj.trans ((r₁.plug hE₂).trans r₄))
    (.raise hB₂ hF₂ok (fun h => hnt ((hF₂.isTry).2 h)) (fun h => hni ((hF₂.isInside).2 h))
      (fun h => hnc ((hF₂.isCtl).2 h)) hv₂) (World.le.refl _) (.inl (hB.plugs (.raise s₁)))
    (r₃.st _ (r₁.st _ (hj.st _ hst))) keepO_refl ((hrg.keep (hj.le.trans r₁.le) keepO_refl))

/-- **A signal leaves a frame.** -/
theorem unw_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {F : Frame} {p ℓ : Nat} {a t₂ : Tm} (hE : OkE E) (hF : F.ok)
    (hni : ¬ F.isInside) (hnc : ¬ F.isCtl) (hnh : ¬ F.catches p ℓ)
    (ht : Sim W .any (plug E (F.plug (.unw p ℓ a))) t₂) (hst : StR W μ μ₂) (hR : W.R = root)
    (hcl₁ : (plug E (F.plug (.unw p ℓ a))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ (plug E (.unw p ℓ a)) := by
  refine focus_cases (E := F :: E) (r := .unw p ℓ a) (hE.cons' hF) (unw_redex _ _ _).1
    (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hR hcl₁
    hcl₂ hrg (.unw hE hF hni hnc hnh) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨ℓ', k, a₂, rfl, hl, h₁⟩ := hs.inv_unw
  obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB, -, hB₂, hF₂ok⟩ := split_at (A := []) (ctl_ne_settle hnc) hfr hE₂
  obtain ⟨μ₃, r₃⟩ := RJ.unwind_unw (W := W') (p := p) (ℓ := ℓ') (a := a₂) μ₂' hA.nil_left
  have r₄ := r₃.plug (E := F₂ :: B₂) (hB₂.cons' hF₂ok)
  rw [← plug_append] at r₄
  have hst' := hj.st _ hst
  exact TRes.last (hj.trans r₄)
    (.unw hB₂ hF₂ok (fun h => hni ((hF₂.isInside).2 h)) (fun h => hnc ((hF₂.isCtl).2 h))
      (fun h => hnh ((hF₂.catches hst'.bij hl).2 h))) (World.le.refl _) (.inl (hB.plugs (.unw hl h₁)))
    (r₃.st _ hst') keepO_refl (hrg.keep hj.le keepO_refl)

end Flick.Equiv
