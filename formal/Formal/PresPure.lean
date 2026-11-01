import Formal.ProgressMain

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
    | hidden ho => subst hT; cases ho
    | head => cases hf
    | app => cases hf
    | rnil => cases hT
    | rcons => cases hT
    | adt => cases hT
  obtain ⟨b, rfl⟩ := this hs rfl rfl
  cases b <;> simp [matchP]

/-- After a failed arm, the remaining arms still cover the value (Р29). -/
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
theorem guard_typ {Ψ g b rest t' φ} (hg : Typ S Ψ [] g (.ex (.base .bool) φ))
    (hb : Typ S Ψ [] b (.ex t' φ)) (hr : Typ S Ψ [] rest (.ex t' φ)) :
    Typ S Ψ [] (.match_ g (.acons (.lit (.bool true)) b
      (.acons (.lit (.bool false)) rest .anil))) (.ex t' φ) :=
  .match_ hg (.acons .lit hb (.acons .lit hr .anil)) (.inl exh_bool)

theorem pure_preservation {Ψ e t φ e'} (h : Typ S Ψ [] e (.ex t φ)) (hh : PHead e e') :
    Typ S Ψ [] e' (.ex t φ) := by
  have nv := hh.not_value
  cases hh with
  | beta hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | lam hb hT' =>
        obtain ⟨_, _, _, h, la, lψ, lt⟩ := hT'.arrow_left
        cases h
        exact (hb.inst (Δ := [_]) (.cons ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩) .nil)).sub
          ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | errVal hv' => exact (nv hv').elim
  | fix hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | fix hb hT' =>
        obtain ⟨_, _, _, h, la, lψ, lt⟩ := hT'.arrow_left
        cases h
        exact (hb.inst (Δ := [_, _]) (.cons ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩)
          (.cons (.fix hb .refl) .nil))).sub ⟨lt.trans hT, Stage.le_trans lψ hψ⟩
    | errVal hv' => exact (nv hv').elim
  | let_ hv =>
    cases h with
    | let_ h₁ h₂ => exact h₂.inst (Δ := [_]) (.cons (h₁.valueAny hv _) .nil)
    | errVal hv'' => exact (nv hv'').elim
  | letrec =>
    cases h with
    | letrec h₁ h₂ => exact h₂.inst (Δ := [_]) (.cons (.fix h₁ .refl) .nil)
    | errVal hv'' => exact (nv hv'').elim
  | proj _ hw =>
    cases h with
    | proj h₁ hR hT => exact (field_typ h₁ hw hR).sub ⟨hT, Stage.le_refl _⟩
    | errVal hv'' => exact (nv hv'').elim
  | update _ _ hv' =>
    cases h with
    | update h₁ hR h₂ => exact setField_typ h₂ h₁ hv' hR
    | errVal hv'' => exact (nv hv'').elim
  | matchYes hv hm =>
    cases h with
    | match_ h₁ hA _ =>
      cases hA with
      | acons hp hb _ => exact hb.inst (hp.inst (h₁.valueAny hv _) hm)
    | errVal hv'' => exact (nv hv'').elim
  | matchNo hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | acons _ _ hr => exact .match_ h₁ hr (hc.tail hv (h₁.shape hv rfl).1 hm)
    | errVal hv'' => exact (nv hv'').elim
  | guardYes hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | aguard hp hg hb hr =>
        have hδ := hp.inst (h₁.valueAny hv _) hm
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
    | bind h₁ hρ => exact .econs hv .enil h₁ (.enil fun _ h => h) fun n h => .inl (hρ n h)
    | errVal hv'' => exact (nv hv'').elim
  | join h₁ h₂ =>
    cases h with
    | join t₁ t₂ hρ => exact env_append_typ t₂ h₂ h₁ t₁ hρ
    | errVal hv'' => exact (nv hv'').elim

end Flick
