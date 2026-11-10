import Formal.Soundness

/-!
# What a live stage uses: region atoms of holes, redexes and children

The region atoms of the stage of a hole are those of the stage around it or of a frame
(`hole_atm`); a step that needs a region has its atom in the stage of its redex; a child or a
finalizer runs at a stage of atoms of its owner's ancestors (`kid_stage`).
-/

namespace Flick

variable {S : Sig}

theorem Stage.kill_atm {n x : Nat} {φ : Stage} (h : (φ.substT (kill n)).svc (.atm (.r x))) :
    φ.svc (.atm (.r x)) ∧ x < n := by
  cases φ with
  | pure => exact h.elim
  | rt r e =>
    simp only [Stage.substT, Stage.svc, Row.substT] at h
    rcases h with ⟨b, hb, hr⟩ | ⟨b, hb, hr⟩
    · cases b with
      | v i => simp [kill] at hr
      | r y =>
        by_cases hy : y < n
        · simp [kill, hy] at hr; subst hr; exact ⟨hb, hy⟩
        · simp [kill, hy] at hr; exact (hr : False).elim
    · cases b with
      | v i => simp [kill, Ty.asRow] at hr
      | r y =>
        by_cases hy : y < n <;> simp [kill, hy, Ty.asRow] at hr
        exact (hr : False).elim

theorem domEM_mem {n : Elem} :
    ∀ {E : List Frame}, domE E n → ∃ F ∈ E, Frame.provides F n
  | [], h => h.elim
  | F :: E, h => by
    rcases (domE_cons F E n).mp h with h | h
    · exact ⟨F, by simp, h⟩
    · obtain ⟨G, hG, h⟩ := domEM_mem h
      exact ⟨G, by simp [hG], h⟩

theorem Frame.provides_atm {F : Frame} {x : Nat} (h : Frame.provides F (.atm (.r x))) :
    F = .inside x ∨ F.rgn = some x := by
  rcases h with ⟨p, rfl, h⟩ | ⟨p, hp, h⟩
  · cases h; exact .inl rfl
  · cases h; exact .inr hp

