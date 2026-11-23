import Formal.SettlePHead

/-!
# Runtime head steps preserve the `settle`-frame invariant
-/

namespace Flick

theorem rgsSettleCount_append_new (l : Nat) (Λ : List Rgn) :
    rgsSettleCount l (Λ ++ [Rgn.new]) = rgsSettleCount l Λ := by
  simp [rgsSettleCount, Rgn.settleCount, Rgn.new]

theorem RHead.settle {Λ : List Rgn} {μ : List Tm} {r : Tm} {Λ' : List Rgn} {μ' : List Tm} {r' : Tm}
    (hh : RHead Λ μ r Λ' μ' r') (hr : r.SettleLin)
    (hrgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.SettleLin)
    (hmem : ∀ (i : Nat) (v : Tm), μ[i]? = some v → v.NoSettle) :
    r'.SettleLin ∧
      (∀ (p : Nat) (g : Rgn), Λ'[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.SettleLin) ∧
      (∀ (i : Nat) (v : Tm), μ'[i]? = some v → v.NoSettle) ∧
      (∀ l, rgsSettleCount l Λ' = rgsSettleCount l Λ) ∧
      SettleDelta μ μ' r r' := by
  have sameMem : ∀ {l : Nat}, Running μ l →
      (∀ (i : Nat) (d : Tm), μ[i]? = some d → d.IsCell → ∃ d' : Tm, μ[i]? = some d' ∧ d'.IsCell) →
      Running μ l :=
    fun h _ => h
  have appendMem : ∀ {c : Tm}, c.NoSettle →
      (∀ (i : Nat) (v : Tm), (μ ++ [c])[i]? = some v → v.NoSettle) ∧
      (∀ l, Running μ l → Running (μ ++ [c]) l) := fun hc =>
    ⟨fun i v hv => by
      by_cases hi : i < μ.length
      · rw [List.getElem?_append_left hi] at hv; exact hmem i v hv
      · rw [List.getElem?_append_right (by omega)] at hv
        cases hj : i - μ.length with
        | zero => simp [hj] at hv; subst hv; exact hc
        | succ => simp [hj] at hv,
     fun l ⟨x, hx⟩ => ⟨x, by
      rw [List.getElem?_append_left (List.getElem?_eq_some_iff.mp hx).1]; exact hx⟩⟩
  cases hh with
  | refMake hv =>
    have hvn := hv.noSettle hr.2.1
    obtain ⟨hm', hrun'⟩ := appendMem hvn
    exact ⟨trivial, hrgs, hm', fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hvn l], fun l h _ => hrun' l h⟩⟩
  | refGet hl =>
    have hvn := hmem _ _ hl
    exact ⟨Tm.SettleLin.of_noSettle hvn, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hvn l], fun _ => sameMem⟩⟩
  | refGetCell | forceRunning =>
    exact ⟨trivial, hrgs, hmem, fun _ => rfl, .inl ⟨fun _ => rfl, fun _ => sameMem⟩⟩
  | @refSet v l₀ _ _ hv hlt =>
    have hvn := hv.noSettle hr.2.1
    refine ⟨trivial, hrgs, fun i w hw => ?_, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hvn l], fun l ⟨e, he⟩ hcell => ?_⟩⟩
    · rcases mem_set_get hw with hw | ⟨-, rfl⟩
      · exact hmem i w hw
      · exact hvn
    · by_cases heq : l₀ = l
      · subst heq
        obtain ⟨d', hd', hdc⟩ := hcell l₀ (.running e) he trivial
        rw [List.getElem?_set_self hlt] at hd'
        cases hd'
        exact (hv.not_cell hdc).elim
      · exact ⟨e, by rw [List.getElem?_set_ne heq]; exact he⟩
  | await =>
    exact ⟨Tm.SettleLin.of_noSettle hr, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hr l], fun _ => sameMem⟩⟩
  | lazy =>
    rename_i e₀
    obtain ⟨hm', hrun'⟩ := appendMem (c := .lazy e₀) (fun l => by simp [Tm.settleCount, hr l])
    exact ⟨trivial, hrgs, hm', fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hr l], fun l h _ => hrun' l h⟩⟩
  | forcePending hl =>
    rename_i l₀ e₀
    have he₀ : e₀.NoSettle := fun l => by have := hmem _ _ hl l; simpa [Tm.settleCount] using this
    refine ⟨show e₀.SettleLin from Tm.SettleLin.of_noSettle he₀, hrgs, fun i w hw => ?_, fun _ => rfl,
      .inr (.inl ⟨l₀, (fun ⟨_, h⟩ => by rw [hl] at h; cases h),
        ⟨e₀, List.getElem?_set_self (List.getElem?_eq_some_iff.mp hl).1⟩,
        fun l => by simp [Tm.settleCount, he₀ l],
        fun l hne ⟨x, hx⟩ => ⟨x, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hx⟩⟩)⟩
    rcases mem_set_get hw with hw | ⟨-, rfl⟩
    · exact hmem i w hw
    · intro l; simpa [Tm.settleCount] using he₀ l
  | forceDone hl =>
    have ho : r'.NoSettle := fun l => by have := hmem _ _ hl l; simpa [Tm.settleCount] using this
    exact ⟨Tm.SettleLin.of_noSettle ho, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, ho l], fun _ => sameMem⟩⟩
  | settleVal hv =>
    rename_i l₀
    have hvn := hv.noSettle hr
    refine ⟨Tm.SettleLin.of_noSettle hvn, hrgs, fun i w hw => ?_, fun _ => rfl,
      .inr (.inr ⟨l₀, by simp [Tm.settleCount],
        fun l => by simp [Tm.settleCount, hvn l],
        fun l hne ⟨x, hx⟩ => ?_⟩)⟩
    · rcases settleCell_get hw with hw | ⟨-, rfl⟩
      · exact hmem i w hw
      · intro l; simpa [Tm.settleCount] using hvn l
    · unfold settleCell; split
      · exact ⟨x, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hx⟩
      · exact ⟨x, hx⟩
  | settleRaise hv =>
    rename_i v l₀
    have hvn := hv.noSettle hr
    refine ⟨Tm.SettleLin.of_noSettle hvn, hrgs, fun i w hw => ?_, fun _ => rfl,
      .inr (.inr ⟨l₀, by simp [Tm.settleCount],
        fun l => by simp [Tm.settleCount, hvn l],
        fun l hne ⟨x, hx⟩ => ?_⟩)⟩
    · rcases settleCell_get hw with hw | ⟨-, rfl⟩
      · exact hmem i w hw
      · intro l; simpa [Tm.settleCount] using hvn l
    · unfold settleCell; split
      · exact ⟨x, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hx⟩
      · exact ⟨x, hx⟩
  | settleUnw =>
    rename_i l₀ p ℓ a
    refine ⟨hr, hrgs, fun i w hw => ?_, fun _ => rfl,
      .inr (.inr ⟨l₀, by simp [Tm.settleCount],
        fun l => by simp [Tm.settleCount, hr l],
        fun l hne ⟨x, hx⟩ => ?_⟩)⟩
    · rcases resetCell_get hw with hw | ⟨-, _, hx, rfl⟩
      · exact hmem i w hw
      · intro l; have := hmem _ _ hx l; simpa [Tm.settleCount] using this
    · rcases resetCell_cases μ l₀ with ⟨_, _, hrc⟩ | ⟨-, hrc⟩
      · rw [hrc]; exact ⟨x, by rw [List.getElem?_set_ne (Ne.symm hne)]; exact hx⟩
      · rw [hrc]; exact ⟨x, hx⟩
  | tryRaise hv =>
    have hvn := hv.noSettle hr.1
    exact ⟨⟨hvn, hr.2⟩, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hvn l, hr.2 l], fun _ => sameMem⟩⟩
  | handleYes _ hm =>
    have hb : ∀ {p b r₀}, (Tm.acons p b r₀).NoSettle → b.NoSettle ∧ r₀.NoSettle :=
      fun h => ⟨fun l => by have := h l; simp only [Tm.settleCount] at this; omega,
        fun l => by have := h l; simp only [Tm.settleCount] at this; omega⟩
    obtain ⟨hb', _⟩ := hb hr.2
    have hsub := hb'.subst_inst (hr.1.of_matchP _ hm)
    exact ⟨Tm.SettleLin.of_noSettle hsub, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by have := hr.2 l; simp only [Tm.settleCount] at this ⊢; rw [hr.1 l, hsub l]; omega,
        fun _ => sameMem⟩⟩
  | handleNo _ _ =>
    have hb : ∀ {p b r₀}, (Tm.acons p b r₀).NoSettle → b.NoSettle ∧ r₀.NoSettle :=
      fun h => ⟨fun l => by have := h l; simp only [Tm.settleCount] at this; omega,
        fun l => by have := h l; simp only [Tm.settleCount] at this; omega⟩
    obtain ⟨_, hrest⟩ := hb hr.2
    exact ⟨⟨hr.1, hrest⟩, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by have := hr.2 l; simp only [Tm.settleCount] at this ⊢; rw [hr.1 l, hrest l]; omega,
        fun _ => sameMem⟩⟩
  | hguardYes _ hm =>
    have hag : ∀ {p g b r₀}, (Tm.aguard p g b r₀).NoSettle → g.NoSettle ∧ b.NoSettle ∧ r₀.NoSettle :=
      fun h => ⟨fun l => by have := h l; simp only [Tm.settleCount] at this; omega,
        fun l => by have := h l; simp only [Tm.settleCount] at this; omega,
        fun l => by have := h l; simp only [Tm.settleCount] at this; omega⟩
    obtain ⟨hg, hb, hrest⟩ := hag hr.2
    have hgs := hg.subst_inst (hr.1.of_matchP _ hm)
    have hbs := hb.subst_inst (hr.1.of_matchP _ hm)
    refine ⟨⟨Tm.SettleLin.of_noSettle hgs, fun l => by simp [Tm.settleCount, hbs l, hr.1 l, hrest l]⟩,
      hrgs, hmem, fun _ => rfl, .inl ⟨fun l => by
        have := hr.2 l; simp only [Tm.settleCount] at this ⊢
        rw [hgs l, hbs l, hr.1 l, hrest l]; omega, fun _ => sameMem⟩⟩
  | hguardNo _ _ =>
    have hag : ∀ {p g b r₀}, (Tm.aguard p g b r₀).NoSettle → g.NoSettle ∧ b.NoSettle ∧ r₀.NoSettle :=
      fun h => ⟨fun l => by have := h l; simp only [Tm.settleCount] at this; omega,
        fun l => by have := h l; simp only [Tm.settleCount] at this; omega,
        fun l => by have := h l; simp only [Tm.settleCount] at this; omega⟩
    obtain ⟨_, _, hrest⟩ := hag hr.2
    exact ⟨⟨hr.1, hrest⟩, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by have := hr.2 l; simp only [Tm.settleCount] at this ⊢; rw [hr.1 l, hrest l]; omega,
        fun _ => sameMem⟩⟩
  | handleNil _ =>
    exact ⟨Tm.SettleLin.of_noSettle hr.1, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hr.1 l], fun _ => sameMem⟩⟩
  | scopeRun hv =>
    rename_i f₀
    have hfn := hv.noSettle hr.2.1
    have hns : (Tm.app (f₀.rename (· + 1)) (.var 0)).NoSettle := fun l => by
      simp [Tm.settleCount, Tm.settleCount_rename, hfn l]
    exact ⟨hns, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, Tm.settleCount_rename, hfn l], fun _ => sameMem⟩⟩
  | scope =>
    have hsub := hr.subst_inst (δ := [.own Λ.length]) (by simp; intro l; rfl)
    refine ⟨Tm.SettleLin.of_noSettle hsub, fun q g hq k hk => ?_, hmem,
      fun l => rgsSettleCount_append_new l Λ,
      .inl ⟨fun l => by simp [Tm.settleCount, hr l, hsub l], fun _ => sameMem⟩⟩
    by_cases hql : q < Λ.length
    · rw [List.getElem?_append_left hql] at hq; exact hrgs q g hq k hk
    · rw [List.getElem?_append_right (by omega)] at hq
      cases hj : q - Λ.length with
      | zero => simp [hj] at hq; subst hq; simp [Rgn.new] at hk
      | succ => simp [hj] at hq
  | open_ hg =>
    rename_i p₀ g₀ _
    refine ⟨trivial, fun q g' hq k hk => ?_, hmem, fun l => ?_,
      .inl ⟨fun _ => rfl, fun _ => sameMem⟩⟩
    · rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
      · simp only [List.mem_append] at hk
        rcases hk with hk | hk | hk
        · exact hrgs _ _ hg k (List.mem_append_left _ hk)
        · exact hrgs _ _ hg k (List.mem_append_right _ hk)
        · simp at hk; subst hk; exact ⟨trivial, trivial, .inl fun _ => rfl⟩
      · exact hrgs q g' hq k hk
    · have h1 := rgsSettleCount_setRgn l hg
        (g' := ⟨g₀.live, g₀.kids, g₀.fins ++ [.app (.const .op) (.cap p₀)]⟩)
      simp only [Rgn.settleCount, List.map_append, List.sum_append, List.map_singleton,
        List.sum_cons, List.sum_nil, Tm.settleCount] at h1
      omega
  | op => exact ⟨trivial, hrgs, hmem, fun _ => rfl, .inl ⟨fun _ => rfl, fun _ => sameMem⟩⟩
  | finRun hg _ _ _ hfs =>
    rename_i g₀ _ f₀ fs₀ _ _ _
    refine ⟨hr, fun q g' hq k hk => ?_, hmem, fun l => ?_,
      .inl ⟨fun _ => rfl, fun _ => sameMem⟩⟩
    · rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
      · apply hrgs _ _ hg k
        simp only [List.mem_append, hfs] at hk ⊢
        rcases hk with (hk | hk) | hk
        · exact .inl hk
        · simp at hk; subst hk; simp
        · simp [hk]
      · exact hrgs q g' hq k hk
    · have h1 := rgsSettleCount_setRgn l hg (g' := ⟨g₀.live, g₀.kids ++ [f₀], fs₀⟩)
      simp only [Rgn.settleCount, hfs, List.map_append, List.sum_append, List.map_singleton,
        List.sum_cons, List.sum_nil] at h1
      omega
  | close hg _ _ _ hfs =>
    rename_i g₀ _ _ _
    refine ⟨hr, fun q g' hq k hk => ?_, hmem, fun l => ?_,
      .inl ⟨fun _ => rfl, fun _ => sameMem⟩⟩
    · rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
      · apply hrgs _ _ hg k
        simp only [List.mem_append, List.not_mem_nil, or_false] at hk
        exact List.mem_append_left _ hk
      · exact hrgs q g' hq k hk
    · have h1 := rgsSettleCount_setRgn l hg (g' := ⟨false, g₀.kids, []⟩)
      simp only [Rgn.settleCount, hfs] at h1
      omega
  | hctlRaise hE hk hv =>
    subst hk
    have hw := hv.noSettle hr.2.2.1
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE hr.2.2.2
    refine ⟨⟨hr.1, hr.2.1, hSE.fill (fun l => by simp [Tm.settleCount, hw l])⟩,
      hrgs, hmem, fun _ => rfl, .inl ⟨fun l => by
        simp only [Tm.settleCount, plug_settleCount, hr.1 l, hr.2.1 l, hw l],
        fun _ => sameMem⟩⟩
  | hctlUnw hE hk =>
    subst hk
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE hr.2.2.2
    refine ⟨⟨hr.1, hr.2.1, hSE.fill (fun l => by simp [Tm.settleCount, hr.2.2.1 l])⟩,
      hrgs, hmem, fun _ => rfl, .inl ⟨fun l => by
        simp only [Tm.settleCount, plug_settleCount, hr.1 l, hr.2.1 l, hr.2.2.1 l],
        fun _ => sameMem⟩⟩
  | hresRaise hA ha hv =>
    subst ha
    have hw := hv.noSettle hr.2
    obtain ⟨hSA, -⟩ := settleLin_of_plug hole_not_value hA hr.1
    refine ⟨hSA.fill (Tm.SettleLin.of_noSettle hw), hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp only [Tm.settleCount, plug_settleCount, hw l],
        fun _ => sameMem⟩⟩
  | hresUnw hA ha =>
    subst ha
    obtain ⟨hSA, -⟩ := settleLin_of_plug hole_not_value hA hr.1
    refine ⟨hSA.fill hr.2, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp only [Tm.settleCount, plug_settleCount, hr.2 l],
        fun _ => sameMem⟩⟩
  | hkarmRaise hE hk hv =>
    subst hk
    have hw := hv.noSettle hr.2.1
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE hr.2.2
    refine ⟨⟨hr.1, hSE.fill (fun l => by simp [Tm.settleCount, hw l])⟩, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp only [Tm.settleCount, plug_settleCount, hr.1 l, hw l],
        fun _ => sameMem⟩⟩
  | hkarmUnw hE hk =>
    subst hk
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE hr.2.2
    refine ⟨⟨hr.1, hSE.fill hr.2.1⟩, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp only [Tm.settleCount, plug_settleCount, hr.1 l, hr.2.1 l],
        fun _ => sameMem⟩⟩
  | hwithCatch =>
    exact ⟨Tm.SettleLin.of_noSettle hr.2.2, hrgs, hmem, fun _ => rfl,
      .inl ⟨fun l => by simp [Tm.settleCount, hr.1 l, hr.2.1 l, hr.2.2 l], fun _ => sameMem⟩⟩
  | hdl =>
    obtain ⟨hm', hrun'⟩ := appendMem (c := .lam .todo) (fun _ => rfl)
    refine ⟨⟨hr.1, hr.2.1, Tm.SettleLin.of_noSettle hr.2.2⟩, fun q g hq k hk => ?_, hm',
      fun l => rgsSettleCount_append_new l Λ,
      .inl ⟨fun l => by simp [Tm.settleCount, hr.1 l, hr.2.1 l, hr.2.2 l], fun l h _ => hrun' l h⟩⟩
    by_cases hql : q < Λ.length
    · rw [List.getElem?_append_left hql] at hq; exact hrgs q g hq k hk
    · rw [List.getElem?_append_right (by omega)] at hq
      cases hj : q - Λ.length with
      | zero => simp [hj] at hq; subst hq; simp [Rgn.new] at hk
      | succ => simp [hj] at hq


end Flick
