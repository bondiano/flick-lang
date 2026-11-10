import Formal.PresHdlRt
import Formal.ScopeRun

/-!
# Preservation of runtime steps at the redex
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

/-- The error type of a raised value. -/
theorem err_type_mode {Ψ Θ v ε} (hv : Value v) (h : TypM S mode Ψ Θ [] v (.ex (.errors ε) .pure))
    {K k ts T} (hK : v.head = .ctor K) (hS : S.ctor K = some (k, ts, T)) :
    ∃ U, ε (.nm T) ∧ U.hd = .adt T ∧ TypM S mode Ψ Θ [] v (.ex U .pure) ∧ Row.Sub (Row.err T U) ε := by
  obtain ⟨K', k', ts', T', U, hK', hS', hε, hU, ht, hsub⟩ := canonical_errors_mode hv h
  rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'
  exact ⟨U, hε, hU, ht, hsub⟩

theorem err_type {Ψ Θ v ε} (hv : Value v) (h : Typ S Ψ Θ [] v (.ex (.errors ε) .pure)) {K k ts T}
    (hK : v.head = .ctor K) (hS : S.ctor K = some (k, ts, T)) :
    ∃ U, ε (.nm T) ∧ U.hd = .adt T ∧ Typ S Ψ Θ [] v (.ex U .pure) ∧ Row.Sub (Row.err T U) ε :=
  err_type_mode hv h hK hS