/-- A region atom of the stage of a hole is one of the stage around, or a frame's. -/
theorem hole_atm {Ψ Θ E r t φ} (hE : OkE E) (hr : ¬ Value r)
    (h : Typ S Ψ Θ [] (plug E r) (.ex t φ)) :
    ∃ K Θ' t' ψ, Typ S (Ψ.map (Ty.up K)) Θ' [] r (.ex t' ψ) ∧ (ψ = .pure ∨ K = 0 ∧ Ext Θ Θ') ∧
      ∀ x, ψ.svc (.atm (.r x)) → φ.svc (.atm (.r x)) ∨ ∃ F ∈ E, F = .inside x ∨ F.rgn = some x := by
  obtain ⟨K, Θ', t', ψ, hr', rel, hp, -⟩ := ctx_typing _ hE hr h
  refine ⟨K, Θ', t', ψ, hr', hp, fun x hx => ?_⟩
  rcases rel.svc _ hx with h' | h'
  · exact .inl h'
  · obtain ⟨F, hF, hd⟩ := domEM_mem h'
    exact .inr ⟨F, hF, Frame.provides_atm hd⟩

/-- `open_ H` needs its region in the stage of its redex. -/
theorem open_atm {Ψ Θ p T ψ} (h : Typ S Ψ Θ [] (.app (.const .open_) (.own p)) (.ex T ψ)) :
    ψ.svc (.atm (.r p)) := by
  cases h with
  | app hf ho hψ _ =>
    cases hf with
    | const _ hT' =>
      obtain ⟨_, _, _, _, h, la, lφ, _, -⟩ := hT'.arrow_left
      cases h; cases LeLive.eq_owner la
      obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
      cases hq
      exact Stage.le_svc (Stage.le_trans lφ hψ) _ (by rw [hs]; exact Or.inl rfl)
  | errVal hv | allI hv => exact absurd hv (by intro hv; cases hv <;> simp_all [Const.arity, Tm.head])

/-- `op_H` needs its region in the stage of its redex. -/
theorem op_atm {Ψ Θ p T ψ} (h : Typ S Ψ Θ [] (.app (.const .op) (.cap p)) (.ex T ψ)) :
    ψ.svc (.atm (.r p)) := by
  cases h with
  | app hf ho hψ _ =>
    cases hf with
    | const _ hT' =>
      obtain ⟨_, _, _, _, h, la, lφ, _, -⟩ := hT'.arrow_left
      cases h; cases LeLive.eq_hnd la
      obtain ⟨q, hq, hs⟩ := canonical_hnd .cap ho
      cases hq
      exact Stage.le_svc (Stage.le_trans lφ hψ) _ (by rw [hs]; exact Or.inl rfl)
  | errVal hv | allI hv => exact absurd hv (by intro hv; cases hv <;> simp_all [Const.arity, Tm.head])

/-- Registering a finalizer or a child needs its owner in the stage of its redex. -/
theorem fin_atm {Ψ Θ c p w T ψ} (hc : c = .addFin ∨ c = .spawn)
    (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T ψ)) : ψ.svc (.atm (.r p)) := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | app hc' ho _ hT₁ =>
      cases hc' with
      | perform => rcases hc with h | h <;> cases h
      | const _ hT' =>
        rcases hc with rfl | rfl <;>
        · obtain ⟨_, _, _, _, h, la₀, _, lt, -⟩ := hT'.arrow_left
          cases h; cases LeLive.eq_owner la₀
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, _, lφ, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          exact Stage.le_svc (Stage.le_trans lφ hψ) _ (by rw [hs]; exact Or.inr rfl)
  | errVal hv' | allI hv' => exact (fin_not_value hc hv').elim

/-- The work a child or a finalizer of `p` is given runs at a stage whose region atoms are `p`
or older and in the stage of the registration or given by its frames, or are atoms of the arms
of a handler it keeps. -/
theorem kid_stage {Ψ Θ E c p w t φ b} (hE : OkE E) (hc : c = .addFin ∨ c = .spawn) (hv : Value w)
    (h : Typ S Ψ Θ [] (plug E (.app (.app (.const c) (.own p)) w)) (.ex t φ))
    (hΘ : Entails Θ Facts.age) (hA : AscE b E) :
    ∃ t' R' ε', Typ S Ψ Facts.age [] (withs p E (.await w)) (.ex t' (.rt R' ε')) ∧
      ∀ x, R' (.atm (.r x)) → (x ≤ p ∧ (φ.svc (.atm (.r x)) ∨ ∃ F ∈ E, F = .inside x ∨ F.rgn = some x)) ∨
        KeptAtm Ψ p E (.r x) := by
  obtain ⟨t', R', ε', hk, -, hat⟩ := withs_typing_atm True (fun a => a = .r p ∨ ∀ x, a = .r x → x < p)
    (fun h' hΘ' => let ⟨tx, R, εx, k₁, k₂, k₃⟩ := fin_await_atm h' hc hΘ' hv
      ⟨tx, R, εx, k₁, k₂, fun _ => k₃⟩) (fun h' => fin_not_pure h' hc) (fin_not_value hc) E hE h hΘ hA
  refine ⟨t', R', ε', hk, fun x hx => ?_⟩
  rcases hat trivial _ hx with ⟨ho, h'⟩ | hk'
  · refine .inl ⟨?_, h'.imp_right fun ⟨F, hF, hd⟩ => ⟨F, hF, Frame.provides_atm hd⟩⟩
    rcases ho with ho | ho
    · cases ho; exact Nat.le_refl _
    · exact Nat.le_of_lt (ho x rfl)
  · exact .inr hk'

/-- The cell of a handler just installed: its arms' row holds atoms of the stage around, of
the frames, or of the handler's own region. -/
theorem hwith_cell {Ψ Θ E n ℓ Ef al rf e₀ t φ} (hE : OkE E)
    (h : Typ S Ψ Θ [] (plug E (.inside n (.hwith n ℓ Ef al rf e₀))) (.ex t φ)) :
    ∃ r ε t', Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) ∧
      ∀ x, r (.atm (.r x)) → φ.svc (.atm (.r x)) ∨ x = n ∨ ∃ F ∈ E, F = .inside x ∨ F.rgn = some x := by
  have h' : Typ S Ψ Θ [] (plug (.inside n :: E) (.hwith n ℓ Ef al rf e₀)) (.ex t φ) := h
  obtain ⟨K, Θ', t', ψ, hr, hp, hat⟩ := hole_atm (OkE.cons' (F := .inside n) hE trivial) (fun hv => by cases hv) h'
  cases hr with
  | hwith hℓ _ _ _ _ _ _ hφ _ =>
    rcases hp with rfl | ⟨rfl, -⟩
    · exact absurd hφ (fun h => by simp [Stage.le] at h)
    · rw [List.map_up_zero] at hℓ
      refine ⟨_, _, _, hℓ, fun x hx => ?_⟩
      rcases hat x (Stage.le_svc hφ _ hx) with h₁ | ⟨F, hF, hd⟩
      · exact .inl h₁
      · rcases List.mem_cons.mp hF with rfl | hF
        · rcases hd with hd | hd
          · cases hd; exact .inr (.inl rfl)
          · simp [Frame.rgn] at hd
        · exact .inr (.inr ⟨F, hF, hd⟩)
  | errVal hv | allI hv => cases hv
end Flick
