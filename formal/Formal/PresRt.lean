import Formal.PresPure

/-!
# Preservation of runtime steps at the redex
-/

namespace Flick

variable {S : Sig}

/-- The error type of a raised value. -/
theorem err_type {Ψ v ε} (hv : Value v) (h : Typ S Ψ [] v (.ex (.errors ε) .pure)) {K ts T}
    (hK : v.head = .ctor K) (hS : S.ctor K = some (ts, T)) :
    ε T ∧ Typ S Ψ [] v (.ex (.adt T) .pure) := by
  obtain ⟨K', ts', T', hK', hS', hε, ht⟩ := canonical_errors hv h
  rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'
  exact ⟨hε, ht⟩

/-- After a failed `try` arm, the remaining arms still remove or let through the
error of the value. -/
theorem HCov.tail {Ψ ε₀ p θ Δ v H H' ε'} (hp : HPat S ε₀ p θ Δ) (hv : Value v)
    (hv' : Typ S Ψ [] v (.ex (.errors ε₀) .pure)) (hc : HCov S v H ε')
    (hH : ∀ n, H n → Catches S ε₀ p θ n ∨ H' n) (hm : matchP p v = none) : HCov S v H' ε' := by
  intro K ts T hK hS
  rcases hc K ts T hK hS with h | h
  · rcases hH T h with (⟨rfl, _⟩ | ⟨rfl, hx⟩) | h'
    · cases hp <;> simp [matchP] at hm
    · obtain ⟨_, ht⟩ := err_type hv hv' hK hS
      obtain ⟨q, hq, hqm⟩ := hx v (ht.shape hv rfl).1
      simp at hq; subst hq; simp [hm] at hqm
    · exact .inl h'
  · exact .inr h

/-- A raised value of a body with errors `ε₀`, at `Errors ε₀`. -/
theorem raise_inv {Ψ v t φ} (hv : Value v) (h : Typ S Ψ [] (.raise v) (.ex t φ)) :
    ∃ ε, Typ S Ψ [] v (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε).le φ := by
  cases h with
  | raise h₁ hφ => exact ⟨_, h₁.valueAny hv _, hφ⟩
  | raiseAdt h₁ hφ => exact ⟨Row.single _, .errVal hv (h₁.valueAny hv _) rfl, hφ⟩
  | errVal hv' => cases hv'

theorem StoreTy.alloc {Ψ μ v t₀} (hμ : StoreTy S Ψ μ) (hv : Value v)
    (h : Typ S Ψ [] v (.ex t₀ .pure)) : StoreTy S (Ψ ++ [t₀]) (μ ++ [v]) := by
  refine ⟨by simp [hμ.1], fun i w s hw hs => ?_⟩
  by_cases hi : i < μ.length
  · rw [List.getElem?_append_left hi] at hw
    rw [List.getElem?_append_left (hμ.1 ▸ hi)] at hs
    obtain ⟨a, b⟩ := hμ.2 i w s hw hs
    exact ⟨a, b.weakenStore _⟩
  · rw [List.getElem?_append_right (by omega)] at hw
    rw [List.getElem?_append_right (by rw [← hμ.1]; omega)] at hs
    rw [hμ.1] at hw
    cases hj : i - Ψ.length with
    | zero => simp [hj] at hw hs; subst hw hs; exact ⟨hv, h.weakenStore _⟩
    | succ => simp [hj] at hw

theorem StoreTy.set {Ψ μ l v t₀} (hμ : StoreTy S Ψ μ) (hl : Ψ[l]? = some t₀)
    (hlt : l < μ.length) (hv : Value v) (h : Typ S Ψ [] v (.ex t₀ .pure)) :
    StoreTy S Ψ (μ.set l v) := by
  refine ⟨by simp [hμ.1], fun i w s hw hs => ?_⟩
  by_cases hi : l = i
  · subst hi
    rw [List.getElem?_set_self hlt] at hw
    cases hw; rw [hl] at hs; cases hs; exact ⟨hv, h⟩
  · rw [List.getElem?_set_ne hi] at hw
    exact hμ.2 i w s hw hs

theorem Stage.rt_sub {r r' e e' : Row} (hr : Row.Sub r r') (he : Row.Sub e e') :
    (Stage.rt r e).le (.rt r' e') := ⟨hr, he⟩

theorem Stage.rt_union_left {i r e ε : Row} :
    (Stage.rt i e).le (.rt (Row.union i r) (Row.union e ε)) :=
  ⟨fun _ h => .inl h, fun _ h => .inl h⟩

theorem Stage.rt_union_right {i r e ε : Row} :
    (Stage.rt r ε).le (.rt (Row.union i r) (Row.union e ε)) :=
  ⟨fun _ h => .inr h, fun _ h => .inr h⟩

theorem rt_preservation {Ψ μ e t φ μ' e'} (h : Typ S Ψ [] e (.ex t φ)) (hμ : StoreTy S Ψ μ)
    (hh : RHead μ e μ' e') : ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) [] e' (.ex t φ) := by
  have nv := hh.not_value
  have same : ∀ {e'}, Typ S Ψ [] e' (.ex t φ) →
      ∃ X, StoreTy S (Ψ ++ X) μ ∧ Typ S (Ψ ++ X) [] e' (.ex t φ) :=
    fun h' => ⟨[], by simpa using hμ, by simpa using h'⟩
  cases hh with
  | refMake hv =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, h, la, _, lt⟩ := hT'.arrow_left
        cases h; cases lt; cases hT
        exact ⟨_, hμ.alloc hv ((ha.valueAny hv _).sub ⟨la, Stage.le_refl _⟩),
          .loc (by simp [hμ.1])⟩
    | errVal hv' => exact (nv hv').elim
  | refGet hl =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, h, la, _, lt⟩ := hT'.arrow_left
        cases h; cases la
        cases ha with
        | loc hl' => exact same (((hμ.2 _ _ _ hl hl').2).sub ⟨lt.trans hT, Stage.pure_le _⟩)
    | errVal hv' => exact (nv hv').elim
  | refSet hv hlt =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | app hc hloc _ hT₁ =>
        cases hc with
        | const _ hT' =>
          obtain ⟨_, _, _, h, la, _, lt⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨_, _, _, h, la', _, lt'⟩ := (lt.trans hT₁).arrow_left
          cases h; cases lt'; cases hT
          cases hloc with
          | loc hl =>
            have hst := hμ.set hl hlt hv ((ha.valueAny hv _).sub ⟨la', Stage.le_refl _⟩)
            exact ⟨[], by simpa using hst, by simpa using .lit⟩
    | errVal hv' => exact (nv hv').elim
  | await =>
    cases h with
    | await he hφ hT =>
      cases he with
      | task he' hψ hT' =>
        obtain ⟨_, _, _, h, hr, hε, lt⟩ := hT'.eff_left
        cases h
        exact same (he'.sub ⟨lt.trans hT, Stage.le_trans (Stage.le_trans hψ (Stage.rt_sub hr hε)) hφ⟩)
    | errVal hv'' => exact (nv hv'').elim
  | build hv hb =>
    cases h with
    | provideL hl he hb' hφ =>
      obtain ⟨_, hb₂, tb⟩ := canonical_layer hv hl
      rw [hb] at hb₂; cases hb₂
      exact same (.provideE (tb.sub ⟨.refl, Stage.le_trans Stage.rt_union_left hφ⟩)
        he hb' (Stage.le_trans Stage.rt_union_right hφ))
    | provideE hl =>
      cases canonical_env hv hl <;> simp [Tm.build] at hb
    | errVal hv'' => exact (nv hv'').elim
  | provideEnv hθ =>
    cases h with
    | provideL hl =>
      obtain ⟨_, hb, -⟩ := canonical_layer hθ.value hl
      cases hθ <;> simp [Tm.build] at hb
    | provideE hl he hb hφ => exact same (.with_ hθ.value (hl.valueAny hθ.value _) he hb hφ)
    | errVal hv'' => exact (nv hv'').elim
  | tryRaise hv =>
    cases h with
    | tryR hb hA hφ hs =>
      obtain ⟨ε, hv', hε⟩ := raise_inv hv hb
      obtain ⟨K, ts, T, hK, hS, hT, ht⟩ := canonical_errors hv hv'
      obtain ⟨_, _, rfl, _, hε'⟩ := Stage.rt_le hφ
      refine same (.handle hv (.errVal hv ht (hε.2 T hT)) hA (fun K' ts' T' hK' hS' => ?_)
        ⟨fun _ h => h.elim, hε'⟩)
      rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'; exact hs T (hε.2 T hT)
    | tryP hb =>
      obtain ⟨_, _, hε⟩ := raise_inv hv hb; exact hε.elim
    | errVal hv'' => exact (nv hv'').elim
  | handleYes hv hm =>
    cases h with
    | handle _ hv' hA _ _ =>
      cases hA with
      | hcons hp hb _ _ => exact same (hb.inst (hp.inst hv hv' hm).1)
    | errVal hv'' => exact (nv hv'').elim
  | handleNo hv hm =>
    cases h with
    | handle _ hv' hA hc hφ =>
      cases hA with
      | hcons hp _ hr hH => exact same (.handle hv hv' hr (hc.tail hp hv hv' hH hm) hφ)
    | errVal hv'' => exact (nv hv'').elim
  | hguardYes hv hm =>
    cases h with
    | handle _ hv' hA hc hφ =>
      cases hA with
      | hguard hp hg hb hr hH =>
        have hδ := (hp.inst hv hv' hm).1
        exact same (guard_typ (hg.inst hδ) (hb.inst hδ)
          (.handle hv hv' hr (fun K ts T hK hS => (hc K ts T hK hS).imp_left (hH T)) hφ))
    | errVal hv'' => exact (nv hv'').elim
  | hguardNo hv hm =>
    cases h with
    | handle _ hv' hA hc hφ =>
      cases hA with
      | hguard _ _ _ hr hH =>
        exact same (.handle hv hv' hr (fun K ts T hK hS => (hc K ts T hK hS).imp_left (hH T)) hφ)
    | errVal hv'' => exact (nv hv'').elim
  | handleNil hv =>
    cases h with
    | handle _ hv' hA hc hφ =>
      cases hA with
      | hnil hH =>
        obtain ⟨K, ts, T, hK, hS, _, ht⟩ := canonical_errors hv hv'
        rcases hc K ts T hK hS with h | h
        · exact (hH T h).elim
        · exact same (.raise (.errVal hv (ht.valueAny hv _) h) hφ)
    | errVal hv'' => exact (nv hv'').elim

end Flick
