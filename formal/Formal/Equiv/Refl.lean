import Formal.Equiv.ReflFrame

/-!
# Equivalence of every run: a step of the right side, on the left

**`reflect_focus`**: at related foci in related contexts, a step of the right side is one the
left side may take too, or one of its children steps alone, or the step leaves the frame of a
right cell and the right side catches up alone. The left side steps by the same rule with the
side conditions read through the relation: related values, cells, regions, handlers and roots; a
`resume` it cannot take is a defect (`Step.resumeDie`), so a left thread at a `resume` steps in
any case.
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel perform_redex fin_redex kont_redex raise_redex unw_redex)

variable {W : World}

theorem hwith_ne_settle {p ℓ : Nat} {E : Name} {al r : Tm} :
    ∀ n, Frame.hwith p ℓ E al r ≠ .settle n := fun _ h => by cases h

/-- A `resume` steps: it resumes the body when the arm holds it, and is a defect otherwise. -/
theorem kont_steps {S : Sig} {root : Roots} {Λ : List Rgn} {μ : List Tm} {E : List Frame}
    {p ℓ : Nat} {u : Tm} (hE : OkE E) (hu : Value u)
    (hty : TypedT S (plug E (.app (.kont p ℓ) u))) :
    ∃ l o, Step root l Λ μ (plug E (.app (.kont p ℓ) u)) o := by
  by_cases hres : Resumable E p ℓ
  · obtain ⟨A, E', ℓh, Ef, al, rf, c, rfl, hA⟩ := hres
    have hEA : OkE A := fun F hF => hE F (List.mem_append_left _ hF)
    have hEB : OkE E' := fun F hF => hE F (List.mem_append_right _ (List.mem_cons_of_mem _ hF))
    rw [plug_append] at hty ⊢
    obtain ⟨_, _, _, _, ht⟩ := hty.inner hEB (fun h => by cases h) rfl
    obtain ⟨Ei, hEi, rfl⟩ := ht.hctl_parked rfl
    exact ⟨_, _, .resume hEB hEA hEi rfl hA hu⟩
  · exact ⟨_, _, .resumeDie hE hu hres⟩

