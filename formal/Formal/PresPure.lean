import Formal.ProgressMain
import Formal.SpineMatch

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

theorem pure_preservation {Ψ Θ e t φ e'} (h : Typ S Ψ Θ [] e (.ex t φ)) (hh : PHead e e') :
    Typ S Ψ Θ [] e' (.ex t φ) := by
  have nv := hh.not_value
  cases hh with
  | beta hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | lam hb _ _ hT' =>
        obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
        cases h
        exact (hb.inst (Δ := [_]) (.cons hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩) .nil)).sub
          ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | errVal hv' => exact (nv hv').elim
  | fix hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | fix hb hn hc hT' =>
        obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
        cases h
        exact (hb.inst (Δ := [_, _]) (.cons hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩)
          (.cons .fix (.fix hb hn hc .refl) .nil))).sub ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | errVal hv' => exact (nv hv').elim
  | let_ hv =>
    cases h with
    | let_ h₁ h₂ => exact h₂.inst (Δ := [_]) (.cons hv (h₁.valueAny hv _) .nil)
    | letGen h₁ h₂ => exact h₂.subst (SubOk.one hv h₁)
    | errVal hv'' => exact (nv hv'').elim
  | letrec =>
    cases h with
    | letrec h₁ hn hc h₂ => exact h₂.inst (Δ := [_]) (.cons .fix (.fix h₁ hn hc .refl) .nil)
    | errVal hv'' => exact (nv hv'').elim
  | proj _ hw =>
    cases h with
    | proj h₁ hR hT => exact (field_typ h₁ hw hR).sub ⟨hT, Stage.le_refl _⟩
    | dproj h₁ hc hk hl hts hH hT =>
      exact (dict_field ‹_› h₁ hc hl hts hH hw).sub ⟨hT, Stage.pure_le _⟩
    | errVal hv'' => exact (nv hv'').elim
  | update _ _ hv' =>
    cases h with
    | update h₁ hR h₂ => exact setField_typ h₂ h₁ hv' hR
    | errVal hv'' => exact (nv hv'').elim
  | matchYes hv hm =>
    cases h with
    | match_ h₁ hA _ =>
      cases hA with
      | acons hp hb _ => exact hb.inst (hp.inst hv (h₁.valueAny hv _) hm)
      | aref hK hU hn _ _ hpA hbA _ => exact aref_match hv h₁ hK hU hn hpA hbA hm
    | errVal hv'' => exact (nv hv'').elim
  | matchNo hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | acons _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
      | aref _ _ _ _ _ _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
    | errVal hv'' => exact (nv hv'').elim
  | guardYes hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | aguard hp hg hb hr =>
        have hδ := hp.inst hv (h₁.valueAny hv _) hm
        exact guard_typ (hg.inst hδ) (hb.inst hδ) (.match_ h₁ hr hc)
    | errVal hv'' => exact (nv hv'').elim
  | guardNo hv hm =>
    cases h with
    | match_ h₁ hA hc => cases hA with | aguard _ _ _ hr => exact .match_ h₁ hr hc
    | errVal hv'' => exact (nv hv'').elim
  | tryVal hv =>
    cases h with
    | tryR h₁ => exact h₁.valueAny hv _
    | tryP h₁ => exact h₁.valueAny hv _
    | errVal hv'' => exact (nv hv'').elim
  | withVal hv =>
    cases h with
    | with_ _ _ h₂ => exact h₂.valueAny hv _
    | errVal hv'' => exact (nv hv'').elim
  | bind hv =>
    cases h with
    | bind h₁ hρ => exact .econs hv .enil h₁ (.enil fun _ h => h) fun n => (hρ n).trans ⟨.inl, fun h => h.resolve_right id⟩
    | errVal hv'' => exact (nv hv'').elim
  | join h₁ h₂ =>
    cases h with
    | join t₁ t₂ hρ => exact env_append_typ t₂ h₂ h₁ t₁ hρ
    | errVal hv'' => exact (nv hv'').elim
  | intEq =>
    cases h with
    | app hf _ _ hT =>
      cases hf with
      | app hc _ _ hT' =>
        cases hc with
        | const _ hT'' =>
          obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT''.arrow_left
          cases h
          obtain ⟨_, _, _, _, h, _, _, lt', -⟩ := (lt.trans hT').arrow_left
          cases h; cases lt'; cases hT; exact .lit
    | errVal hv'' => exact (nv hv'').elim

end Flick
