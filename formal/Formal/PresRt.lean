import Formal.ScopeStep

/-!
# Preservation of runtime steps at the redex
-/

namespace Flick

variable {S : Sig}

/-- The error type of a raised value. -/
theorem err_type {Ψ Θ v ε} (hv : Value v) (h : Typ S Ψ Θ [] v (.ex (.errors ε) .pure)) {K k ts T}
    (hK : v.head = .ctor K) (hS : S.ctor K = some (k, ts, T)) :
    ∃ U, ε (.nm T) ∧ U.hd = .adt T ∧ Typ S Ψ Θ [] v (.ex U .pure) ∧ Row.Sub (Row.err T U) ε := by
  obtain ⟨K', k', ts', T', U, hK', hS', hε, hU, ht, hsub⟩ := canonical_errors hv h
  rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'
  exact ⟨U, hε, hU, ht, hsub⟩

/-- The arms of `try` remove names of errors, not what they reach. -/
theorem Typ.halts_no_atm {Ψ Θ Γ A J} (h : Typ S Ψ Θ Γ A J) :
    ∀ {ε₀ H t φ}, J = .halts ε₀ H t φ → ∀ a, ¬ H (.atm a) := by
  induction h with
  | hnil hH => intro _ _ _ _ hJ; cases hJ; exact fun a => hH _
  | hcons _ _ _ hH _ ih₂ =>
    intro _ _ _ _ hJ; cases hJ
    intro a ha
    rcases hH _ ha with h | h
    · exact h.elim
    · exact ih₂ rfl a h
  | hguard _ _ _ _ hH _ _ ih₃ => intro _ _ _ _ hJ; cases hJ; exact fun a ha => ih₃ rfl a (hH _ ha)
  | _ => intro _ _ _ _ hJ; cases hJ

