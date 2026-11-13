import Formal.FrameTy

/-!
# Typing through evaluation contexts (L3)
-/

namespace Flick

variable {S : Sig}

private theorem Typ.frameCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ r F t φ, Γ = [] → e = F.plug r → J = .ex t φ →
      ¬ Value r → F.ok → FrameTy S Ψ Θ F r t φ := by
  induction h with
  | conv _ he ih =>
    intro r F t φ hctx hterm hjudge hr hok
    have ht := (ih r F _ _ hctx hterm rfl hr hok).conv he
    cases hjudge
    exact ht
  | allI hv | errVal hv =>
    intro r F t φ _ hterm _ hr _
    rw [hterm] at hv
    exact (not_value_plug hr F hv).elim
  | @hwith _ _ _ _ _ _ _ _ p _ _ _ _ _ _ _ _ _ hℓ hal hrf hψ he hφ₀ hc hφ hT hat =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case hwith =>
      cases hterm.symm
      refine .mk0 ⟨_, _, he, .installs rfl hφ₀ hφ, fun σ hσ X _ h' => ?_⟩
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      exact .hwith (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ (hrf.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inl h)))) X) (by simpa only [Stage.use_substT] using Stage.le_substT σ hψ) h'
        (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (Stage.le_substT σ hφ) (hT.substT σ)
        (Stage.substT_svc_atm hp hat)
    all_goals cases hterm
  | @hctl _ _ _ _ _ _ _ _ p _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ tl hℓ hal hrf hψ hop hc htl hE hk hp' hφ₀ ha hl hcm hφ hT hat =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case hctlA =>
      cases hterm.symm
      refine .mk0 ⟨_, _, ha, FRel.use_rgn (fun x hx => by
        cases tl with
        | true => exact absurd hx (by simp [Caps.none])
        | false => exact .inr ⟨p, rfl, congrArg Elem.atm hx⟩)
        (Stage.use_mono hφ (Caps.Sub.refl _)), fun σ hσ X _ h' => ?_⟩
      subst hk
      obtain ⟨-, hb⟩ := S.op_closed _ _ _ _ _ σ hop
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      have h₃ := hp'.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inr (.inr
        (plug_has_of_nosite (by simp [Tm.Has]) h)))))) X
      refine .hctl (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ (hrf.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inl h)))) X) (by simpa only [Stage.use_substT] using Stage.le_substT σ hψ)
        hop hc htl hE rfl h₃ (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl)))) ?_
        (fun h => ?_) ⟨hcm.1, fun h => .inl (hcm.2 h)⟩ (Stage.le_substT σ hφ) (hT.substT σ)
        (Stage.substT_svc_atm hp hat)
      · rw [Stage.use_substT_eq _ (Caps.substT_if_rg hp)] at h'
        cases tl <;> simpa [hb] using h'
      · obtain ⟨C', hl₁, hl₂⟩ := hl h
        refine ⟨C'.substT σ, ?_, ⟨_, hl₂, by simp [hp]⟩⟩
        rw [List.getElem?_append_left (by simpa using getElem?_lt hl₁), List.getElem?_map, hl₁]
        simp [Ty.substT, Stage.substT, hb]
    all_goals cases hterm
  | @hkeep _ _ _ _ _ _ _ _ q _ _ _ _ _ _ _ _ hℓ hal he hφ₀ hc hnt hle hφ hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case hkeep =>
      cases hterm.symm
      refine .mk0 ⟨_, _, he, .installs rfl hφ₀ hφ, fun σ hσ X _ h' => ?_⟩
      have hp : σ (.r q) = .rg q := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      exact .hkeep (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ h'
        (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (fun h => .inl (hnt h))
        (by simpa [Stage.substT] using Stage.le_substT σ hle) (Stage.le_substT σ hφ) (hT.substT σ)
    all_goals cases hterm
  | @hkarm _ _ _ _ _ _ _ _ q _ _ _ _ _ _ _ _ _ _ _ _ _ hℓ hal hop hE hk hp' hφ₀ hc hnt ha hle hφ hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case hkarmA =>
      cases hterm.symm
      refine .mk0 ⟨_, _, ha, .of_le (Stage.le_trans hle hφ), fun σ hσ X _ h' => ?_⟩
      subst hk
      obtain ⟨-, hb⟩ := S.op_closed _ _ _ _ _ σ hop
      have hp : σ (.r q) = .rg q := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      have h₃ := hp'.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inr
        (plug_has_of_nosite (by simp [Tm.Has]) h))))) X
      exact .hkarm (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ hop hE rfl h₃
        (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (fun h => .inl (hnt h)) (by simpa [hb] using h')
        (by simpa [Stage.substT] using Stage.le_substT σ hle) (Stage.le_substT σ hφ) (hT.substT σ)
    all_goals cases hterm
  | @hres _ _ _ _ _ _ _ _ p _ _ _ _ _ _ _ hℓ hb hA ha hp' hl hcp hφ hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case hresB =>
      cases hterm.symm
      refine .mk0 ⟨_, _, hb, FRel.use_rgn (fun x hx => by
        exact .inr ⟨p, rfl, congrArg Elem.atm hx⟩)
        (Stage.use_mono hφ (Caps.Sub.refl _)), fun σ hσ X _ h' => ?_⟩
      subst ha
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inr (.inl rfl))
      rw [Stage.use_substT_rg _ hp] at h'
      have h₂ := hp'.sx (hσ.sub fun s h => by
        rcases plug_has_iff.mp h with h | h
        · exact .inr (.inr (.inl (plug_has_iff.mpr (.inl h))))
        · simp only [Tm.Has] at h; rcases h with h | h
          · subst h; exact .inr (.inl rfl)
          · exact h.elim) X
      simp only [Flick.J.substT, Stage.use_substT_rg _ hp] at h₂
      exact .hres (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h' hA rfl h₂
        (by rw [List.getElem?_append_left (by simpa using getElem?_lt hl), List.getElem?_map, hl]; rfl)
        (by exact ⟨.r _, hcp, by rw [hp]; rfl⟩) (Stage.le_substT σ hφ) (hT.substT σ)
    all_goals cases hterm
  | @hdone _ _ _ _ _ _ p ha =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case hdone =>
      cases hterm.symm
      refine .mk0 ⟨_, _, ha, FRel.use_rgn (fun x hx => by
        exact .inr ⟨p, rfl, congrArg Elem.atm hx⟩)
        (Stage.le_refl _), fun σ hσ X _ h' => ?_⟩
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inr (.inl rfl))
      rw [Stage.use_substT_rg _ hp] at h'
      exact .hdone h'
    all_goals cases hterm
  | app h₁ h₂ hψ hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case appL =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .app h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (by simpa only [Stage.use_substT] using Stage.le_substT σ hψ)
        (hT.substT σ)⟩
    case appR =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .app (h₁.sx (hσ.sub fun _ h => .inl h) X) h' (by simpa only [Stage.use_substT] using Stage.le_substT σ hψ)
        (hT.substT σ)⟩
    all_goals cases hterm
  | let_ h₁ h₂ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case let_ =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .let_ h' (h₂.sx1 (hσ.sub fun _ h => .inr h) X)⟩
    all_goals cases hterm
  | letGen h₁ h₂ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case let_ =>
      cases hterm.symm
      exact ⟨_, _, _, _, h₁, ⟨fun _ h => h.elim, fun _ _ h => h.elim, fun _ => rfl,
        fun _ h => (h rfl).elim⟩, .inl rfl, fun σ hσ X _ h' => .letGen (by
          simpa [Facts.union_substT, Facts.up_comm] using h')
          (by simpa [Scheme.substT] using (h₂.substT σ (hσ.sub fun _ h => .inr h)).weakenStore X)⟩
    all_goals cases hterm
  | match_ h₁ h₂ hc =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case match_ =>
      cases hterm.symm
      have hx : Exh S _ _ := hc.resolve_right fun ⟨hv, _⟩ => hr hv
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .match_ h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (.inl fun v hs => hx v (Shp.of_substT hs))⟩
    all_goals cases hterm
  | rcons hR hl h₁ h₂ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case rconsL =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl) h'
          (h₂.sx (hσ.sub fun _ h => .inr h) X)⟩
    case rconsR =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl)
          (h₁.sx (hσ.sub fun _ h => .inl h) X) h'⟩
    all_goals cases hterm
  | proj h₁ hR hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case proj =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .proj h' (Ty.field_substT σ hR) (hT.substT σ)⟩
    all_goals cases hterm
  | dproj h₁ hc hk hl hts hH hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case proj =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ _ _ h' => .dproj_at h' hc hk hl hts hH hT⟩
    all_goals cases hterm
  | update h₁ hR h₂ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case updL =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .update h' (Ty.field_substT σ hR) (h₂.sx (hσ.sub fun _ h => .inr h) X)⟩
    case updR =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .update (h₁.sx (hσ.sub fun _ h => .inl h) X) (Ty.field_substT σ hR) h'⟩
    all_goals cases hterm
  | await h₁ hφ hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case await =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .await h' (by simpa only [Stage.use_substT] using Stage.le_substT σ hφ) (hT.substT σ)⟩
    all_goals cases hterm
  | raise h₁ hφ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case raise =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .raise h'
        (by simpa [Stage.substT] using Stage.le_substT σ hφ)⟩
    all_goals cases hterm
  | raiseAdt h₁ hU hφ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case raise =>
      cases hterm.symm
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .raiseAdt h'
        (Ty.hd_substT σ hU) (by simpa [Stage.substT, Row.err_substT] using Stage.le_substT σ hφ)⟩
    all_goals cases hterm
  | tryR h₁ h₂ hφ hs =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case try_ =>
      cases hterm.symm
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₁, ⟨fun n hn => .inl (Row.le_modNames (hr' n hn)), fun h => (h trivial).elim,
        fun hp => (by cases hp), fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => .tryR h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (Stage.le_substT σ hφ)
          (hs.back_substT σ)⟩
    all_goals cases hterm
  | tryP h₁ h₂ =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case try_ =>
      cases hterm.symm
      refine .mk0 ⟨_, _, h₁, ⟨fun _ h => h.elim, fun h => (h trivial).elim, fun _ => rfl,
        fun h => (h trivial).elim⟩, fun σ hσ X _ h' => .tryP h' (by
          have := h₂.sx (hσ.sub fun _ h => .inr h) X
          simp only [Flick.J.substT, Row.none_substT] at this; exact this)⟩
    all_goals cases hterm
  | @inside _ p _ _ _ _ _ _ _ _ _ _ hF h₁ hφ' ht hε hφ hT =>
    intro r F t φ hctx hterm hjudge hr hok
    cases hctx; cases hjudge
    cases F <;> simp only [Frame.plug] at hterm
    case inside =>
      cases hterm.symm
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine ⟨0, _, _, _, by simpa using h₁, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp),
        fun _ _ h => (by cases h)⟩, .inr ⟨rfl, Ext.inside hF, fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => ?_⟩
      · rcases Stage.le_svc hφ' n hn with h | h | h
        · cases n with
          | atm a => exact .inr (Frame.provides_of (F := .inside p) ⟨p, rfl, h⟩)
          | _ => exact h.elim
        · exact .inl (Row.le_modNames (hr' n h))
        · cases n with
          | ob N a => exact .inr (Frame.provides_of (F := .inside p) (.inr ⟨p, rfl, h.1⟩))
          | obv v a => exact .inr (Frame.provides_of (F := .inside p) ⟨p, rfl, h.1⟩)
          | _ => exact h.elim
      · exact (Stage.le_err hφ').trans he'
      · have hp : σ (.r p) = .rg p := hσ.1 _ (.inl rfl)
        have hav := hσ.2.1 (.inr (.inl rfl))
        rw [List.map_up_zero, liftT_zero, Facts.union_substT] at h'
        refine .inside ?_ h' ?_ (fun h => ht ?_) (Ty.no_rg_substT (t := .errors _) hav hε)
          (Stage.le_substT σ hφ) (hT.substT σ)
        · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
          obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
          exact ⟨by simp [Ty.substT, hp], fun q ⟨b, hb, h''⟩ => by rw [hav b q h''] at hb; exact hC q hb⟩
        · have := Stage.le_substT σ hφ'
          simp only [Stage.substT, Row.union_substT, Row.scope_substT, Ty.substT, hp] at this
          refine Stage.le_rt_sub this ?_
          rintro x (hx | hx | hx)
          · exact .inl hx
          · exact .inr (.inl hx)
          · exact .inr (.inr (Row.disch_substT (fun y hy => by rw [hp] at hy; exact hy) x hx))
        · obtain ⟨b, hb, h''⟩ := (Ty.reach_substT σ _ _).mp h
          rwa [hav b _ h''] at hb
    all_goals cases hterm
  | _ =>
    intro r F t φ _ hterm hjudge _ _
    cases hjudge <;> cases F <;> simp [Frame.plug] at hterm

theorem frame_typing {Ψ Θ r t φ} (hr : ¬ Value r) (F : Frame) (hF : F.ok)
    (h : Typ S Ψ Θ [] (F.plug r) (.ex t φ)) : FrameTy S Ψ Θ F r t φ :=
  h.frameCore r F t φ rfl rfl rfl hr hF

end Flick