/-- **A step of the right side at related foci** is answered on the left. -/
theorem reflect_focus {S : Sig} {root : Roots} {l : Lbl} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm}
    {E E₂ : List Frame} {r r₂ : Tm} {o₂ : Out} (hE : OkE E) (hE₂ : OkE E₂) (hr : Redex r)
    (hr₂ : Redex r₂) (hs : Sim W .top r r₂) (hfr : FrL W E E₂) (hst : StR W μ μ₂)
    (hrg : RgsR W μ none Λ Λ₂) (hB : ¬ W.B) (hty : TypedT S (plug E r))
    (hs₂ : Step root l Λ₂ μ₂ (plug E₂ r₂) o₂) :
    Moves root Λ μ (plug E r) ∨ Catch W μ₂ (plug E r) (plug E₂ r₂) := by
  have mv : ∀ {l' o}, Step root l' Λ μ (plug E r) o →
      Moves root Λ μ (plug E r) ∨ Catch W μ₂ (plug E r) (plug E₂ r₂) :=
    fun h => .inl (.inl ⟨_, _, h⟩)
  have hrt : TypedT S r := hty.inner hE hr.1 rfl
  generalize ht₂ : plug E₂ r₂ = t₂ at hs₂
  cases hs₂ with
  | pure hE' hh =>
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ hh.redex hE₂ hE' ht₂
    obtain ⟨r', h'⟩ := phead_rev hs hr hrt hh
    exact mv (.pure hE h')
  | rt hE' hh =>
    rcases hh.redex' with h | ⟨F, v, hF, hv, rfl⟩ | ⟨F, q, ℓ, c, hF, rfl⟩
    · obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ h hE₂ hE' ht₂
      exact .inl ((rhead_rev hs hr hr₂ hst hrg hB hh).moves hE)
    · obtain ⟨rfl, rfl⟩ := decomp_eq (E' := F :: _) hr₂ (raise_redex hv) hE₂ (hE'.cons' hF) ht₂
      exact raise_frame_rev hE hE' hr hs hv hfr hst hrg hB hty hh
    · obtain ⟨rfl, rfl⟩ := decomp_eq (E' := F :: _) hr₂ (unw_redex _ _ _) hE₂ (hE'.cons' hF) ht₂
      exact unw_frame_rev hE hE' hs hfr hst hrg hB hty hh
  | addFin hE' hg hl hv | spawn hE' hg hl hv =>
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (fin_redex (by first | exact .inl rfl | exact .inr rfl) hv)
      hE₂ hE' ht₂
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    obtain ⟨f₁, y, rfl, hf₁, hy⟩ := hf.rinv_app
    obtain rfl := hf₁.top_const_rev rfl
    obtain rfl := hy.rinv_own
    obtain ⟨g, hg', hR⟩ := hrg.get' hg
    first
      | exact mv (.addFin hE hg' (hR.live ▸ hl) (Redex.app hr rfl).2)
      | exact mv (.spawn hE hg' (hR.live ▸ hl) (Redex.app hr rfl).2)
  | performRoot hE' hh hrt' hf hv =>
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (perform_redex hv) hE₂ hE' ht₂
    obtain ⟨f, a, rfl, hf', -⟩ := hs.rinv_app
    obtain rfl := hf'.top_const_rev rfl
    exact mv (.performRoot hE (hfr.handlesE' hh) hrt' hf (Redex.app hr rfl).2)
  | raise hE' hF hnt hni hnc hv =>
    obtain ⟨rfl, rfl⟩ := decomp_eq (E' := _ :: _) hr₂ (raise_redex hv) hE₂ (hE'.cons' hF) ht₂
    obtain ⟨e, rfl, -⟩ := hs.rinv_raise
    have hev := hr.2 .raise e trivial rfl
    cases hfr with
    | @junkL n E₀ _ _ _ => exact mv (.rt (E := E₀) (hE.cons).2 (.settleRaise hev))
    | junkR => exact (hnc trivial).elim
    | againF | againR => exact (hnc trivial).elim
    | @cons F' _ E' _ hFS _ =>
      exact mv (.raise (hE.cons).2 (hE.cons).1 (fun h => hnt (hFS.isTry.1 h))
        (fun h => hni (hFS.isInside.1 h)) (fun h => hnc (hFS.isCtl.1 h)) hev)
  | unw hE' hF hni hnc hnh =>
    obtain ⟨rfl, rfl⟩ := decomp_eq (E' := _ :: _) hr₂ (unw_redex _ _ _) hE₂ (hE'.cons' hF) ht₂
    obtain ⟨ℓ, k, c, rfl, hl, -⟩ := hs.rinv_unw
    cases hfr with
    | @junkL n E₀ _ _ _ => exact mv (.rt (E := E₀) (hE.cons).2 .settleUnw)
    | junkR => exact (hnc trivial).elim
    | againF | againR => exact (hnc trivial).elim
    | @cons F' _ E' _ hFS _ =>
      exact mv (.unw (hE.cons).2 (hE.cons).1 (fun h => hni (hFS.isInside.1 h))
        (fun h => hnc (hFS.isCtl.1 h)) (fun h => hnh ((hFS.catches hst.bij hl).1 h)))
  | performAbort hE' hEi hh ha hv | performTail hE' hEi hh ha hv | performCtl hE' hEi hh ha hv =>
    have ht₂' := ht₂.trans (plug_append _ (.hwith _ _ _ _ _ :: _) _).symm
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (perform_redex hv) hE₂ (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) ht₂'
    obtain ⟨A, G, B, rfl, hA, hG, -⟩ := hfr.split_right hwith_ne_settle (fun _ h => by cases h)
    obtain ⟨f, a, rfl, hf', -⟩ := hs.rinv_app
    obtain rfl := hf'.top_const_rev rfl
    have hEA : OkE A := fun F hF => hE F (List.mem_append_left _ hF)
    have hEB : OkE B := fun F hF => hE F (List.mem_append_right _ (List.mem_cons_of_mem _ hF))
    cases hG with
    | hwith _ hal =>
      obtain ⟨b', hb'⟩ := ORel.some_right (armOf_eq hal rfl) ha
      first
        | exact .inl (.inl ⟨_, _, by rw [plug_append]; exact .performAbort hEB hEA (hA.handlesE' hh) hb' (Redex.app hr rfl).2⟩)
        | exact .inl (.inl ⟨_, _, by rw [plug_append]; exact .performTail hEB hEA (hA.handlesE' hh) hb' (Redex.app hr rfl).2⟩)
        | exact .inl (.inl ⟨_, _, by rw [plug_append]; exact .performCtl hEB hEA (hA.handlesE' hh) hb' (Redex.app hr rfl).2⟩)
  | performKept hE' hEi hh ha hv hq =>
    have ht₂' := ht₂.trans (plug_append _ (.hkeep _ _ _ _ :: _) _).symm
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (perform_redex hv) hE₂ (hEi.append' (hE'.cons' (F := .hkeep _ _ _ _) trivial)) ht₂'
    obtain ⟨A, G, B, rfl, hA, hG, -⟩ := hfr.split_right (fun _ h => by cases h) (fun _ h => by cases h)
    obtain ⟨f, a, rfl, hf', -⟩ := hs.rinv_app
    obtain rfl := hf'.top_const_rev rfl
    have hEA : OkE A := fun F hF => hE F (List.mem_append_left _ hF)
    have hEB : OkE B := fun F hF => hE F (List.mem_append_right _ (List.mem_cons_of_mem _ hF))
    cases hG with
    | hkeep _ hal =>
      obtain ⟨b', hb'⟩ := ORel.some_right (armOf_eq hal rfl) ha
      obtain ⟨g₂, hg₂, hl₂⟩ := hq
      obtain ⟨g, hg, hR⟩ := hrg.get' hg₂
      exact .inl (.inl ⟨_, _, by
        rw [plug_append]
        exact .performKept hEB hEA (hA.handlesE' hh) hb' (Redex.app hr rfl).2 ⟨g, hg, hR.live ▸ hl₂⟩⟩)
  | resume hE' hA' hEi hk hn hu | resumeDie hE' hu hnr =>
    first
      | (have ht₂' := ht₂.trans (plug_append _ (.hctlA _ _ _ _ _ _ _ _ :: _) _).symm
         obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (kont_redex hu) hE₂ (hA'.append' (hE'.cons' (F := .hctlA _ _ _ _ _ _ _ _) trivial)) ht₂')
      | obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (kont_redex hu) hE₂ hE' ht₂
    all_goals
      obtain ⟨f, a, rfl, hf', -⟩ := hs.rinv_app
      obtain ⟨ℓ₀, k₀, rfl, -⟩ := hf'.rinv_kont
      exact .inl (.inl (kont_steps hE (Redex.app hr rfl).2 hty))
  | todo hE' =>
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ todo_redex hE₂ hE' ht₂
    obtain rfl := hs.rinv_todo
    exact mv (.todo hE)
  | glob hE' hg =>
    obtain ⟨rfl, rfl⟩ := decomp_eq hr₂ (glob_redex _) hE₂ hE' ht₂
    obtain rfl := hs.rinv_glob
    exact mv (.glob hE hg)

end Flick.Equiv
