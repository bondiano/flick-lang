import Formal.Elab.CoreCell
import Formal.Elab.CoreDictSem

/-!
# λFlick⁺ as a `Target`: pure head steps of related terms

A pure head step of a term related by a congruence rule is matched by the same rule on the
other side, with related results (`phead_sim`). The other side needs no step before it: the
values at the redex are related to values (`Cg.value`). A base pair at the redex itself is
left to the caller; the runtime head steps are in `CoreHead`.
-/

namespace Flick.Elab

private theorem not_alts_expr {S : Sig} {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ t φ, J = .ex t φ → IsAlts e → False := by
  induction h with
  | conv _ _ ih => intro t φ hJ ha; cases hJ; exact ih _ _ rfl ha
  | errVal _ _ _ ih => intro t φ hJ ha; cases hJ; exact ih _ _ rfl ha
  | funcRef _ _ ih => intro t φ hJ ha; cases hJ; exact ih _ _ rfl ha
  | allI hv => intro t φ _ ha; cases hv <;> simp [IsAlts] at ha
  | _ => intro t φ hJ ha; cases hJ <;> simp [IsAlts] at ha

/-- Lists of alternatives. -/
theorem DP.noAlts {S : Sig} {root : Roots} : BNoAlts (DP S root) := by
  rintro a b ⟨-, -, -, -, Θ, t, ht⟩ ha
  exact not_alts_expr ht _ _ rfl ha

variable {B : Flick.Tm → Flick.Tm → Prop}

theorem All2.mono {α β : Type} {R R' : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    (f : ∀ a b, R a b → R' a b) : All2 R' l l' := by
  induction h with
  | nil => exact .nil
  | cons r _ ih => exact .cons (f _ _ r) ih

theorem SubR.inst {vs ws : List Flick.Tm} (h : All2 (fun a b => ∀ x, Cg B x a b) vs ws) :
    SubR B (Flick.inst vs) (Flick.inst ws) := by
  intro i x
  unfold Flick.inst
  have hl := h.length
  by_cases hi : i < vs.length
  · rw [dif_pos hi, dif_pos (hl ▸ hi)]; exact h.get i hi (hl ▸ hi) x
  · rw [dif_neg hi, dif_neg (hl ▸ hi), hl]; exact .var

theorem Cg.setField {v v₂ w w₂ : Flick.Tm} (h : Cg B true v v₂) (hw : Cg B true w w₂) (l : Label) :
    ORel (Cg B true) (v.setField l w) (v₂.setField l w₂) := by
  induction v generalizing v₂ with
  | rcons l' e r _ ihr =>
    cases h with
    | rcons h₁ h₂ =>
      simp only [Flick.Tm.setField]
      split
      · exact .rcons hw h₂
      · have := ihr h₂
        revert this
        cases (_ : Flick.Tm).setField l w <;> cases (_ : Flick.Tm).setField l w₂ <;>
          simp [ORel] <;> exact fun h => .rcons h₁ h
  | _ => cases h <;> simp [Flick.Tm.setField, ORel]

/-- Matched values, related at either index. -/
theorem matchP_sub {p : Pat} {v v₂ : Flick.Tm} (h : Cg B true v v₂) {δ : List Flick.Tm}
    (hm : Flick.matchP p v = some δ) :
    ∃ δ₂, Flick.matchP p v₂ = some δ₂ ∧ SubR B (Flick.inst δ) (Flick.inst δ₂) := by
  obtain ⟨δ₂, h₂, hδ⟩ := (Cg.matchP (p := p) h).some_left hm
  refine ⟨δ₂, h₂, SubR.inst (hδ.mono fun a b r x => ?_)⟩
  cases x
  · exact r.lower
  · exact r

/-- **A pure head step of related terms.** -/
theorem phead_sim (hC : BClosed B) (hV : BNoVal B) (hA : BNoAlts B) (hP : BPlain B) {r r' r₂ : Flick.Tm}
    (hh : PHead r r') (h : Cg B false r r₂) : B r r₂ ∨ ∃ r₂', PHead r₂ r₂' ∧ Cg B false r' r₂' := by
  have nb : ∀ {a b : Flick.Tm}, B a b → Value a → False := fun hb hv => hV _ _ hb hv
  cases hh with
  | beta hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | lam hb =>
        obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₂
        exact .inr ⟨_, .beta hv₂, hb.subst hC _ _ (SubR.inst (.cons hy .nil))⟩
      | base hb => exact (nb hb .lam).elim
  | fix hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | fix hb =>
        obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₂
        exact .inr ⟨_, .fix hv₂, hb.subst hC _ _
          (SubR.inst (.cons hy (.cons (fun _ => .fix hb) .nil)))⟩
      | base hb => exact (nb hb .fix).elim
  | let_ hv =>
    cases h with
    | base hb => exact .inl hb
    | let_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      exact .inr ⟨_, .let_ hv₂, h₂.subst hC _ _ (SubR.inst (.cons hy .nil))⟩
  | letrec =>
    cases h with
    | base hb => exact .inl hb
    | letrec h₁ h₂ =>
      exact .inr ⟨_, .letrec, h₂.subst hC _ _ (SubR.inst (.cons (fun _ => .fix h₁) .nil))⟩
  | proj hv hf =>
    cases h with
    | base hb => exact .inl hb
    | proj h₁ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      obtain ⟨w₂, hw₂, hw⟩ := ((hy true).field _).some_left hf
      exact .inr ⟨_, .proj hv₂ hw₂, hw.lower⟩
  | update hv hw hs =>
    cases h with
    | base hb => exact .inl hb
    | update h₁ h₂ =>
      obtain ⟨hv₂, -, hy₁⟩ := Cg.value hV hv h₁
      obtain ⟨hw₂, -, hy₂⟩ := Cg.value hV hw h₂
      obtain ⟨v₂', hs₂, hr⟩ := ((hy₁ true).setField (hy₂ true) _).some_left hs
      exact .inr ⟨_, .update hv₂ hw₂ hs₂, hr.lower⟩
  | matchYes hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | acons hb _ =>
        obtain ⟨δ₂, hm₂, hσ⟩ := matchP_sub (hy true) hm
        exact .inr ⟨_, .matchYes hv₂ hm₂, hb.subst hC _ _ hσ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | matchNo hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | acons _ hr =>
        exact .inr ⟨_, .matchNo hv₂ ((Cg.matchP (hy true)).none_left hm), .match_ h₁ hr⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | guardYes hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | aguard hg hb hr =>
        obtain ⟨δ₂, hm₂, hσ⟩ := matchP_sub (hy true) hm
        exact .inr ⟨_, .guardYes hv₂ hm₂, .match_ (hg.subst hC _ _ hσ)
          (.acons (hb.subst hC _ _ hσ) (.acons (.match_ h₁ hr) .anil))⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | guardNo hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | aguard _ _ hr =>
        exact .inr ⟨_, .guardNo hv₂ ((Cg.matchP (hy true)).none_left hm), .match_ h₁ hr⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | tryVal hv =>
    cases h with
    | base hb => exact .inl hb
    | try_ h₁ _ => exact .inr ⟨_, .tryVal (Cg.value hV hv h₁).1, h₁⟩
  | hwithVal hv =>
    cases h with
    | base hb => exact .inl hb
    | hwith _ h₂ h₃ => exact .inr ⟨_, .hwithVal (Cg.value hV hv h₃).1, .app h₂ h₃⟩
  | hkeepVal hv =>
    cases h with
    | base hb => exact .inl hb
    | hkeep _ h₂ => exact .inr ⟨_, .hkeepVal (Cg.value hV hv h₂).1, h₂⟩
  | hkarmVal hE hk hv =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hkarm h₁ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      exact .inr ⟨_, .hkarmVal (FrR.okE hV hfr hE) rfl (Cg.value hV hv h₃).1,
        .hkeep h₁ (FrR.plugs hfr ((Cg.value hV hv h₃).2.2 false))⟩
  | hctlTail hE hk hv =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hctl h₁ h₂ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      exact .inr ⟨_, .hctlTail (FrR.okE hV hfr hE) rfl (Cg.value hV hv h₃).1,
        .hwith h₁ h₂ (FrR.plugs hfr ((Cg.value hV hv h₃).2.2 false))⟩
  | hctlDisc hE hk hv =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hctl h₁ h₂ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      exact .inr ⟨_, .hctlDisc (FrR.okE hV hfr hE) rfl (Cg.value hV hv h₃).1,
        .hwith h₁ h₂ (FrR.plugs hfr (.unw h₃))⟩
  | hresVal hA ha hv =>
    subst ha
    cases h with
    | base hb => exact .inl hb
    | hres h₁ h₂ =>
      obtain ⟨A₂, rfl, hfr⟩ := Cg.plug_hole hP h₁
      exact .inr ⟨_, .hresVal (FrR.okE hV hfr hA) rfl (Cg.value hV hv h₂).1,
        .hdone (FrR.plugs hfr ((Cg.value hV hv h₂).2.2 false))⟩
  | hdoneVal hv =>
    cases h with
    | base hb => exact .inl hb
    | hdone h₁ => exact .inr ⟨_, .hdoneVal (Cg.value hV hv h₁).1, h₁⟩
  | funcRef hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | base hb => exact (nb hb .const).elim
      | const => exact .inr ⟨_, .funcRef (Cg.value hV hv h₂).1, h₂⟩
  | intEq =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₂ with
      | base hb => exact (nb hb .lit).elim
      | lit =>
        cases h₁ with
        | base hb => exact (nb hb (.constApp rfl .lit)).elim
        | app h₃ h₄ =>
          cases h₃ with
          | base hb => exact (nb hb .const).elim
          | const =>
            cases h₄ with
            | base hb => exact (nb hb .lit).elim
            | lit => exact .inr ⟨_, .intEq, .lit⟩

end Flick.Elab