/-- After a failed `try` arm, the remaining arms still remove or let through the
error of the value. -/
theorem HCov.tail {Ψ Θ ε₀ p θ Δ v H H' ε'} (hp : HPat S ε₀ p θ Δ) (hv : Value v)
    (hv' : TypM S mode Ψ Θ [] v (.ex (.errors ε₀) .pure)) (hc : HCov S v H ε')
    (hH : ∀ n, H n → Catches S ε₀ p θ n ∨ H' n) (hm : matchP p v = none) : HCov S v H' ε' := by
  intro K k ts T hK hS
  rcases hc K k ts T hK hS with h | h
  · rcases hH _ h with (⟨rfl, _⟩ | ⟨rfl, hx⟩) | h'
    · cases hp <;> simp [matchP] at hm
    · obtain ⟨U, _, hU, ht, _⟩ := err_type_mode hv hv' hK hS
      have hs' : Shp S true (.adt T) v := by
        have hs := (ht.shape hv rfl).1
        cases hs with
        | hidden ho => simp [Ty.Opaque, hU] at ho
        | adt hs hU₁ hU₂ =>
          rw [hU] at hU₂; cases hU₂
          refine .adt hs hU₁ rfl fun _ _ _ _ _ _ _ h hl => ⟨Ty.ident, ?_⟩
          obtain ⟨rfl, rfl⟩ := Ty.tcon_eq_adt h.symm
          rw [List.length_eq_zero_iff.mp hl.symm]; rfl
        | _ => simp [Ty.hd] at hU
      obtain ⟨q, hq, hqm⟩ := hx v hs'
      simp at hq; subst hq; simp [hm] at hqm
    · exact .inl h'
  · exact .inr h

/-- A raised value of a body with errors `ε₀`, at `Errors ε₀`. -/
theorem raise_inv_mode {Ψ Θ v t φ} (hv : Value v) (h : TypM S mode Ψ Θ [] (.raise v) (.ex t φ)) :
    ∃ ε, TypM S mode Ψ Θ [] v (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε).le φ := by
  cases h with
  | raise h₁ hφ => exact ⟨_, h₁.valueAny hv _, hφ⟩
  | raiseAdt h₁ hU hφ => exact ⟨_, .errVal hv (h₁.valueAny hv _) hU (Row.Sub.refl _), hφ⟩
  | errVal hv' | allI hv' => cases hv'

theorem raise_inv {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    ∃ ε, Typ S Ψ Θ [] v (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε).le φ :=
  raise_inv_mode hv h

theorem StoreTyM.alloc {Ψ μ v t₀} (hμ : StoreTyM S mode Ψ μ) (hv : Value v)
    (h : TypM S mode Ψ Facts.age [] v (.ex t₀ .pure)) :
    StoreTyM S mode (Ψ ++ [t₀]) (μ ++ [v]) := by
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

theorem StoreTyM.set {Ψ μ l v t₀} (hμ : StoreTyM S mode Ψ μ) (hl : Ψ[l]? = some t₀)
    (hlt : l < μ.length) (hv : Value v) (h : TypM S mode Ψ Facts.age [] v (.ex t₀ .pure)) :
    StoreTyM S mode Ψ (μ.set l v) := by
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

theorem rt_preservation_mode {Ψ Θ Λ μ e t φ Λ' μ' e'} (h : TypM S mode Ψ Θ [] e (.ex t φ))
    (hμ : StoreTyM S mode Ψ μ) (hc : RtCtx Ψ Θ Λ.length) (hbl : e.Below Λ.length)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) (ht : t.YF Λ.length)
    (hφ : φ.substT (kill Λ.length) = φ) (hh : RHead Λ μ e Λ' μ' e') :
    ∃ X, StoreTyM S mode (Ψ ++ X) μ' ∧ TypM S mode (Ψ ++ X) Θ [] e' (.ex t φ) ∧ ∀ u ∈ X, u.YF Λ.length := by
  have nv := hh.not_value
  have same : ∀ {e'}, TypM S mode Ψ Θ [] e' (.ex t φ) →
      ∃ X, StoreTyM S mode (Ψ ++ X) μ ∧ TypM S mode (Ψ ++ X) Θ [] e' (.ex t φ) ∧ ∀ u ∈ X, u.YF Λ.length :=
    fun h' => ⟨[], by simpa using hμ, by simpa using h', by simp⟩
  cases hh with
  | refMake hv =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h
        cases TypingMode.subtype.eq_ref_left lt
        cases TypingMode.subtype.eq_ref_left hT
        exact ⟨_, hμ.alloc hv (((ha.valueAny hv .pure).sub (J' := .ex _ .pure) ⟨la, Stage.le_refl _⟩).cut
            hc.outΘ),
          .loc (by simp [hμ.1]), by simpa using ht.ref⟩
    | errVal hv' | allI hv' => exact (nv hv').elim
  | refGet hl =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases TypingMode.subtype.eq_ref la
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
    | errVal hv' | allI hv' => exact (nv hv').elim
  | refSet hv hlt =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | app hc hloc _ hT₁ =>
        cases hc with
        | const _ hT' =>
          obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
          cases h; cases TypingMode.subtype.eq_ref la
          obtain ⟨_, _, _, _, h, la', _, lt', -⟩ := (lt.trans hT₁).arrow_left
          cases h
          cases TypingMode.subtype.eq_base_left lt'
          cases TypingMode.subtype.eq_base_left hT
          cases hloc with
          | loc hl =>
            have hst := hμ.set hl hlt hv (((ha.valueAny hv .pure).sub (J' := .ex _ .pure)
              ⟨la', Stage.le_refl _⟩).cut hc.outΘ)
            exact ⟨[], by simpa using hst, by simpa using .lit, by simp⟩
    | errVal hv' | allI hv' => exact (nv hv').elim
  | await =>
    cases h with
    | await he hφ hT =>
      cases he with
      | task he' hψ _ _ hT' =>
        obtain ⟨_, _, _, _, h, hle, lt, -⟩ := TypingMode.subtype.eff_left_mode hT'
        cases h
        exact same (he'.sub ⟨lt.trans hT, Stage.le_trans (Stage.le_trans hψ hle) hφ⟩)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | tryRaise hv =>
    cases h with
    | tryR hb hA hφ hs =>
      obtain ⟨ε, hv', hε⟩ := raise_inv_mode hv hb
      obtain ⟨K, k, ts, T, U, hK, hS, hT, hU, ht, hsub⟩ := canonical_errors_mode hv hv'
      obtain ⟨_, _, rfl, _, hε'⟩ := Stage.rt_le hφ
      refine same (.handle hv (.errVal hv ht hU (hsub.trans hε.2)) hA
        (fun K' k' ts' T' hK' hS' => ?_) (fun x hx => ?_) ⟨fun _ h => h.elim, hε'⟩)
      · rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'; exact hs _ (hε.2 _ hT)
      · exact .inr (hs _ hx)
    | tryP hb =>
      obtain ⟨_, _, hε⟩ := raise_inv_mode hv hb; exact hε.elim
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | handleYes hv hm =>
    cases h with
    | handle _ hv' hA _ _ _ =>
      cases hA with
      | hcons hp hb _ _ => exact same (hb.inst (hp.inst_mode hv hv' hm).1)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | handleNo hv hm =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hcons hp _ hr hH =>
        refine same (.handle hv hv' hr (hc.tail hp hv hv' hH hm) (fun x hx => ?_) hφ)
        rcases ha x hx with h | h | h
        · exact .inl h
        · rcases hH x h with h | h
          · cases x with
            | nm N => exact .inl ⟨N, rfl⟩
            | _ => obtain ⟨rfl, _⟩ := h; cases hp <;> simp [matchP] at hm
          · exact .inr (.inl h)
        · exact .inr (.inr h)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hguardYes hv hm =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hguard hp hg hb hr hH =>
        have hδ := (hp.inst_mode hv hv' hm).1
        exact same (guard_typ_mode (hg.inst hδ) (hb.inst hδ)
          (.handle hv hv' hr (fun K k ts T hK hS => (hc K k ts T hK hS).imp_left (hH _))
            (fun x hx => (ha x hx).imp_right (Or.imp_left (hH x))) hφ))
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hguardNo hv hm =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hguard _ _ _ hr hH =>
        exact same (.handle hv hv' hr (fun K k ts T hK hS => (hc K k ts T hK hS).imp_left (hH _))
          (fun x hx => (ha x hx).imp_right (Or.imp_left (hH x))) hφ)
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | handleNil hv =>
    cases h with
    | handle _ hv' hA hc ha hφ =>
      cases hA with
      | hnil hH =>
        obtain ⟨K, k, ts, T, U, hK, hS, _, hU, ht, hsub⟩ := canonical_errors_mode hv hv'
        rcases hc K k ts T hK hS with h | h
        · exact (hH _ h).elim
        · refine same (.raise (.errVal hv (ht.valueAny hv _) hU fun x hx => ?_) hφ)
          rcases hx with rfl | hx
          · exact h
          · cases x with
            | atm a =>
              exact (((ha _ (hsub _ (.inr hx))).resolve_left fun ⟨_, h⟩ => by cases h).resolve_left
                (hH _))
            | _ => exact hx.elim
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | scopeRun hv =>
    cases h with
    | app hf ha hψ hT =>
      cases hf with
      | const hok hT' =>
        obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
        cases h; cases TypingMode.subtype.eq_all la
        exact same (scopeRun_body hv ha
          (Stage.le_trans (TypingMode.le_use mode _ _) (Stage.le_trans lψ hψ)) (lt.trans hT)
          fun hm => by subst hm; exact hok)
    | errVal hv' | allI hv' => exact (nv hv').elim
  | scope =>
    cases h with
    | scope hn hF hbody hφ' hφs hT =>
      obtain ⟨F', _, hF', hb', hφ₁⟩ := scope_body hn (fun p hp => hbl p hp) hF hbody hφ' hc.yf hc.idem
      refine same (.inside hF' hb' hφ₁
        (Ty.kill_fresh _ _) (Row.kill_fresh _ _)
        (Stage.YF_kill hφs hφ) ?_)
      have := hT.substT (kill Λ.length); rwa [ht] at this
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | open_ =>
    cases h with
    | app hf ha _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases TypingMode.subtype.eq_owner la
        obtain ⟨_, he, hs⟩ := canonical_owner_mode .own ha
        cases he; rw [hs] at lt
        exact same ((Typ.cap).sub ⟨lt.trans hT, Stage.pure_le _⟩)
    | errVal hv' | allI hv' => exact (nv hv').elim
  | op =>
    cases h with
    | app hf _ _ hT =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, hop⟩ := Const.ty_op_arrow S _ mode
        rw [hop] at hT'
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h
        cases TypingMode.subtype.eq_base_left lt
        cases TypingMode.subtype.eq_base_left hT
        exact same .lit
    | errVal hv' | allI hv' => exact (nv hv').elim
  | finRun => exact same h
  | close _ _ hw =>
    cases h with
    | inside hF hbody hφ' _ _ hφ hT =>
      have hcut := inside_entails (hbl _ (.inl rfl)) hF hc.inΘ
      have hbody := hbody.cut hcut
      rcases hw with hw | ⟨v, hv, rfl⟩ | ⟨_, _, _, rfl⟩
      · exact same ((hbody.valueAny hw _).sub ⟨hT, Stage.le_refl _⟩)
      · obtain ⟨ε, hv', hε⟩ := raise_inv_mode hv hbody
        obtain ⟨_, _, rfl, _, he⟩ := Stage.rt_le hφ
        exact same (.raise (hv'.valueAny hv _) ⟨fun _ h => h.elim,
          fun n hn => he n ((Stage.le_err (Stage.le_trans hε hφ')) n hn)⟩)
      · cases hbody with
        | unw h₁ h₂ _ => exact same (.unw h₁ h₂ (Stage.none_le (Stage.rt_ne_pure hφ)))
        | errVal hv | allI hv => cases hv
    | errVal hv'' | allI hv'' => exact (nv hv'').elim
  | hctlRaise hEi hk _ => exact same (hctl_signal hEi hk raise_any h)
  | hctlUnw hEi hk => exact same (hctl_signal hEi hk unw_any h)
  | hkarmRaise hEi hk _ => exact same (hkarm_raise hEi hk h)
  | hkarmUnw hEi hk => exact same (hkarm_unw hEi hk h)
  | hresRaise hA ha _ => exact same (hres_signal hA ha raise_any h)
  | hresUnw hA ha => exact same (hres_signal hA ha unw_any h)
  | hwithCatch => exact same (hwith_catch h)
  | hdl =>
    cases h with
    | hdl hal hn hrf hψ he hφ₀ hOb hc' hφ' hT =>
      obtain ⟨c, hcy, hcl, hb⟩ := hdl_install hal hn hrf hψ he hφ₀ hOb hc' hφ' hT hbl hc.yf hc.idem ht hφ
      exact ⟨[c], hμ.alloc .lam hcl, by rw [hμ.1]; exact hb, by simpa using hcy⟩
    | errVal hv'' | allI hv'' => exact (nv hv'').elim


theorem rt_preservation {Ψ Θ Λ μ e t φ Λ' μ' e'} (h : Typ S Ψ Θ [] e (.ex t φ))
    (hμ : StoreTy S Ψ μ) (hc : RtCtx Ψ Θ Λ.length) (hbl : e.Below Λ.length)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) (ht : t.YF Λ.length)
    (hφ : φ.substT (kill Λ.length) = φ) (hh : RHead Λ μ e Λ' μ' e') :
    ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) Θ [] e' (.ex t φ) ∧ ∀ u ∈ X, u.YF Λ.length :=
  rt_preservation_mode h hμ hc hbl hμb ht hφ hh

end Flick
