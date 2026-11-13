import Formal.ProgressMain
import Formal.SpineMatch
import Formal.PresHdl

/-!
# Preservation of pure steps
-/

namespace Flick

variable {S : Sig}

theorem exh_bool : Exh S (.base .bool) [.lit (.bool true), .lit (.bool false)] := by
  intro v hs
  have : ∀ {fl T}, Shp S fl T v → T = .base .bool → fl = true → ∃ b, v = .lit (.bool b) := by
    intro fl T h hT hf
    cases h with
    | @lit l => cases l <;> simp [Lit.base] at hT; exact ⟨_, rfl⟩
    | hidden ho => subst hT; simp [Ty.Opaque, Ty.hd] at ho
    | head => cases hf
    | app => cases hf
    | rnil => cases hT
    | rcons => cases hT
    | adt _ _ h => subst hT; simp [Ty.hd] at h
  obtain ⟨b, rfl⟩ := this hs rfl rfl
  cases b <;> simp [matchP]

/-- After a failed arm, the remaining arms still cover the value (`docs/formal/core.md#the-residual-match-after-a-false-guard-fails-exh`). -/
theorem Covers.tail {t p ps v} (hc : Covers S t (p :: ps) v) (hv : Value v)
    (hs : Shp S true t v) (hm : matchP p v = none) : Covers S t ps v := by
  have key : ∀ {q}, q ∈ p :: ps → (matchP q v).isSome → Covers S t ps v := by
    intro q hq hqm
    rcases List.mem_cons.mp hq with rfl | hq
    · simp [hm] at hqm
    · exact .inr ⟨hv, q, hq, hqm⟩
  rcases hc with hx | ⟨_, q, hq, hqm⟩
  · obtain ⟨q, hq, hqm⟩ := hx v hs; exact key hq hqm
  · exact key hq hqm

