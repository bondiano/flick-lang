import Formal.Context

/-!
# Preservation (core.md §4, stage P1a)
-/

namespace Flick

variable {S : Sig}

theorem exh_bool : Exh S (.base .bool) [.lit (.bool true), .lit (.bool false)] := by
  intro v hs
  have : ∀ {fl T}, Shp S fl T v → T = .base .bool → fl = true → ∃ b, v = .lit (.bool b) := by
    intro fl T h hT hf
    cases h with
    | @lit l => cases l <;> simp [Lit.base] at hT; exact ⟨_, rfl⟩
    | head => cases hf
    | app => cases hf
    | arrow => cases hT
    | ref => cases hT
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

theorem StoreTy.weaken {Ψ μ} (h : StoreTy S Ψ μ) (X : List Ty) {i : Nat} {v t}
    (hv : μ[i]? = some v) (ht : Ψ[i]? = some t) : Value v ∧ Typ S (Ψ ++ X) [] v (.ex t) :=
  let ⟨a, b⟩ := h.2 i v t hv ht
  ⟨a, b.weakenStore X⟩

theorem StoreTy.alloc {Ψ μ v t₀} (hμ : StoreTy S Ψ μ) (hv : Value v)
    (h : Typ S Ψ [] v (.ex t₀)) : StoreTy S (Ψ ++ [t₀]) (μ ++ [v]) := by
  refine ⟨by simp [hμ.1], fun i w s hw hs => ?_⟩
  by_cases hi : i < μ.length
  · rw [List.getElem?_append_left hi] at hw
    rw [List.getElem?_append_left (hμ.1 ▸ hi)] at hs
    exact hμ.weaken [t₀] hw hs
  · rw [List.getElem?_append_right (by omega)] at hw
    rw [List.getElem?_append_right (by rw [← hμ.1]; omega)] at hs
    rw [hμ.1] at hw
    cases hj : i - Ψ.length with
    | zero => simp [hj] at hw hs; subst hw hs; exact ⟨hv, h.weakenStore _⟩
    | succ => simp [hj] at hw

theorem head_preservation {Ψ μ r t μ' r'} (h : Typ S Ψ [] r (.ex t)) (hμ : StoreTy S Ψ μ)
    (hh : Head μ r μ' r') : ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) [] r' (.ex t) := by
  have same : ∀ {e'}, Typ S Ψ [] e' (.ex t) →
      ∃ X, StoreTy S (Ψ ++ X) μ ∧ Typ S (Ψ ++ X) [] e' (.ex t) :=
    fun h' => ⟨[], by simpa using hμ, by simpa using h'⟩
  cases hh with
  | beta hv =>
    cases h with
    | app h₁ h₂ => cases h₁ with | lam hb => exact same (hb.inst (Δ := [_]) (.cons h₂ .nil))
  | fix hv =>
    cases h with
    | app h₁ h₂ =>
      cases h₁ with
      | fix hb => exact same (hb.inst (Δ := [_, _]) (.cons h₂ (.cons (.fix hb) .nil)))
  | let_ hv =>
    cases h with | let_ h₁ h₂ => exact same (h₂.inst (Δ := [_]) (.cons h₁ .nil))
  | letrec =>
    cases h with | letrec h₁ h₂ => exact same (h₂.inst (Δ := [_]) (.cons (.fix h₁) .nil))
  | proj hv hw => cases h with | proj h₁ hR => exact same (field_typ h₁ hw hR)
  | update hv hw hv' =>
    cases h with | update h₁ hR h₂ => exact same (setField_typ h₂ h₁ hv' hR)
  | matchYes hv hm =>
    cases h with
    | match_ h₁ hA _ =>
      cases hA with
      | acons hp hb _ =>
        obtain ⟨_, hδ⟩ := hp.typ_match h₁ hm (fun _ => rfl)
        exact same (hb.inst hδ)
  | matchNo hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | acons _ _ hr => exact same (.match_ h₁ hr (hc.tail hv (hv.shape h₁).1 hm))
  | guardYes hv hm =>
    cases h with
    | match_ h₁ hA hc =>
      cases hA with
      | aguard hp hg hb hr =>
        obtain ⟨_, hδ⟩ := hp.typ_match h₁ hm (fun _ => rfl)
        refine same (.match_ (hg.inst hδ) (.acons .lit (hb.inst hδ)
          (.acons .lit (.match_ h₁ hr hc) .anil)) (.inl exh_bool))
  | guardNo hv hm =>
    cases h with
    | match_ h₁ hA hc => cases hA with | aguard _ _ _ hr => exact same (.match_ h₁ hr hc)
  | refMake hv =>
    cases h with
    | app h₁ h₂ =>
      have hty := h₁.const_inv
      simp only [Const.ty, Ty.arrow.injEq] at hty
      obtain ⟨rfl, rfl⟩ := hty
      exact ⟨_, hμ.alloc hv h₂, .loc (by simp [hμ.1])⟩
  | refGet hl =>
    cases h with
    | app h₁ h₂ =>
      have hty := h₁.const_inv
      simp only [Const.ty, Ty.arrow.injEq] at hty
      obtain ⟨rfl, rfl⟩ := hty
      cases h₂ with
      | loc hl' => exact same (hμ.2 _ _ _ hl hl').2
  | @refSet v l _ t₀ hv hlt =>
    cases h with
    | app h₁ h₂ =>
      cases h₁ with
      | app h₃ h₄ =>
        have hty := h₃.const_inv
        simp only [Const.ty, Ty.arrow.injEq] at hty
        obtain ⟨rfl, rfl, rfl⟩ := hty
        cases h₄ with
        | loc hl =>
          refine ⟨[], ⟨by simp [hμ.1], fun i w s hw hs => ?_⟩, by simpa using .lit⟩
          simp only [List.append_nil] at hs ⊢
          by_cases hi : l = i
          · subst hi
            rw [List.getElem?_set_self hlt] at hw
            cases hw; rw [hl] at hs; cases hs; exact ⟨hv, h₂⟩
          · rw [List.getElem?_set_ne hi] at hw
            exact hμ.2 i w s hw hs

/-- Preservation: a step keeps the type and grows the store typing. -/
theorem preservation {Ψ μ e t μ' e'} (h : Typ S Ψ [] e (.ex t)) (hμ : StoreTy S Ψ μ)
    (hs : Step μ e (.cfg μ' e')) : ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) [] e' (.ex t) := by
  cases hs with
  | head _ hh =>
    obtain ⟨_, hr, back⟩ := ctx_typing _ hh.not_value h
    obtain ⟨X, hμ', hr'⟩ := head_preservation hr hμ hh
    exact ⟨X, hμ', back X _ hr'⟩

end Flick
