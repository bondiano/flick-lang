import Formal.SettleRHead

/-!
# Thread and configuration steps preserve the `settle`-frame invariant
-/

namespace Flick

theorem Step.settle {root : Roots} {lbl : Lbl} {Λ : List Rgn} {μ : List Tm} {t : Tm}
    {Λ' : List Rgn} {μ' : List Tm} {t' : Tm} (hroot : root.NoSettle)
    (hs : Step root lbl Λ μ t (.cfg Λ' μ' t')) (ht : t.SettleLin)
    (hrgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.SettleLin)
    (hmem : ∀ (i : Nat) (v : Tm), μ[i]? = some v → v.NoSettle) :
    t'.SettleLin ∧
      (∀ (p : Nat) (g : Rgn), Λ'[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.SettleLin) ∧
      (∀ (i : Nat) (v : Tm), μ'[i]? = some v → v.NoSettle) ∧
      (∀ l, rgsSettleCount l Λ' = rgsSettleCount l Λ) ∧
      SettleDelta μ μ' t t' := by
  have sameMem : ∀ {l : Nat}, Running μ l →
      (∀ (i : Nat) (d : Tm), μ[i]? = some d → d.IsCell → ∃ d' : Tm, μ[i]? = some d' ∧ d'.IsCell) →
      Running μ l :=
    fun h _ => h
  cases hs with
  | pure hE hh =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug hh.not_value hE ht
    obtain ⟨hr', hcnt⟩ := hh.settle hr
    exact ⟨hSE.fill hr', hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp only [plug_settleCount, hcnt l], fun _ => sameMem⟩⟩
  | rt hE hh =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug hh.not_value hE ht
    obtain ⟨hr', hrgs', hmem', hrc, hdel⟩ := hh.settle hr hrgs hmem
    refine ⟨hSE.fill hr', hrgs', hmem', hrc, ?_⟩
    rcases hdel with ⟨hcnt, hrun⟩ | ⟨l₀, hnr, hr', hcnt, hoth⟩ | ⟨l₀, hge, hcnt, hoth⟩
    · exact .inl ⟨fun l => by simp only [plug_settleCount, hcnt l], hrun⟩
    · exact .inr (.inl ⟨l₀, hnr, hr', fun l => by
        simp only [plug_settleCount, hcnt l]; omega, hoth⟩)
    · exact .inr (.inr ⟨l₀, by simp only [plug_settleCount]; omega, fun l => by
        have := hcnt l; simp only [plug_settleCount]; omega, hoth⟩)
  | addFin hE hg _ hv =>
    rename_i E₀ p₀ g₀ t₀ _
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fin_not_value (.inl rfl)) hE ht
    have htn := hv.noSettle hr.2.1
    have hwn : (withs p₀ E₀ (.await t₀)).NoSettle := withs_noSettle hSE htn
    refine ⟨hSE.fill trivial, fun q g' hq k hk => ?_, hmem, fun l => ?_,
      .inl ⟨fun l => by simp [plug_settleCount, Tm.settleCount, htn l], fun _ => sameMem⟩⟩
    · rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
      · simp only [List.mem_append] at hk
        rcases hk with hk | hk | hk
        · exact hrgs _ _ hg k (List.mem_append_left _ hk)
        · exact hrgs _ _ hg k (List.mem_append_right _ hk)
        · simp at hk; subst hk; exact Tm.SettleLin.of_noSettle hwn
      · exact hrgs q g' hq k hk
    · have h1 := rgsSettleCount_setRgn l hg (g' := ⟨g₀.live, g₀.kids, g₀.fins ++ [withs p₀ E₀ (.await t₀)]⟩)
      simp only [Rgn.settleCount, List.map_append, List.sum_append, List.map_singleton,
        List.sum_cons, List.sum_nil, hwn l] at h1
      omega
  | spawn hE hg _ hv =>
    rename_i E₀ p₀ g₀ t₀ _
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fin_not_value (.inr rfl)) hE ht
    have htn := hv.noSettle hr.2.1
    have hwn : (withs p₀ E₀ (.await t₀)).NoSettle := withs_noSettle hSE htn
    refine ⟨hSE.fill trivial, fun q g' hq k hk => ?_, hmem, fun l => ?_,
      .inl ⟨fun l => by simp [plug_settleCount, Tm.settleCount, htn l], fun _ => sameMem⟩⟩
    · rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
      · simp only [List.mem_append] at hk
        rcases hk with (hk | hk) | hk
        · exact hrgs _ _ hg k (List.mem_append_left _ hk)
        · simp at hk; subst hk; exact Tm.SettleLin.of_noSettle hwn
        · exact hrgs _ _ hg k (List.mem_append_right _ hk)
      · exact hrgs q g' hq k hk
    · have h1 := rgsSettleCount_setRgn l hg (g' := ⟨g₀.live, g₀.kids ++ [withs p₀ E₀ (.await t₀)], g₀.fins⟩)
      simp only [Rgn.settleCount, List.map_append, List.sum_append, List.map_singleton,
        List.sum_cons, List.sum_nil, hwn l] at h1
      omega
  | performRoot hE _ hsvc hf hv =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug perform_not_value hE ht
    have hfn := (hroot.1 _ _ hsvc).field hf
    have hvn := hv.noSettle hr.2.1
    exact ⟨hSE.fill ⟨Tm.SettleLin.of_noSettle hfn, Tm.SettleLin.of_noSettle hvn, .inl hvn⟩,
      hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [plug_settleCount, Tm.settleCount, hfn l, hvn l], fun _ => sameMem⟩⟩
  | raise hE hF _ _ hnc hv =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (not_value_plug raise_not_value _) hE ht
    obtain ⟨hSF, hr'⟩ := Frame.settleLin_of_plug hF raise_not_value hr
    have hvn := hv.noSettle hr'
    exact ⟨hSE.fill (Tm.SettleLin.of_noSettle hvn), hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Frame.settleCount_plug, Frame.not_isCtl_settleCount hSF hnc l,
          Tm.settleCount, hvn l], fun _ => sameMem⟩⟩
  | unw hE hF _ hnc =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (not_value_plug unw_not_value _) hE ht
    obtain ⟨hSF, hr'⟩ := Frame.settleLin_of_plug hF unw_not_value hr
    exact ⟨hSE.fill hr', hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Frame.settleCount_plug, Frame.not_isCtl_settleCount hSF hnc l]
        omega, fun _ => sameMem⟩⟩
  | performAbort hE hEi _ ha hv =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fun h => by cases h) hE ht
    obtain ⟨hSEi, hri⟩ := settleLin_of_plug perform_not_value hEi hr.2.2
    have hvn := hv.noSettle hri.2.1
    have hsub := (hr.1.armOf ha).subst_inst (δ := [_]) (by simpa using hvn)
    exact ⟨hSE.fill ⟨hr.1, hr.2.1, hSEi.fill hsub⟩, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Tm.settleCount, hr.1 l, hr.2.1 l, hvn l, hsub l],
        fun _ => sameMem⟩⟩
  | performTail hE hEi _ ha hv =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fun h => by cases h) hE ht
    obtain ⟨hSEi, hri⟩ := settleLin_of_plug perform_not_value hEi hr.2.2
    have hvn := hv.noSettle hri.2.1
    have hsub := (hr.1.armOf ha).subst_inst (δ := [_]) (by simpa using hvn)
    exact ⟨hSE.fill ⟨hr.1, hr.2.1, Tm.SettleLin.of_noSettle hsub, hSEi.fill trivial⟩,
      hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Tm.settleCount, hr.1 l, hr.2.1 l, hvn l, hsub l],
        fun _ => sameMem⟩⟩
  | performKept hE hEi _ ha hv _ =>
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fun h => by cases h) hE ht
    obtain ⟨hSEi, hri⟩ := settleLin_of_plug perform_not_value hEi hr.2
    have hvn := hv.noSettle hri.2.1
    have hsub := (hr.1.armOf ha).subst_inst (δ := [_]) (by simpa using hvn)
    exact ⟨hSE.fill ⟨hr.1, Tm.SettleLin.of_noSettle hsub, hSEi.fill trivial⟩,
      hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Tm.settleCount, hr.1 l, hvn l, hsub l],
        fun _ => sameMem⟩⟩
  | performCtl hE hEi _ ha hv =>
    rename_i v₀ p₀ _ _ _ _ _ _
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fun h => by cases h) hE ht
    obtain ⟨hSEi, hri⟩ := settleLin_of_plug perform_not_value hEi hr.2.2
    have hvn := hv.noSettle hri.2.1
    have hsub := (hr.1.armOf ha).subst_inst (δ := [v₀, .kont p₀ μ.length]) (by
      intro w hw; simp only [List.mem_cons, List.not_mem_nil, or_false] at hw
      rcases hw with rfl | rfl
      · exact hvn
      · intro _; rfl)
    refine ⟨hSE.fill ⟨hr.1, hr.2.1, Tm.SettleLin.of_noSettle hsub, hSEi.fill trivial⟩,
      hrgs, fun i w hw => ?_, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Tm.settleCount, hr.1 l, hr.2.1 l, hvn l, hsub l],
        fun l ⟨x, hx⟩ _ => ⟨x, by
          rw [List.getElem?_append_left (List.getElem?_eq_some_iff.mp hx).1]; exact hx⟩⟩⟩
    by_cases hi : i < μ.length
    · rw [List.getElem?_append_left hi] at hw; exact hmem i w hw
    · rw [List.getElem?_append_right (by omega)] at hw
      cases hj : i - μ.length with
      | zero => simp [hj] at hw; subst hw; intro _; rfl
      | succ => simp [hj] at hw
  | resume hE hA hEi hk _ hu =>
    subst hk
    obtain ⟨hSE, hr⟩ := settleLin_of_plug (fun h => by cases h) hE ht
    obtain ⟨hSA, hri⟩ := settleLin_of_plug kont_not_value hA hr.2.2.1
    obtain ⟨hSEi, -⟩ := settleLin_of_plug hole_not_value hEi hr.2.2.2
    have hun := hu.noSettle hri.2.1
    exact ⟨hSE.fill ⟨hSA.fill trivial, ⟨hr.1, hr.2.1, hSEi.fill (Tm.SettleLin.of_noSettle hun)⟩⟩,
      hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by
        simp only [plug_settleCount, Tm.settleCount, hr.1 l, hr.2.1 l, hun l]; omega,
        fun _ => sameMem⟩⟩
  | glob hE hg =>
    obtain ⟨hSE, -⟩ := settleLin_of_plug (fun h => by cases h) hE ht
    have hvn := hroot.2 _ _ hg
    exact ⟨hSE.fill (Tm.SettleLin.of_noSettle hvn), hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [plug_settleCount, Tm.settleCount, hvn l], fun _ => sameMem⟩⟩