/-- The two arms that a guard step builds. -/
theorem guard_typ {Ψ Θ g b rest t' φ} (hg : Typ S Ψ Θ [] g (.ex (.base .bool) φ))
    (hb : Typ S Ψ Θ [] b (.ex t' φ)) (hr : Typ S Ψ Θ [] rest (.ex t' φ)) :
    Typ S Ψ Θ [] (.match_ g (.acons (.lit (.bool true)) b
      (.acons (.lit (.bool false)) rest .anil))) (.ex t' φ) :=
  .match_ hg (.acons .lit hb (.acons .lit hr .anil)) (.inl exh_bool)

private theorem Typ.pureStepCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ t φ e', Γ = [] → J = .ex t φ → PHead e e' → Typ S Ψ Θ [] e' (.ex t φ) := by
  induction h with
  | conv _ he ih =>
    intro t φ e' hΓ hJ hh; cases hJ
    exact (ih _ _ e' hΓ rfl hh).conv he
  | allI hv | errVal hv =>
    intro _ _ _ _ _ hh
    exact (hh.not_value hv).elim
  | app hf ha hψ hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | beta hv =>
      obtain ⟨_, _, _, _, hb, _, hT'⟩ := hf.lam_arrow
      obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := LeLive.arrow_left hT'
      cases h
      exact (hb.inst (Δ := [_]) (.cons hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩) .nil)).sub
        ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | fix hv =>
      obtain ⟨_, _, _, _, hb, hn, hT'⟩ := hf.fix_arrow
      obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := LeLive.arrow_left hT'
      cases h
      exact (hb.inst (Δ := [_, _]) (.cons hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩)
        (.cons .fix (.fix hb hn (LeLive.refl)) .nil))).sub
        ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | intEq =>
      obtain ⟨_, _, _, _, hc, _, _, hT'⟩ := hf.app_arrow
      obtain ⟨_, _, hT''⟩ := hc.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := LeLive.arrow_left hT''
      cases h
      obtain ⟨_, _, _, _, h, _, _, lt', -⟩ := LeLive.arrow_left (lt.trans hT')
      cases h
      obtain rfl := LeLive.eq_base_left lt'
      obtain rfl := LeLive.eq_base_left hT
      exact .lit
  | let_ h₁ h₂ =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | let_ hv =>
      exact h₂.inst (Δ := [_]) (.cons hv (h₁.valueAny hv _) .nil)
  | letGen h₁ h₂ =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | let_ hv =>
      exact h₂.subst (SubOk.one hv h₁)
  | letrec h₁ hn h₂ =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | letrec =>
      exact h₂.inst (Δ := [_]) (.cons .fix (.fix h₁ hn (LeLive.refl)) .nil)
  | proj h₁ hR hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | proj _ hw =>
      exact (field_typ h₁ hw hR).sub ⟨hT, Stage.le_refl _⟩
  | dproj h₁ hc hk hl hts hH hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | proj _ hw =>
      exact (dict_field ‹_› h₁ hc hl hts hH hw).sub ⟨hT, Stage.pure_le _⟩
  | update h₁ hR h₂ =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | update _ _ hv' =>
      exact setField_typ h₂ h₁ hv' hR
  | match_ h₁ hA hc =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | matchYes hv hm =>
      cases hA with
      | acons hp hb _ => exact hb.inst (hp.inst hv (h₁.valueAny hv _) hm)
      | aref hK hU hn _ _ hpA hbA _ => exact aref_match hv h₁ hK hU hn hpA hbA hm
    | matchNo hv hm =>
      cases hA with
      | acons _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
      | aref _ _ _ _ _ _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
    | guardYes hv hm =>
      cases hA with
      | aguard hp hg hb hr =>
        have hδ := hp.inst hv (h₁.valueAny hv _) hm
        exact guard_typ (hg.inst hδ) (hb.inst hδ) (.match_ h₁ hr hc)
    | guardNo hv hm =>
      cases hA with | aguard _ _ _ hr => exact .match_ h₁ hr hc
  | tryR h₁ =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | tryVal hv =>
      exact h₁.valueAny hv _
  | tryP h₁ =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | tryVal hv =>
      exact h₁.valueAny hv _
  | hwith _ _ hrf hψ he _ _ hφ hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | hwithVal hv =>
      exact .app (hrf.sub ⟨LeLive.refl, hφ⟩) (he.valueAny hv _)
        (Stage.le_trans hψ hφ) hT
  | hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha _ hcm hφ hT hat =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | hctlTail hE' hk' hv =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hE' hE (hk'.symm.trans hk)
      refine .hwith hℓ hal hrf hψ (refill_app hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ _ hb _ => ?_) hφ₀ hcm hφ hT hat
      exact (((by simpa using ha : Typ S _ _ [] _ (.ex _ _)).valueAny hv φ').sub (J' := .ex T φ')
        ⟨hb hc, Stage.le_refl _⟩).cut hΘ.entails
    | hctlDisc hE' hk' hv =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hE' hE (hk'.symm.trans hk)
      refine .hwith hℓ hal hrf hψ (refill_app hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ hψ₀ _ _ => ?_) hφ₀ hcm hφ hT hat
      exact .unw hℓ ((by simpa using ha : Typ S _ _ [] _ (.ex _ _)).cut hΘ.entails)
          (Stage.none_le (Stage.rt_ne_pure hψ₀))
  | hkeep _ _ he _ _ _ _ hφ hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | hkeepVal hv =>
      exact (he.valueAny hv _).sub ⟨hT, Stage.pure_le _⟩
  | hkarm hℓ hal hop hE hk hp hφ₀ hc hnt ha hle hφ hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | hkarmVal hE' hk' hv =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hE' hE (hk'.symm.trans hk)
      refine .hkeep hℓ hal (refill_app hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ _ hb _ => ?_) hφ₀ hc hnt hle hφ hT
      exact ((ha.valueAny hv φ').sub (J' := .ex T φ')
        ⟨hb (by decide), Stage.le_refl _⟩).cut hΘ.entails
  | hres _ hb hA ha hp hl hcp hφ hT =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | hresVal hA' ha' hv =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hA' hA (ha'.symm.trans ha)
      have h₁ := refill_app hA kont_not_value hp (kont_fn hl) (fun _ _ h l => h.trans l)
        fun T φ' hΘ _ hb' _ =>
          ((hb.valueAny hv φ').sub (J' := .ex T φ') ⟨hb', Stage.le_refl _⟩).cut hΘ.entails
      exact .hdone (h₁.sub ⟨hT, Stage.use_mono hφ (Caps.Sub.refl _)⟩)
  | hdone ha =>
    intro t φ e' hΓ hJ hh
    cases hΓ; cases hJ
    cases hh with
    | hdoneVal hv =>
      exact ha.valueAny hv _
  | _ => intro _ _ _ _ _ hh; cases hh

theorem pure_preservation {Ψ Θ e t φ e'} (h : Typ S Ψ Θ [] e (.ex t φ))
    (hh : PHead e e') : Typ S Ψ Θ [] e' (.ex t φ) :=
  h.pureStepCore t φ e' rfl rfl hh

end Flick
