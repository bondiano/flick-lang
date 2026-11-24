import Formal.Settle

/-!
# Steps preserve the reachability invariant of `settle` frames

Every step of a thread either keeps the count of every `settle l` frame unchanged, starts a
pending cell `l₀` (adding one `settle l₀` frame and making `l₀` running), or finishes a `settle l₀`
frame (removing one `settle l₀` frame). Given that `Ref.set` never overwrites a lazy cell
(proved from `StoreTy` + `Typ.value_not_cell` in `Reach`), `CStep` and `CSteps` preserve
`CfgSettle`.
-/

namespace Flick

/-- How one thread step affects the `settle` frame counts and `Running` cells of the store. -/
def SettleDelta (μ μ' : List Tm) (t t' : Tm) : Prop :=
  ((∀ l, t'.settleCount l = t.settleCount l) ∧
    (∀ l, Running μ l →
      (∀ (i : Nat) (d : Tm), μ[i]? = some d → d.IsCell → ∃ d' : Tm, μ'[i]? = some d' ∧ d'.IsCell) →
      Running μ' l)) ∨
  (∃ l₀, ¬ Running μ l₀ ∧ Running μ' l₀ ∧
    (∀ l, t'.settleCount l = t.settleCount l + if l₀ = l then 1 else 0) ∧
    (∀ l, l ≠ l₀ → Running μ l → Running μ' l)) ∨
  (∃ l₀, 1 ≤ t.settleCount l₀ ∧
    (∀ l, t'.settleCount l + (if l₀ = l then 1 else 0) = t.settleCount l) ∧
    (∀ l, l ≠ l₀ → Running μ l → Running μ' l))

theorem PHead.settle {r r' : Tm} (hh : PHead r r') (h : r.SettleLin) :
    r'.SettleLin ∧ ∀ l, r'.settleCount l = r.settleCount l := by
  cases hh with
  | @beta v b hv =>
    have h₁ := hv.noSettle h.2.1
    have hsub := h.1.subst_inst (δ := [v]) (by simpa using h₁)
    exact ⟨Tm.SettleLin.of_noSettle hsub, fun l => by simp [Tm.settleCount, h₁ l, h.1 l, hsub l]⟩
  | @fix v f hv =>
    have h₁ := hv.noSettle h.2.1
    have hsub := h.1.subst_inst (δ := [v, .fix f]) (by
      intro w hw; simp only [List.mem_cons, List.not_mem_nil, or_false] at hw
      rcases hw with rfl | rfl
      · exact h₁
      · exact h.1)
    exact ⟨Tm.SettleLin.of_noSettle hsub, fun l => by simp [Tm.settleCount, h₁ l, h.1 l, hsub l]⟩
  | @let_ v b hv =>
    have h₁ := hv.noSettle h.1
    have hsub := h.2.subst_inst (δ := [v]) (by simpa using h₁)
    exact ⟨Tm.SettleLin.of_noSettle hsub, fun l => by simp [Tm.settleCount, h₁ l, h.2 l, hsub l]⟩
  | @letrec f b =>
    have hsub := h.2.subst_inst (δ := [.fix f]) (by simpa using h.1)
    exact ⟨Tm.SettleLin.of_noSettle hsub, fun l => by simp [Tm.settleCount, h.1 l, h.2 l, hsub l]⟩
  | proj hv hf =>
    have h₁ := hv.noSettle h
    have h₂ := h₁.field hf
    exact ⟨Tm.SettleLin.of_noSettle h₂, fun l => by simp [Tm.settleCount, h₁ l, h₂ l]⟩
  | update hv hw hf =>
    have h₁ := hv.noSettle h.1
    have h₂ := hw.noSettle h.2.1
    have h₃ := h₁.setField h₂ hf
    exact ⟨Tm.SettleLin.of_noSettle h₃, fun l => by simp [Tm.settleCount, h₁ l, h₂ l, h₃ l]⟩
  | @matchYes v p δ b r₀ hv hm =>
    have h₁ := hv.noSettle h.1
    have hb : b.NoSettle := fun l => by have := h.2 l; simp only [Tm.settleCount] at this; omega
    have hsub := hb.subst_inst (h₁.of_matchP _ hm)
    exact ⟨Tm.SettleLin.of_noSettle hsub, fun l => by
      have hr := h.2 l; simp only [Tm.settleCount] at hr ⊢; rw [h₁ l, hsub l]; omega⟩
  | @matchNo v p b r₀ hv =>
    have h₁ := hv.noSettle h.1
    have hr : r₀.NoSettle := fun l => by have := h.2 l; simp only [Tm.settleCount] at this; omega
    exact ⟨⟨Tm.SettleLin.of_noSettle h₁, hr⟩, fun l => by
      have h2 := h.2 l; simp only [Tm.settleCount] at h2 ⊢; rw [h₁ l, hr l]; omega⟩
  | @guardYes v p δ g b r₀ hv hm =>
    have h₁ := hv.noSettle h.1
    have hg : g.NoSettle := fun l => by have := h.2 l; simp only [Tm.settleCount] at this; omega
    have hb : b.NoSettle := fun l => by have := h.2 l; simp only [Tm.settleCount] at this; omega
    have hr : r₀.NoSettle := fun l => by have := h.2 l; simp only [Tm.settleCount] at this; omega
    have hgs := hg.subst_inst (h₁.of_matchP _ hm)
    have hbs := hb.subst_inst (h₁.of_matchP _ hm)
    refine ⟨⟨Tm.SettleLin.of_noSettle hgs, fun l => by
      simp only [Tm.settleCount, hbs l, h₁ l, hr l]⟩, fun l => by
      have h2 := h.2 l; simp only [Tm.settleCount] at h2 ⊢
      rw [hgs l, hbs l, h₁ l, hr l]; omega⟩
  | @guardNo v p g b r₀ hv =>
    have h₁ := hv.noSettle h.1
    have hr : r₀.NoSettle := fun l => by have := h.2 l; simp only [Tm.settleCount] at this; omega
    exact ⟨⟨Tm.SettleLin.of_noSettle h₁, hr⟩, fun l => by
      have h2 := h.2 l; simp only [Tm.settleCount] at h2 ⊢; rw [h₁ l, hr l]; omega⟩
  | tryVal hv =>
    have h₁ := hv.noSettle h.1
    exact ⟨Tm.SettleLin.of_noSettle h₁, fun l => by simp [Tm.settleCount, h₁ l, h.2 l]⟩
  | intEq => exact ⟨trivial, fun _ => rfl⟩
  | @funcRef v hv =>
    have h₁ := hv.noSettle h.2.1
    exact ⟨Tm.SettleLin.of_noSettle h₁, fun l => by simp [Tm.settleCount, h₁ l]⟩
  | hwithVal hv =>
    have h₁ := hv.noSettle h.2.2
    exact ⟨⟨Tm.SettleLin.of_noSettle h.2.1, Tm.SettleLin.of_noSettle h₁, .inl h₁⟩,
      fun l => by simp [Tm.settleCount, h.1 l, h.2.1 l, h₁ l]⟩
  | hctlTail hE hk hv =>
    subst hk
    have hw := hv.noSettle h.2.2.1
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE h.2.2.2
    refine ⟨⟨h.1, h.2.1, hSE.fill (Tm.SettleLin.of_noSettle hw)⟩, fun l => by
      simp only [Tm.settleCount, plug_settleCount, h.1 l, h.2.1 l, hw l]⟩
  | hctlDisc hE hk hv =>
    subst hk
    have hw := hv.noSettle h.2.2.1
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE h.2.2.2
    refine ⟨⟨h.1, h.2.1, hSE.fill hw⟩, fun l => by
      simp only [Tm.settleCount, plug_settleCount, h.1 l, h.2.1 l, hw l]⟩
  | hresVal hA ha hv =>
    subst ha
    have hw := hv.noSettle h.2
    obtain ⟨hSA, -⟩ := settleLin_of_plug hole_not_value hA h.1
    refine ⟨hSA.fill (Tm.SettleLin.of_noSettle hw), fun l => by
      simp only [Tm.settleCount, plug_settleCount, hw l]⟩
  | hdoneVal _ =>
    exact ⟨h, fun _ => by simp [Tm.settleCount]⟩
  | hkeepVal _ =>
    exact ⟨h.2, fun l => by simp [Tm.settleCount, h.1 l]⟩
  | hkarmVal hE hk hv =>
    subst hk
    have hw := hv.noSettle h.2.1
    obtain ⟨hSE, -⟩ := settleLin_of_plug hole_not_value hE h.2.2
    refine ⟨⟨h.1, hSE.fill (Tm.SettleLin.of_noSettle hw)⟩, fun l => by
      simp only [Tm.settleCount, plug_settleCount, h.1 l, hw l]⟩


end Flick