/-- Every step of a thread leaves every existing child in its slot. -/
theorem Step.kid_same {root : Roots} {lbl : Lbl} {Λ : List Rgn} {μ : List Tm} {t : Tm}
    {Λ' : List Rgn} {μ' : List Tm} {t' : Tm} {p j : Nat} {g : Rgn} {k : Tm}
    (hs : Step root lbl Λ μ t (.cfg Λ' μ' t')) (hg : Λ[p]? = some g) (hk : g.kids[j]? = some k) :
    ∃ g' : Rgn, Λ'[p]? = some g' ∧ g'.kids[j]? = some k := by
  have hp_lt := (List.getElem?_eq_some_iff.mp hg).1
  have hj_lt := (List.getElem?_eq_some_iff.mp hk).1
  cases hs with
  | rt _ hh =>
    cases hh with
    | scope | hdl =>
      exact ⟨g, by rw [List.getElem?_append_left hp_lt]; exact hg, hk⟩
    | open_ hg₀ =>
      rename_i p₀ _ _
      by_cases hp : p = p₀
      · subst hp; rw [hg] at hg₀; cases hg₀
        exact ⟨_, List.getElem?_set_self hp_lt, hk⟩
      · exact ⟨g, by rw [setRgn, List.getElem?_set_ne (Ne.symm hp)]; exact hg, hk⟩
    | finRun hg₀ =>
      rename_i p₀ _ _ _ _ _ _ _ _
      by_cases hp : p = p₀
      · subst hp; rw [hg] at hg₀; cases hg₀
        exact ⟨_, List.getElem?_set_self hp_lt, by
          rw [List.getElem?_append_left hj_lt]; exact hk⟩
      · exact ⟨g, by rw [setRgn, List.getElem?_set_ne (Ne.symm hp)]; exact hg, hk⟩
    | close hg₀ =>
      rename_i p₀ _ _ _ _ _
      by_cases hp : p = p₀
      · subst hp; rw [hg] at hg₀; cases hg₀
        exact ⟨_, List.getElem?_set_self hp_lt, hk⟩
      · exact ⟨g, by rw [setRgn, List.getElem?_set_ne (Ne.symm hp)]; exact hg, hk⟩
    | _ => exact ⟨g, hg, hk⟩
  | addFin _ hg₀ =>
    rename_i p₀ _ _ _ _ _
    by_cases hp : p = p₀
    · subst hp; rw [hg] at hg₀; cases hg₀
      exact ⟨_, List.getElem?_set_self hp_lt, hk⟩
    · exact ⟨g, by rw [setRgn, List.getElem?_set_ne (Ne.symm hp)]; exact hg, hk⟩
  | spawn _ hg₀ =>
    rename_i p₀ _ _ _ _ _
    by_cases hp : p = p₀
    · subst hp; rw [hg] at hg₀; cases hg₀
      exact ⟨_, List.getElem?_set_self hp_lt, by
        rw [List.getElem?_append_left hj_lt]; exact hk⟩
    · exact ⟨g, by rw [setRgn, List.getElem?_set_ne (Ne.symm hp)]; exact hg, hk⟩
  | _ => exact ⟨g, hg, hk⟩

private theorem count_preserve {c c' : Cfg} {t t' : Tm}
    (hc : ∀ l, c.settleCount l ≤ 1 ∧ (c.settleCount l = 1 → Running c.mem l))
    (hle : ∀ l, t.settleCount l ≤ c.settleCount l)
    (hbal : ∀ l, c'.settleCount l + t.settleCount l = c.settleCount l + t'.settleCount l)
    (hdel : SettleDelta c.mem c'.mem t t')
    (hcell : ∀ (i : Nat) (d : Tm), c.mem[i]? = some d → d.IsCell →
      ∃ d' : Tm, c'.mem[i]? = some d' ∧ d'.IsCell) :
    ∀ l, c'.settleCount l ≤ 1 ∧ (c'.settleCount l = 1 → Running c'.mem l) := by
  intro l
  have hb := hbal l
  have ⟨hcl, hrun⟩ := hc l
  rcases hdel with ⟨hcnt, hkeep⟩ | ⟨l₀, hnr, hr', hcnt, hoth⟩ | ⟨l₀, hge, hcnt, hoth⟩
  · have hc' : c'.settleCount l = c.settleCount l := by have := hcnt l; omega
    rw [hc']
    exact ⟨hcl, fun h1 => hkeep l (hrun h1) hcell⟩
  · have hcl' := hcnt l
    by_cases hl : l₀ = l
    · subst hl
      have hz : c.settleCount l₀ = 0 := by
        cases h0 : c.settleCount l₀ with
        | zero => rfl
        | succ m =>
          have hm : m = 0 := by omega
          subst hm
          exact (hnr (hrun h0)).elim
      simp only [if_true] at hcl'
      exact ⟨by omega, fun _ => hr'⟩
    · simp only [hl, if_false, Nat.add_zero] at hcl'
      have hc' : c'.settleCount l = c.settleCount l := by omega
      rw [hc']
      exact ⟨hcl, fun h1 => hoth l (Ne.symm hl) (hrun h1)⟩
  · have hcl' := hcnt l
    by_cases hl : l₀ = l
    · subst hl
      simp only [if_true] at hcl'
      have h1 : c.settleCount l₀ = 1 := by have := hle l₀; omega
      have hz : c'.settleCount l₀ = 0 := by omega
      rw [hz]
      exact ⟨by omega, fun h => by omega⟩
    · simp only [hl, if_false, Nat.add_zero] at hcl'
      have hc' : c'.settleCount l = c.settleCount l := by omega
      rw [hc']
      exact ⟨hcl, fun h1 => hoth l (Ne.symm hl) (hrun h1)⟩

/-- A configuration step preserves `CfgSettle`, given that lazy cells stay lazy cells. -/
theorem CStep.settle {root : Roots} {lbl : Lbl} {c c' : Cfg} (hroot : root.NoSettle)
    (hc : CfgSettle c) (hs : CStep root lbl c c')
    (hcell : ∀ (i : Nat) (d : Tm), c.mem[i]? = some d → d.IsCell →
      ∃ d' : Tm, c'.mem[i]? = some d' ∧ d'.IsCell) :
    CfgSettle c' := by
  cases hs with
  | main hs =>
    obtain ⟨hm', hrgs', hmem', hrc, hdel⟩ := hs.settle hroot hc.main hc.rgs hc.mem
    refine ⟨hm', hrgs', hmem', count_preserve hc.count
      (fun l => by simp [Cfg.settleCount])
      (fun l => by simp only [Cfg.settleCount, hrc l]; omega) hdel hcell⟩
  | kid hg _ hk hs =>
    rename_i k' _ _
    have hkt := hc.rgs _ _ hg _ (List.mem_append_left _ (List.mem_of_getElem? hk))
    obtain ⟨hk', hrgs', hmem', hrc, hdel⟩ := hs.settle hroot hkt hc.rgs hc.mem
    obtain ⟨g', hg', hk₀⟩ := hs.kid_same hg hk
    refine ⟨hc.main, fun q g'' hq x hx => ?_, hmem', count_preserve hc.count
      (fun l => kid_settleCount_le l hg hk)
      (fun l => by
        have h1 := rgsSettleCount_setKid l (k' := k') hg' hk₀
        have h2 := hrc l
        simp only [Cfg.settleCount]
        omega) hdel hcell⟩
    unfold setKid at hq
    rw [hg'] at hq; dsimp only at hq
    rcases setRgn_get hg' hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
    · simp only [List.mem_append] at hx
      rcases hx with hx | hx
      · obtain ⟨i, hi⟩ := List.mem_iff_getElem?.mp hx
        rcases mem_set_get hi with hi | ⟨-, rfl⟩
        · exact hrgs' _ _ hg' x (List.mem_append_left _ (List.mem_of_getElem? hi))
        · exact hk'
      · exact hrgs' _ _ hg' x (List.mem_append_right _ hx)
    · exact hrgs' q g'' hq x hx

/-- Any reachable configuration preserves `CfgSettle`, given that lazy cells stay lazy cells along
every reachable step. -/
theorem CSteps.settle {root : Roots} {c c' : Cfg} (hroot : root.NoSettle) (hc : CfgSettle c)
    (hs : CSteps root c c')
    (hcell : ∀ c₁, CSteps root c c₁ → ∀ lbl c₂, CStep root lbl c₁ c₂ →
      ∀ (i : Nat) (d : Tm), c₁.mem[i]? = some d → d.IsCell →
        ∃ d' : Tm, c₂.mem[i]? = some d' ∧ d'.IsCell) :
    CfgSettle c' := by
  induction hs with
  | refl => exact hc
  | step s _ ih =>
    exact ih (s.settle hroot hc (hcell _ .refl _ _ s))
      fun c₁ h₁ => hcell c₁ (.step s h₁)

end Flick
