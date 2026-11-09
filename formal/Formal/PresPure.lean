import Formal.ProgressMain
import Formal.SpineMatch
import Formal.PresHdl

/-!
# Preservation of pure steps
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

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
theorem guard_typ_mode {Ψ Θ g b rest t' φ} (hg : TypM S mode Ψ Θ [] g (.ex (.base .bool) φ))
    (hb : TypM S mode Ψ Θ [] b (.ex t' φ)) (hr : TypM S mode Ψ Θ [] rest (.ex t' φ)) :
    TypM S mode Ψ Θ [] (.match_ g (.acons (.lit (.bool true)) b
      (.acons (.lit (.bool false)) rest .anil))) (.ex t' φ) :=
  .match_ hg (.acons .lit hb (.acons .lit hr .anil)) (.inl exh_bool)

theorem guard_typ {Ψ Θ g b rest t' φ} (hg : Typ S Ψ Θ [] g (.ex (.base .bool) φ))
    (hb : Typ S Ψ Θ [] b (.ex t' φ)) (hr : Typ S Ψ Θ [] rest (.ex t' φ)) :
    Typ S Ψ Θ [] (.match_ g (.acons (.lit (.bool true)) b
      (.acons (.lit (.bool false)) rest .anil))) (.ex t' φ) :=
  guard_typ_mode hg hb hr

theorem pure_preservation_mode {Ψ Θ e t φ e'} (h : TypM S mode Ψ Θ [] e (.ex t φ))
    (hh : PHead e e') : TypM S mode Ψ Θ [] e' (.ex t φ) := by
  have nv := hh.not_value
  cases hh with
  | beta hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | lam hb _ _ hT' =>
        obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := TypingMode.subtype.arrow_left hT'
        cases h
        exact (hb.inst (Δ := [_]) (.cons hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩) .nil)).sub
          ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | errVal hv' | allI hv' => exact (nv hv').elim
  | fix hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | fix hb hn hc hT' =>
        obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := TypingMode.subtype.arrow_left hT'
        cases h
        exact (hb.inst (Δ := [_, _]) (.cons hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩)
          (.cons .fix (.fix hb hn hc (TypingMode.subtype.refl)) .nil))).sub
          ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | errVal hv' | allI hv' => exact (nv hv').elim
  | let_ hv =>
    cases h with
    | let_ h₁ h₂ => exact h₂.inst (Δ := [_]) (.cons hv (h₁.valueAny hv _) .nil)
    | letGen h₁ h₂ => exact h₂.subst (SubOkM.one hv h₁)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | letrec =>
    cases h with
    | letrec h₁ hn hc h₂ =>
      exact h₂.inst (Δ := [_]) (.cons .fix (.fix h₁ hn hc (TypingMode.subtype.refl)) .nil)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | proj _ hw =>
    cases h with
    | proj h₁ hR hT => exact (field_typ_mode h₁ hw hR).sub ⟨hT, Stage.le_refl _⟩
    | dproj h₁ hc hk hl hts hH hT =>
      exact (dict_field_mode ‹_› h₁ hc hl hts hH hw).sub ⟨hT, Stage.pure_le _⟩
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | update _ _ hv' =>
    cases h with
    | update h₁ hR h₂ => exact setField_typ_mode h₂ h₁ hv' hR
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | matchYes hv hm =>
    cases h with
    | match_ h₁ hA _ =>
      cases hA with
      | acons hp hb _ => exact hb.inst (hp.inst_mode hv (h₁.valueAny hv _) hm)
      | aref hK hU hn _ _ hpA hbA _ => exact aref_match_mode hv h₁ hK hU hn hpA hbA hm
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | matchNo hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | acons _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
      | aref _ _ _ _ _ _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | guardYes hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | aguard hp hg hb hr =>
        have hδ := hp.inst_mode hv (h₁.valueAny hv _) hm
        exact guard_typ_mode (hg.inst hδ) (hb.inst hδ) (.match_ h₁ hr hc)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | guardNo hv hm =>
    cases h with
    | match_ h₁ hA hc => cases hA with | aguard _ _ _ hr => exact .match_ h₁ hr hc
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | tryVal hv =>
    cases h with
    | tryR h₁ => exact h₁.valueAny hv _
    | tryP h₁ => exact h₁.valueAny hv _
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | intEq =>
    cases h with
    | app hf _ _ hT =>
      cases hf with
      | app hc _ _ hT' =>
        cases hc with
        | const _ hT'' =>
          obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := TypingMode.subtype.arrow_left hT''
          cases h
          obtain ⟨_, _, _, _, h, _, _, lt', -⟩ := TypingMode.subtype.arrow_left (lt.trans hT')
          cases h
          obtain rfl := TypingMode.subtype.eq_base_left lt'
          obtain rfl := TypingMode.subtype.eq_base_left hT
          exact .lit
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hwithVal hv =>
    cases h with
    | hwith _ _ hrf hψ he _ _ hφ hT =>
      exact .app (hrf.sub ⟨TypingMode.subtype.refl, hφ⟩) (he.valueAny hv _)
        (Stage.le_trans hψ hφ) hT
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hctlTail hE' hk' hv =>
    cases h with
    | hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha _ hcm hφ hT hat =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hE' hE (hk'.symm.trans hk)
      refine .hwith hℓ hal hrf hψ (refill_app_mode hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ _ hb _ => ?_) hφ₀ hcm hφ hT hat
      exact (((by simpa using ha : TypM S mode _ _ [] _ (.ex _ _)).valueAny hv φ').sub (J' := .ex T φ')
        ⟨hb hc, Stage.le_refl _⟩).cut hΘ.entails
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hkeepVal hv =>
    cases h with
    | hkeep _ _ he _ _ _ _ hφ hT => exact (he.valueAny hv _).sub ⟨hT, Stage.pure_le _⟩
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hkarmVal hE' hk' hv =>
    cases h with
    | hkarm hℓ hal hop hE hk hp hφ₀ hc hnt ha hle hφ hT =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hE' hE (hk'.symm.trans hk)
      refine .hkeep hℓ hal (refill_app_mode hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ _ hb _ => ?_) hφ₀ hc hnt hle hφ hT
      exact ((ha.valueAny hv φ').sub (J' := .ex T φ')
        ⟨hb (by decide), Stage.le_refl _⟩).cut hΘ.entails
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hctlDisc hE' hk' hv =>
    cases h with
    | hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha _ hcm hφ hT hat =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hE' hE (hk'.symm.trans hk)
      refine .hwith hℓ hal hrf hψ (refill_app_mode hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ hψ₀ _ _ => ?_) hφ₀ hcm hφ hT hat
      exact .unw hℓ ((by simpa using ha : TypM S mode _ _ [] _ (.ex _ _)).cut hΘ.entails)
          (Stage.none_le (Stage.rt_ne_pure hψ₀))
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hresVal hA' ha' hv =>
    cases h with
    | hres _ hb hA ha hp hl hcp hφ hT =>
      obtain ⟨rfl, -⟩ := plug_hole_inj hA' hA (ha'.symm.trans ha)
      have h₁ := refill_app_mode hA kont_not_value hp (kont_fn hl) (fun _ _ h l => h.trans l)
        fun T φ' hΘ _ hb' _ =>
          ((hb.valueAny hv φ').sub (J' := .ex T φ') ⟨hb', Stage.le_refl _⟩).cut hΘ.entails
      exact .hdone (h₁.sub ⟨hT, TypingMode.use_mono mode hφ (Caps.Sub.refl _)⟩)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hdoneVal hv =>
    cases h with
    | hdone ha => exact ha.valueAny hv φ
    | errVal hv'' | allI hv'' => exact (nv hv'').elim

theorem pure_preservation {Ψ Θ e t φ e'} (h : Typ S Ψ Θ [] e (.ex t φ)) (hh : PHead e e') :
    Typ S Ψ Θ [] e' (.ex t φ) :=
  pure_preservation_mode h hh

end Flick
