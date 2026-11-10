import Formal.Elab.CoreHead

/-!
# λFlick⁺ as a `Target`: a step of related configurations

A step of one thread is matched by the other side (`thread_sim`): either the redex lies inside
the left side of a base pair, and the other side waits or reaches the value of the pair by
pure steps (`inside_sim`), or the same frames hold a hole related by a congruence rule, and
the other side takes the same step. `cstep_sim` lifts it to the main thread and the children
of the regions: `Rel = CfgR (DP S root)` is a weak simulation. `final_sim`: a literal of the
main thread is related only to itself.
-/

namespace Flick.Elab

variable {S : Sig} {root : Roots}

/-- A step of the configurations of the core. -/
def CS (root : Roots) (a b : Cfg) : Prop := ∃ l, CStep root l a b

theorem Star.plug {a b : Flick.Tm} (h : Star (PStep root) a b) {E : List Frame} (hE : OkE E) :
    Star (PStep root) (Flick.plug E a) (Flick.plug E b) := by
  induction E generalizing a b with
  | nil => exact h
  | cons F E ih => obtain ⟨hF, hE'⟩ := hE.cons; exact ih (h.frame hF) hE'

/-- The other side waits, or takes pure steps; store and regions stay. -/
def Waits (root : Roots) (B : Flick.Tm → Flick.Tm → Prop) (Λ Λ' : List Rgn)
    (μ μ' : List Flick.Tm) (t₂ t₁' : Flick.Tm) : Prop :=
  Λ' = Λ ∧ μ' = μ ∧ ∃ t₂', Star (PStep root) t₂ t₂' ∧ Cg B false t₁' t₂'

/-- **A step inside a base pair**: the other side waits, or reaches its value. -/
theorem inside_sim (hroot : RootTyM S .live root) {E₁ E₂ : List Frame} {x x₂ : Flick.Tm} (hE₁ : OkE E₁)
    (hfr : All2 (FrR (DP S root)) E₁ E₂) (hb : DP S root x x₂) {l : Lbl} {Λ Λ' : List Rgn}
    {μ μ' : List Flick.Tm} {t' : Flick.Tm}
    (hs : Flick.Step root l Λ μ (Flick.plug E₁ x) (.cfg Λ' μ' t')) :
    Waits root (DP S root) Λ Λ' μ μ' (Flick.plug E₂ x₂) t' := by
  obtain ⟨hv, -, -, -, Θ, t, ht⟩ := id hb
  obtain ⟨rfl, rfl, x₁, hx, rfl⟩ := dp_calm hv ht hE₁ hs
  refine ⟨rfl, rfl, ?_⟩
  obtain ⟨h₁, h₂⟩ := DP.step hroot hb hx
  by_cases hv₁ : Value x₁
  · exact ⟨_, Star.plug (h₂ hv₁).1 (FrR.okE DP.noVal hfr hE₁), FrR.plugs hfr (Cg.refl _ _)⟩
  · exact ⟨_, .refl, FrR.plugs hfr (.base (h₁ hv₁))⟩

/-- The redex of a step: inside a base pair, or the same frames around a related hole. -/
theorem around (hroot : RootTyM S .live root) {E : List Frame} {e t₂ : Flick.Tm} (hE : OkE E)
    (ht : Cg (DP S root) false (Flick.plug E e) t₂) {l : Lbl} {Λ Λ' : List Rgn}
    {μ μ' : List Flick.Tm} {t' : Flick.Tm}
    (hs : Flick.Step root l Λ μ (Flick.plug E e) (.cfg Λ' μ' t')) :
    Waits root (DP S root) Λ Λ' μ μ' t₂ t' ∨ ∃ E₂ e₂, t₂ = Flick.plug E₂ e₂ ∧
      All2 (FrR (DP S root)) E E₂ ∧ OkE E₂ ∧ Cg (DP S root) false e e₂ ∧
      (DP S root e e₂ → Waits root (DP S root) Λ Λ' μ μ' t₂ t') := by
  rcases ht.decomp with ⟨E₀, E₁, E₂, x₂, rfl, hb, rfl, hfr⟩ | ⟨E₂, e₂, rfl, hfr, he⟩
  · left
    rw [plug_append] at hs
    exact inside_sim hroot (fun F hF => hE F (List.mem_append_right _ hF)) hfr hb hs
  · exact .inr ⟨E₂, e₂, rfl, hfr, FrR.okE DP.noVal hfr hE, he, fun hb => inside_sim hroot hE hfr hb hs⟩

/-- **A step of one thread** is matched by the other side. -/
theorem thread_sim (hroot : RootTyM S .live root) {t₁ t₂ : Flick.Tm} (ht : Cg (DP S root) false t₁ t₂)
    {Λ Λ₂ : List Rgn} {μ μ₂ : List Flick.Tm} (hst : StR (DP S root) Λ Λ₂ μ μ₂) {l : Lbl}
    {Λ' : List Rgn} {μ' : List Flick.Tm} {t₁' : Flick.Tm}
    (hs : Flick.Step root l Λ μ t₁ (.cfg Λ' μ' t₁')) :
    Waits root (DP S root) Λ Λ' μ μ' t₂ t₁' ∨
    ∃ l₂ Λ₂' μ₂' t₂', Flick.Step root l₂ Λ₂ μ₂ t₂ (.cfg Λ₂' μ₂' t₂') ∧
      Cg (DP S root) false t₁' t₂' ∧ StR (DP S root) Λ' Λ₂' μ' μ₂' := by
  have hC : BClosed (DP S root) := DP.closed
  have hV : BNoVal (DP S root) := DP.noVal
  have hR : BNoRaise (DP S root) := DP.noRaise
  have hA : BNoAlts (DP S root) := DP.noAlts
  have nb : ∀ {a b : Flick.Tm}, DP S root a b → Value a → False := fun hb hv => hV _ _ hb hv
  cases hs with
  | pure hE hh =>
    rcases around hroot hE ht (.pure hE hh) with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    rcases phead_sim hC hV hA DP.plain hh he with hb | ⟨e₂', hh₂, hr⟩
    · exact .inl (hw hb)
    · exact .inr ⟨_, _, _, _, .pure hE₂ hh₂, FrR.plugs hfr hr, hst⟩
  | rt hE hh =>
    rcases around hroot hE ht (.rt hE hh) with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    rcases rhead_sim hV hR hA hC DP.plain hh he hst with hb | ⟨_, _, e₂', hh₂, hr, hst'⟩
    · exact .inl (hw hb)
    · exact .inr ⟨_, _, _, _, .rt hE₂ hh₂, FrR.plugs hfr hr, hst'⟩
  | glob hE hg =>
    rcases around hroot hE ht (.glob hE hg) with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    cases he with
    | glob => exact .inr ⟨_, _, _, _, .glob hE₂ hg, FrR.plugs hfr (Cg.refl _ _), hst⟩
    | base hb => exact .inl (hw hb)
  | performRoot hE hh hr hf hv =>
    rcases around hroot hE ht (.performRoot hE hh hr hf hv) with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    cases he with
    | base hb => exact .inl (hw hb)
    | app h₁ h₂ =>
      cases h₁ with
      | base hb => exact (nb hb .const).elim
      | const =>
        have hv' := Cg.value hV hv h₂
        exact .inr ⟨_, _, _, _, .performRoot hE₂ (FrR.handlesE hA DP.plain hfr hh) hr hf hv'.1,
          FrR.plugs hfr (.app (Cg.refl _ _) h₂), hst⟩
  | addFin hE hg hl hv =>
    rcases around hroot hE ht (.addFin hE hg hl hv) with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    cases he with
    | base hb => exact .inl (hw hb)
    | app h₁ h₂ =>
      cases h₁ with
      | base hb => exact (nb hb (.constApp rfl .own)).elim
      | app h₃ h₄ =>
        cases h₃ with
        | base hb => exact (nb hb .const).elim
        | const =>
          cases h₄ with
          | base hb => exact (nb hb .own).elim
          | own =>
            obtain ⟨g₂, hg₂, hl₂, r⟩ := live_rel hst.1 hg hl
            exact .inr ⟨_, _, _, _, .addFin hE₂ hg₂ hl₂ (Cg.value hV hv h₂).1,
              FrR.plugs hfr .lit, hst.1.set _ ⟨r.1, r.2.1,
                r.2.2.app (.cons (FrR.withs hfr _ (.await h₂)) .nil)⟩, hst.2⟩
  | spawn hE hg hl hv =>
    rcases around hroot hE ht (.spawn hE hg hl hv) with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    cases he with
    | base hb => exact .inl (hw hb)
    | app h₁ h₂ =>
      cases h₁ with
      | base hb => exact (nb hb (.constApp rfl .own)).elim
      | app h₃ h₄ =>
        cases h₃ with
        | base hb => exact (nb hb .const).elim
        | const =>
          cases h₄ with
          | base hb => exact (nb hb .own).elim
          | own =>
            obtain ⟨g₂, hg₂, hl₂, r⟩ := live_rel hst.1 hg hl
            exact .inr ⟨_, _, _, _, .spawn hE₂ hg₂ hl₂ (Cg.value hV hv h₂).1,
              FrR.plugs hfr .lit, hst.1.set _ ⟨r.1,
                r.2.1.app (.cons (FrR.withs hfr _ (.await h₂)) .nil), r.2.2⟩, hst.2⟩
  | @raise E v _ _ F hE hF hnt hni hnc hv =>
    have hE' : OkE (F :: E) := fun G hG => (List.mem_cons.mp hG).elim (fun h => h ▸ hF) (hE G)
    rcases around (E := F :: E) (e := .raise v) hroot hE' ht (.raise hE hF hnt hni hnc hv) with
      hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    cases hfr with
    | cons hF₂ hfr' =>
      cases he with
      | base hb => exact (hR.1 _ _ hb).elim
      | raise h' =>
        obtain ⟨hF₂ok, hE₂'⟩ := hE₂.cons
        exact .inr ⟨_, _, _, _, .raise hE₂' hF₂ok (fun h => hnt (hF₂.isTry.2 h))
          (fun h => hni (hF₂.isInside.2 h)) (fun h => hnc (hF₂.isCtl.2 h)) (Cg.value hV hv h').1,
          FrR.plugs hfr' (.raise h'), hst⟩
  | @unw E p ℓ _ _ a F hE hF hni hnc hnh =>
    rcases around (E := F :: E) (e := .unw p ℓ a) hroot (hE.cons' hF) ht (.unw hE hF hni hnc hnh) with
      hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    cases hfr with
    | cons hF₂ hfr' =>
      cases he with
      | base hb => exact (hR.2 _ _ _ _ hb).elim
      | unw h' =>
        obtain ⟨hF₂ok, hE₂'⟩ := hE₂.cons
        exact .inr ⟨_, _, _, _, .unw hE₂' hF₂ok (fun h => hni (hF₂.isInside.2 h))
          (fun h => hnc (hF₂.isCtl.2 h)) (fun h => hnh ((hF₂.catches p ℓ).2 h)), FrR.plugs hfr' (.unw h'),
          hst⟩
  | @performAbort E o b v _ _ p ℓ r Ei Ef al hE hEi hh ha hv =>
    have hs := Flick.Step.performAbort (root := root) (Λ := Λ) (μ := μ) (p := p) (ℓ := ℓ) (r := r) hE hEi hh ha hv
    rw [show Flick.plug E (.hwith p ℓ Ef al r (Flick.plug Ei (.app (.const (.perform Ef o .abort)) v))) =
      Flick.plug (Ei ++ .hwith p ℓ Ef al r :: E) (.app (.const (.perform Ef o .abort)) v) by
        rw [plug_append]; rfl] at ht hs
    rcases around hroot (hEi.append' (hE.cons' (F := .hwith p ℓ Ef al r) trivial)) ht hs with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    obtain ⟨Ei₂, Eo₂, rfl, hfi, hfo⟩ := All2.split hfr
    cases hfo with
    | cons hF₂ hfo' =>
      cases hF₂ with
      | hwith hal hr =>
        cases he with
        | base hb => exact .inl (hw hb)
        | app h₁ h₂ =>
          cases h₁ with
          | base hb => exact (nb hb .const).elim
          | const =>
            obtain ⟨b₂, hb₂, hbb⟩ := (Cg.armOf (o := o) hA DP.plain hal).some_left ha
            have hv' := Cg.value hV hv h₂
            refine .inr ⟨.rt, _, _, _, ?_, FrR.plugs hfo' (.hwith hal hr (FrR.plugs hfi
              (.unw (hbb.subst hC _ _ (SubR.inst (.cons hv'.2.2 .nil)))))), hst⟩
            rw [plug_append]
            exact .performAbort (fun G hG => hE₂ G (List.mem_append_right _ (List.mem_cons_of_mem _ hG)))
              (fun G hG => hE₂ G (List.mem_append_left _ hG)) (FrR.handlesE hA DP.plain hfi hh) hb₂ hv'.1
  | @performTail E o b v _ _ p ℓ r Ei Ef al hE hEi hh ha hv =>
    have hs := Flick.Step.performTail (root := root) (Λ := Λ) (μ := μ) (p := p) (ℓ := ℓ) (r := r) hE hEi hh ha hv
    rw [show Flick.plug E (.hwith p ℓ Ef al r (Flick.plug Ei (.app (.const (.perform Ef o .tail)) v))) =
      Flick.plug (Ei ++ .hwith p ℓ Ef al r :: E) (.app (.const (.perform Ef o .tail)) v) by
        rw [plug_append]; rfl] at ht hs
    rcases around hroot (hEi.append' (hE.cons' (F := .hwith p ℓ Ef al r) trivial)) ht hs with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    obtain ⟨Ei₂, Eo₂, rfl, hfi, hfo⟩ := All2.split hfr
    cases hfo with
    | cons hF₂ hfo' =>
      cases hF₂ with
      | hwith hal hr =>
        cases he with
        | base hb => exact .inl (hw hb)
        | app h₁ h₂ =>
          cases h₁ with
          | base hb => exact (nb hb .const).elim
          | const =>
            obtain ⟨b₂, hb₂, hbb⟩ := (Cg.armOf (o := o) hA DP.plain hal).some_left ha
            have hv' := Cg.value hV hv h₂
            refine .inr ⟨.rt, _, _, _, ?_, FrR.plugs hfo' (.hctl hal hr (hbb.subst hC _ _ (SubR.inst (.cons hv'.2.2 .nil))) (FrR.plugs hfi .hole)), hst⟩
            rw [plug_append]
            exact .performTail (fun G hG => hE₂ G (List.mem_append_right _ (List.mem_cons_of_mem _ hG)))
              (fun G hG => hE₂ G (List.mem_append_left _ hG)) (FrR.handlesE hA DP.plain hfi hh) hb₂ hv'.1
  | @performKept E o b v _ q _ ℓ Ei Ef al hE hEi hh ha hv hl =>
    have hs := Flick.Step.performKept (root := root) (Λ := Λ) (μ := μ) (q := q) (ℓ := ℓ) hE hEi hh ha hv hl
    rw [show Flick.plug E (.hkeep q ℓ Ef al (Flick.plug Ei (.app (.const (.perform Ef o .tail)) v))) =
      Flick.plug (Ei ++ .hkeep q ℓ Ef al :: E) (.app (.const (.perform Ef o .tail)) v) by
        rw [plug_append]; rfl] at ht hs
    rcases around hroot (hEi.append' (hE.cons' (F := .hkeep q ℓ Ef al) trivial)) ht hs with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    obtain ⟨Ei₂, Eo₂, rfl, hfi, hfo⟩ := All2.split hfr
    cases hfo with
    | cons hF₂ hfo' =>
      cases hF₂ with
      | hkeep hal =>
        cases he with
        | base hb => exact .inl (hw hb)
        | app h₁ h₂ =>
          cases h₁ with
          | base hb => exact (nb hb .const).elim
          | const =>
            obtain ⟨b₂, hb₂, hbb⟩ := (Cg.armOf (o := o) hA DP.plain hal).some_left ha
            have hv' := Cg.value hV hv h₂
            obtain ⟨g, hg, hgl⟩ := hl
            obtain ⟨g₂, hg₂, hl₂', -⟩ := live_rel hst.1 hg hgl
            refine .inr ⟨.rt, _, _, _, ?_, FrR.plugs hfo' (.hkarm hal (hbb.subst hC _ _ (SubR.inst (.cons hv'.2.2 .nil))) (FrR.plugs hfi .hole)), hst⟩
            rw [plug_append]
            exact .performKept (fun G hG => hE₂ G (List.mem_append_right _ (List.mem_cons_of_mem _ hG)))
              (fun G hG => hE₂ G (List.mem_append_left _ hG)) (FrR.handlesE hA DP.plain hfi hh) hb₂ hv'.1
              ⟨g₂, hg₂, hl₂'⟩
  | @performCtl E o b v _ _ p ℓ r Ei Ef al hE hEi hh ha hv =>
    have hs := Flick.Step.performCtl (root := root) (Λ := Λ) (μ := μ) (p := p) (ℓ := ℓ) (r := r) hE hEi hh ha hv
    rw [show Flick.plug E (.hwith p ℓ Ef al r (Flick.plug Ei (.app (.const (.perform Ef o .control)) v))) =
      Flick.plug (Ei ++ .hwith p ℓ Ef al r :: E) (.app (.const (.perform Ef o .control)) v) by
        rw [plug_append]; rfl] at ht hs
    rcases around hroot (hEi.append' (hE.cons' (F := .hwith p ℓ Ef al r) trivial)) ht hs with hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    obtain ⟨Ei₂, Eo₂, rfl, hfi, hfo⟩ := All2.split hfr
    cases hfo with
    | cons hF₂ hfo' =>
      cases hF₂ with
      | hwith hal hr =>
        cases he with
        | base hb => exact .inl (hw hb)
        | app h₁ h₂ =>
          cases h₁ with
          | base hb => exact (nb hb .const).elim
          | const =>
            obtain ⟨b₂, hb₂, hbb⟩ := (Cg.armOf (o := o) hA DP.plain hal).some_left ha
            have hv' := Cg.value hV hv h₂
            have hl := hst.2.length
            refine .inr ⟨.rt, _, _, _, ?_, FrR.plugs hfo' (by
                rw [hl]
                exact .hctl hal hr (hbb.subst hC _ _ (SubR.inst (.cons hv'.2.2 (.cons (fun _ => .kont) .nil))))
                  (FrR.plugs hfi .hole)), hst.1, hst.2.app (.cons (.lam .todo) .nil)⟩
            rw [plug_append]
            exact .performCtl (fun G hG => hE₂ G (List.mem_append_right _ (List.mem_cons_of_mem _ hG)))
              (fun G hG => hE₂ G (List.mem_append_left _ hG)) (FrR.handlesE hA DP.plain hfi hh) hb₂ hv'.1
  | @resume E A k p u _ _ ℓ r Ef al Ei ℓh hE hA' hEi hk hn hu =>
    have hs := Flick.Step.resume (root := root) (Λ := Λ) (μ := μ) (p := p) (r := r) (k := k) (Ef := Ef)
      (al := al) (ℓ := ℓ) (ℓh := ℓh) hE hA' hEi hk hn hu
    rw [show Flick.plug E (.hctl p ℓh ℓ false Ef al r (Flick.plug A (.app (.kont p ℓ) u)) k) =
      Flick.plug (A ++ .hctlA p ℓh ℓ false Ef al r k :: E) (.app (.kont p ℓ) u) by
        rw [plug_append]; rfl] at ht hs
    subst hk
    rcases around hroot (hA'.append' (hE.cons' (F := .hctlA p ℓh ℓ false Ef al r _) trivial)) ht hs with
      hw | ⟨E₂, e₂, rfl, hfr, hE₂, he, hw⟩
    · exact .inl hw
    obtain ⟨A₂, Eo₂, rfl, hfa, hfo⟩ := All2.split hfr
    cases hfo with
    | cons hF₂ hfo' =>
      cases hF₂ with
      | hctlA hal hr hk =>
        obtain ⟨Ei₂, rfl, hfe⟩ := Cg.plug_hole DP.plain hk
        cases he with
        | base hb => exact .inl (hw hb)
        | app h₁ h₂ =>
          cases h₁ with
          | base hb => exact (nb hb .kont).elim
          | kont =>
            have hu' := Cg.value hV hu h₂
            refine .inr ⟨.rt, _, _, _, ?_, FrR.plugs hfo' (.hres (FrR.plugs hfa .hole)
              (.hwith hal hr (FrR.plugs hfe (hu'.2.2 false)))), hst⟩
            rw [plug_append]
            exact .resume (fun G hG => hE₂ G (List.mem_append_right _ (List.mem_cons_of_mem _ hG)))
              (fun G hG => hE₂ G (List.mem_append_left _ hG)) (FrR.okE hV hfe hEi) rfl
              (FrR.resumesE hfa hn) hu'.1

end Flick.Elab