/-- After a failed `try` arm, the remaining arms still remove or let through the
error of the value. -/
theorem HCov.tail {Ψ Θ ε₀ p θ Δ v H H' ε'} (hp : HPat S ε₀ p θ Δ) (hv : Value v)
    (hv' : Typ S Ψ Θ [] v (.ex (.errors ε₀) .pure)) (hc : HCov S v H ε')
    (hH : ∀ n, H n → Catches S ε₀ p θ n ∨ H' n) (hm : matchP p v = none) : HCov S v H' ε' := by
  intro K k ts T hK hS
  rcases hc K k ts T hK hS with h | h
  · rcases hH _ h with (⟨rfl, _⟩ | ⟨rfl, hx⟩) | h'
    · cases hp <;> simp [matchP] at hm
    · obtain ⟨U, _, hU, ht, _⟩ := err_type hv hv' hK hS
      have hs' : Shp S true (.adt T) v := by
        have hs := (ht.shape hv rfl).1
        cases hs with
        | hidden ho => simp [Ty.Opaque, hU] at ho
        | adt hs hU₁ hU₂ => rw [hU] at hU₂; cases hU₂; exact .adt hs hU₁ rfl
        | _ => simp [Ty.hd] at hU
      obtain ⟨q, hq, hqm⟩ := hx v hs'
      simp at hq; subst hq; simp [hm] at hqm
    · exact .inl h'
  · exact .inr h

/-- A raised value of a body with errors `ε₀`, at `Errors ε₀`. -/
theorem raise_inv {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    ∃ ε, Typ S Ψ Θ [] v (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε).le φ := by
  cases h with
  | raise h₁ hφ => exact ⟨_, h₁.valueAny hv _, hφ⟩
  | raiseAdt h₁ hU hφ => exact ⟨_, .errVal hv (h₁.valueAny hv _) hU (Row.Sub.refl _), hφ⟩
  | errVal hv' => cases hv'

theorem StoreTy.alloc {Ψ μ v t₀} (hμ : StoreTy S Ψ μ) (hv : Value v)
    (h : Typ S Ψ Facts.age [] v (.ex t₀ .pure)) : StoreTy S (Ψ ++ [t₀]) (μ ++ [v]) := by
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
    (hlt : l < μ.length) (hv : Value v) (h : Typ S Ψ Facts.age [] v (.ex t₀ .pure)) :
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

theorem Ty.YF.ref {n : Nat} {t : Ty} (h : (Ty.ref t).YF n) : t.YF n := by
  simp only [Ty.YF, Ty.substT, Ty.ref.injEq] at h; exact h

theorem rt_preservation {Ψ Θ Λ μ e t φ Λ' μ' e'} (h : Typ S Ψ Θ [] e (.ex t φ))
    (hμ : StoreTy S Ψ μ) (hc : RtCtx Ψ Θ Λ.length) (hbl : e.Below Λ.length)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) (ht : t.YF Λ.length)
    (hφ : φ.substT (kill Λ.length) = φ) (hh : RHead Λ μ e Λ' μ' e') :
    ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) Θ [] e' (.ex t φ) ∧ ∀ u ∈ X, u.YF Λ.length := by
  have nv := hh.not_value
  have same : ∀ {e'}, Typ S Ψ Θ [] e' (.ex t φ) →
      ∃ X, StoreTy S (Ψ ++ X) μ ∧ Typ S (Ψ ++ X) Θ [] e' (.ex t φ) ∧ ∀ u ∈ X, u.YF Λ.length :=
    fun h' => ⟨[], by simpa using hμ, by simpa using h', by simp⟩
  cases hh with
  | refMake hv =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases lt; cases hT
        exact ⟨_, hμ.alloc hv (((ha.valueAny hv .pure).sub (J' := .ex _ .pure) ⟨la, Stage.le_refl _⟩).cut
            hc.outΘ),
          .loc (by simp [hμ.1]), by simpa using ht.ref⟩
    | errVal hv' => exact (nv hv').elim
  | refGet hl =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases la
        cases ha with
        | loc hl' =>
          have hv := (hμ.2 _ _ _ hl hl').2
          have hk := hv.substT (kill Λ.length) (Fits.kill (hμb _ _ hl))
          simp only [Flick.J.substT, Stage.substT] at hk
          rw [show Ψ.map (Ty.substT (kill Λ.length)) = Ψ by
              conv => rhs; rw [← List.map_id Ψ]
              exact List.map_congr_left fun u hu => hc.yf u hu,
            hc.yf _ (List.mem_of_getElem? hl')] at hk
          exact same ((hk.cut hc.inΘ).sub ⟨lt.trans hT, Stage.pure_le _⟩)
    | errVal hv' => exact (nv hv').elim
  | refSet hv hlt =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | app hc hloc _ hT₁ =>
        cases hc with
        | const _ hT' =>
          obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨_, _, _, _, h, la', _, lt', -⟩ := (lt.trans hT₁).arrow_left
          cases h; cases lt'; cases hT
          cases hloc with
          | loc hl =>
            have hst := hμ.set hl hlt hv (((ha.valueAny hv .pure).sub (J' := .ex _ .pure)
              ⟨la', Stage.le_refl _⟩).cut hc.outΘ)
            exact ⟨[], by simpa using hst, by simpa using .lit, by simp⟩
    | errVal hv' => exact (nv hv').elim
  | await =>
    cases h with
    | await he hφ hT =>
      cases he with
      | task he' hψ _ _ hT' =>
        obtain ⟨_, _, _, _, h, hr, hε, lt, -⟩ := hT'.eff_left
        cases h
        exact same (he'.sub ⟨lt.trans hT, Stage.le_trans (Stage.le_trans hψ (Stage.rt_sub hr hε)) hφ⟩)
    | errVal hv'' => exact (nv hv'').elim
  | build hv hb =>
    cases h with
    | provideL hl he hb' h₀ hφ =>
      obtain ⟨_, hb₂, tb⟩ := canonical_layer hv hl
      rw [hb] at hb₂; cases hb₂
      exact same (.provideE (tb.sub ⟨.refl, Stage.le_trans Stage.rt_union_left hφ⟩)
        he hb' h₀ (Stage.le_trans Stage.rt_union_right hφ))
    | provideE hl =>
      cases canonical_env hv hl <;> simp [Tm.build] at hb
    | errVal hv'' => exact (nv hv'').elim
  | @provideEnv θ₀ _ _ e₀ hθ =>
    cases h with
    | provideL hl =>
      obtain ⟨_, hb, -⟩ := canonical_layer hθ.value hl
      cases hθ <;> simp [Tm.build] at hb
    | @provideE _ _ _ _ o _ _ _ φ₀ r ε hl he hb h₀ hφr =>
      have hθt := hl.valueAny hθ.value .pure
      have nm : ∀ x, o x → ∃ N, x = .nm N := fun x hx => env_nm hθ hθt hx
      have hw : Typ S Ψ (Facts.union Θ Facts.none) [] (.with_ Λ.length θ₀ e₀)
          (.ex t (.rt (Row.union r (Row.needs (.r Λ.length))) ε)) := by
        refine .with_ hθ.value (hθt.cut (Entails.union_left _ _)) (he.cut (Entails.union_left _ _))
          (Stage.le_of_svc hb fun x hx => ?_) (Stage.le_refl _)
        have hx' := Stage.le_svc hb x hx
        cases x with
        | nm N =>
          refine ⟨fun _ => .inr rfl, fun h => ?_⟩
          rcases hx' with h' | h'
          · exact (h h').elim
          · exact .inl h'
        | ob N a =>
          refine ⟨fun h => (h₀ N a hx h).elim, fun _ => ?_⟩
          rcases hx' with h' | h'
          · obtain ⟨_, h''⟩ := nm _ h'; cases h''
          · exact .inl h'
        | atm a =>
          rcases hx' with h' | h'
          · obtain ⟨_, h''⟩ := nm _ h'; cases h''
          · exact .inl h'
        | need N a =>
          rcases hx' with h' | h'
          · obtain ⟨_, h''⟩ := nm _ h'; cases h''
          · exact .inl h'
      have hε : ¬ ε (.atm (.r Λ.length)) := fun h => by
        obtain ⟨_, _, rfl, _, he'⟩ := Stage.rt_le hφr
        have h' := he' _ h
        simp only [Stage.substT, Stage.rt.injEq] at hφ
        rw [← hφ.2] at h'
        exact Row.kill_fresh _ _ h'
      exact same (.inside (F := Facts.none) (fun _ _ h => h.elim) hw
        (Stage.le_rt_sub (Stage.le_refl _) fun x hx => hx.elim (fun h => .inr (.inl h))
          fun h => .inr (.inr (.inr h)))
        (fun h => Nat.lt_irrefl _ (ht.reach h)) hε hφr .refl)
    | errVal hv'' => exact (nv hv'').elim
  | tryRaise hv =>
    cases h with
    | tryR hb hA hφ hs =>
      obtain ⟨ε, hv', hε⟩ := raise_inv hv hb
      obtain ⟨K, k, ts, T, U, hK, hS, hT, hU, ht, hsub⟩ := canonical_errors hv hv'
      obtain ⟨_, _, rfl, _, hε'⟩ := Stage.rt_le hφ
      refine same (.handle hv (.errVal hv ht hU (hsub.trans hε.2)) hA
        (fun K' k' ts' T' hK' hS' => ?_) (fun a ha => ?_) ⟨fun _ h => h.elim, hε'⟩)
      · rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'; exact hs _ (hε.2 _ hT)
      · rcases hs _ ha with h | h
        · exact (hA.halts_no_atm rfl a h).elim
        · exact h
    | tryP hb =>
      obtain ⟨_, _, hε⟩ := raise_inv hv hb; exact hε.elim
    | errVal hv'' => exact (nv hv'').elim
  | handleYes hv hm =>
    cases h with
    | handle _ hv' hA _ _ _ =>
      cases hA with
      | hcons hp hb _ _ => exact same (hb.inst (hp.inst hv hv' hm).1)
    | errVal hv'' => exact (nv hv'').elim
  | handleNo hv hm =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hcons hp _ hr hH => exact same (.handle hv hv' hr (hc.tail hp hv hv' hH hm) ha hφ)
    | errVal hv'' => exact (nv hv'').elim
  | hguardYes hv hm =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hguard hp hg hb hr hH =>
        have hδ := (hp.inst hv hv' hm).1
        exact same (guard_typ (hg.inst hδ) (hb.inst hδ)
          (.handle hv hv' hr (fun K k ts T hK hS => (hc K k ts T hK hS).imp_left (hH _)) ha hφ))
    | errVal hv'' => exact (nv hv'').elim
  | hguardNo hv hm =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hguard _ _ _ hr hH =>
        exact same (.handle hv hv' hr (fun K k ts T hK hS => (hc K k ts T hK hS).imp_left (hH _)) ha hφ)
    | errVal hv'' => exact (nv hv'').elim
  | handleNil hv =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hnil hH =>
        obtain ⟨K, k, ts, T, U, hK, hS, _, hU, ht, hsub⟩ := canonical_errors hv hv'
        rcases hc K k ts T hK hS with h | h
        · exact (hH _ h).elim
        · refine same (.raise (.errVal hv (ht.valueAny hv _) hU fun x hx => ?_) hφ)
          rcases hx with rfl | hx
          · exact h
          · cases x with
            | atm a => exact ha _ (hsub _ (.inr hx))
            | _ => exact hx.elim
    | errVal hv'' => exact (nv hv'').elim
  | scope =>
    cases h with
    | scope hn hF hbody hφ' hφs hT =>
      obtain ⟨F', _, hF', hb', hφ₁⟩ := scope_body hn (fun p hp => hbl p hp) hF hbody hφ' hc.yf hc.idem
      refine same (.inside hF' hb' (Stage.le_rt_sub hφ₁ fun x hx => hx.elim .inl fun h =>
          h.elim (fun h => .inr (.inl h)) fun h => .inr (.inr (.inl h)))
        (Ty.kill_fresh _ _) (Row.kill_fresh _ _)
        (Stage.YF_kill hφs hφ) ?_)
      have := hT.substT (kill Λ.length); rwa [ht] at this
    | errVal hv'' => exact (nv hv'').elim
  | open_ =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases la
        obtain ⟨_, he, hs⟩ := canonical_owner .own ha
        cases he; rw [hs] at lt
        exact same ((Typ.cap).sub ⟨lt.trans hT, Stage.pure_le _⟩)
    | errVal hv' => exact (nv hv').elim
  | op =>
    cases h with
    | app hf _ _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h; cases lt; cases hT; exact same .lit
    | errVal hv' => exact (nv hv').elim
  | finRun => exact same h
  | close _ _ hw =>
    cases h with
    | inside hF hbody hφ' _ _ hφ hT =>
      have hcut := inside_entails (hbl _ (.inl (.inl rfl))) hF hc.inΘ
      have hbody := hbody.cut hcut
      rcases hw with hw | ⟨v, hv, rfl⟩
      · exact same ((hbody.valueAny hw _).sub ⟨hT, Stage.le_refl _⟩)
      · obtain ⟨ε, hv', hε⟩ := raise_inv hv hbody
        obtain ⟨_, _, rfl, _, he⟩ := Stage.rt_le hφ
        exact same (.raise (hv'.valueAny hv _) ⟨fun _ h => h.elim,
          fun n hn => he n ((Stage.le_err (Stage.le_trans hε hφ')) n hn)⟩)
    | errVal hv'' => exact (nv hv'').elim

end Flick
