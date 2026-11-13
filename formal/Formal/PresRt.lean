import Formal.PresHdlRt
import Formal.ScopeRun
import Formal.RaiseTyping

/-!
# Preservation of runtime steps at the redex
-/

namespace Flick

variable {S : Sig}

/-- The error type of a raised value. -/
theorem err_type {Ψ Θ v ε} (hv : Value v) (h : Typ S Ψ Θ [] v (.ex (.errors ε) .pure))
    {K k ts T} (hK : v.head = .ctor K) (hS : S.ctor K = some (k, ts, T)) :
    ∃ U, ε (.nm T) ∧ U.hd = .adt T ∧ Typ S Ψ Θ [] v (.ex U .pure) ∧ Row.Sub (Row.err T U) ε := by
  obtain ⟨K', k', ts', T', U, hK', hS', hε, hU, ht, hsub⟩ := canonical_errors hv h
  rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'
  exact ⟨U, hε, hU, ht, hsub⟩

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
        | adt hs hU₁ hU₂ =>
          rw [hU] at hU₂; cases hU₂
          refine .adt hs hU₁ rfl fun _ _ _ _ _ _ _ h hl => ⟨Ty.ident, ?_⟩
          obtain ⟨rfl, rfl⟩ := Ty.tcon_eq_adt h.symm
          rw [List.length_eq_zero_iff.mp hl.symm]; exact .refl
        | _ => simp [Ty.hd] at hU
      obtain ⟨q, hq, hqm⟩ := hx v hs'
      simp at hq; subst hq; simp [hm] at hqm
    · exact .inl h'
  · exact .inr h

/-- A raised value of a body with errors `ε₀`, at `Errors ε₀`. -/
theorem raise_inv {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    ∃ ε, Typ S Ψ Θ [] v (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε).le φ := by
  exact h.raise_value v t φ rfl rfl hv

theorem StoreTy.alloc {Ψ μ v t₀} (hμ : StoreTy S Ψ μ) (hv : Value v)
    (h : Typ S Ψ Facts.age [] v (.ex t₀ .pure)) :
    StoreTy S (Ψ ++ [t₀]) (μ ++ [v]) := by
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

/-- An unchanged store needs no new cells. -/
theorem StoreTy.same {Ψ μ Θ e t φ} (hμ : StoreTy S Ψ μ)
    (h : Typ S Ψ Θ [] e (.ex t φ)) {n : Nat} :
    ∃ X, StoreTy S (Ψ ++ X) μ ∧ Typ S (Ψ ++ X) Θ [] e (.ex t φ) ∧ ∀ u ∈ X, u.YF n :=
  ⟨[], by simpa using hμ, by simpa using h, by simp⟩

private theorem Typ.rtStepCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ Λ μ t φ Λ' μ' e', Γ = [] → J = .ex t φ → StoreTy S Ψ μ → RtCtx Ψ Θ Λ.length →
      e.Below Λ.length →
      (∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) → t.YFMod Λ.length →
      φ.substT (kill Λ.length) = φ → RHead Λ μ e Λ' μ' e' →
      ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) Θ [] e' (.ex t φ) ∧
        ∀ u ∈ X, u.YF Λ.length := by
  induction h with
  | conv _ he ih =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hJ
    obtain ⟨X, hstore, hbody, hfresh⟩ := ih Λ μ _ _ Λ' μ' e' hΓ rfl hμ hcRt hbl hμb
      (ht.equiv he.symm) hφfixed hh
    exact ⟨X, hstore, hbody.conv he, hfresh⟩
  | allI hv | errVal hv =>
    intro _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ hh
    exact (hh.not_value hv).elim
  | app hf ha hψ hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | refMake hv =>
      obtain ⟨ι, _, hT'⟩ := hf.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
      cases h
      have he := LeLive.eq_ref_left (lt.trans hT)
      have hfresh := (ht.equiv he).ref
      have hv' := ((ha.valueAny hv .pure).sub (J' := .ex _ .pure)
        ⟨la, Stage.le_refl _⟩).conv hfresh.symm
      refine ⟨[ι.t.substT (kill Λ.length)], hμ.alloc hv (hv'.cut hcRt.outΘ), ?_, ?_⟩
      · exact (Typ.loc (by simp [hμ.1])).sub
          ⟨(LeLive.equiv (.ref hfresh)).trans (lt.trans hT), Stage.pure_le _⟩
      · simpa using Ty.yf_kill Λ.length ι.t
    | refGet hl =>
      obtain ⟨_, _, hT'⟩ := hf.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
      cases h
      obtain ⟨u, hl', hu⟩ := (ha.conv (LeLive.eq_ref la)).loc_ref _ _ _ rfl rfl
      have hv := (hμ.2 _ _ _ hl hl').2
      have hk := hv.substT (kill Λ.length) (Fits.kill (hμb _ _ hl))
      simp only [Flick.J.substT, Stage.substT] at hk
      have hΨ := List.map_congr_left hcRt.yf
      simp only [List.map_id_fun'] at hΨ
      rw [hΨ, hcRt.yf _ (List.mem_of_getElem? hl')] at hk
      exact hμ.same (((hk.conv hu).cut hcRt.inΘ).sub ⟨lt.trans hT, Stage.pure_le _⟩)
    | refSet hv hlt =>
      obtain ⟨_, _, _, _, hConst, hloc, _, hT₁⟩ := hf.app_arrow
      obtain ⟨_, _, hT'⟩ := hConst.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
      cases h
      obtain ⟨u, hl, hu⟩ := (hloc.conv (LeLive.eq_ref la)).loc_ref _ _ _ rfl rfl
      obtain ⟨_, _, _, _, h, la', _, lt', -⟩ := (lt.trans hT₁).arrow_left
      cases h
      cases LeLive.eq_base_left lt'
      cases LeLive.eq_base_left hT
      have hw := ((ha.valueAny hv .pure).sub (J' := .ex _ .pure)
        ⟨la', Stage.le_refl _⟩).conv hu.symm
      have hst := hμ.set hl hlt hv (hw.cut hcRt.outΘ)
      exact ⟨[], by simpa using hst, by simpa using .lit, by simp⟩
    | scopeRun hv =>
      obtain ⟨_, hok, hT'⟩ := hf.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
      cases h
      exact hμ.same (scopeRun_body hv (ha.conv (LeLive.eq_all la))
        (Stage.le_trans (Stage.le_use _ _) (Stage.le_trans lψ hψ)) (lt.trans hT) hok)
    | open_ =>
      obtain ⟨_, _, hT'⟩ := hf.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
      cases h
      obtain ⟨_, he, hs⟩ := canonical_owner .own (ha.conv (LeLive.eq_owner la))
      cases he; rw [hs] at lt
      exact hμ.same ((Typ.cap).sub ⟨lt.trans hT, Stage.pure_le _⟩)
    | op =>
      obtain ⟨_, _, hT'⟩ := hf.const_arrow (by intro E o cl h; cases h)
      obtain ⟨_, hop⟩ := Const.ty_op_arrow S _
      rw [hop] at hT'
      obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
      cases h
      cases LeLive.eq_base_left lt
      cases LeLive.eq_base_left hT
      exact hμ.same .lit
  | await he hφ hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | await =>
      obtain ⟨_, _, he', hψ, ht⟩ := he.task_eff _ _ _ _ _ _ rfl rfl
      exact hμ.same (he'.sub ⟨ht.trans hT, Stage.le_trans hψ hφ⟩)
  | tryR hb hA hφ hs =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | tryRaise hv =>
      obtain ⟨ε, hv', hε⟩ := raise_inv hv hb
      obtain ⟨K, k, ts, T, U, hK, hS, hT, hU, ht, hsub⟩ := canonical_errors hv hv'
      obtain ⟨_, _, rfl, _, hε'⟩ := Stage.rt_le hφ
      refine hμ.same (.handle hv (.errVal hv ht hU (hsub.trans hε.2)) hA
        (fun K' k' ts' T' hK' hS' => ?_) (fun x hx => ?_) ⟨fun _ h => h.elim, hε'⟩)
      · rw [hK] at hK'; cases hK'; rw [hS] at hS'; cases hS'; exact hs _ (hε.2 _ hT)
      · exact .inr ((hs _ hx).imp_right Row.back_modNames)
  | tryP hb =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | tryRaise hv =>
      obtain ⟨_, _, hε⟩ := raise_inv hv hb; exact hε.elim
  | handle hv hv' hA hc ha hφ =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | handleYes hv hm =>
      cases hA with
      | hcons hp hb _ _ => exact hμ.same (hb.inst (hp.inst hv hv' hm).1)
    | handleNo hv hm =>
      cases hA with
      | hcons hp _ hr hH =>
        refine hμ.same (.handle hv hv' hr (hc.tail hp hv hv' hH hm) (fun x hx => ?_) hφ)
        rcases ha x hx with h | h | h
        · exact .inl h
        · rcases hH x h with h | h
          · cases x with
            | nm N => exact .inl ⟨N, rfl⟩
            | _ => obtain ⟨rfl, _⟩ := h; cases hp <;> simp [matchP] at hm
          · exact .inr (.inl h)
        · exact .inr (.inr h)
    | hguardYes hv hm =>
      cases hA with
      | hguard hp hg hb hr hH =>
        have hδ := (hp.inst hv hv' hm).1
        exact hμ.same (guard_typ (hg.inst hδ) (hb.inst hδ)
          (.handle hv hv' hr (fun K k ts T hK hS => (hc K k ts T hK hS).imp_left (hH _))
            (fun x hx => (ha x hx).imp_right (Or.imp_left (hH x))) hφ))
    | hguardNo hv hm =>
      cases hA with
      | hguard _ _ _ hr hH =>
        exact hμ.same (.handle hv hv' hr (fun K k ts T hK hS => (hc K k ts T hK hS).imp_left (hH _))
          (fun x hx => (ha x hx).imp_right (Or.imp_left (hH x))) hφ)
    | handleNil hv =>
      cases hA with
      | hnil hH =>
        obtain ⟨K, k, ts, T, U, hK, hS, _, hU, ht, hsub⟩ := canonical_errors hv hv'
        rcases hc K k ts T hK hS with h | h
        · exact (hH _ h).elim
        · refine hμ.same (.raise (.errVal hv (ht.valueAny hv _) hU fun x hx => ?_) hφ)
          rcases hx with rfl | hx
          · exact h
          · cases x with
            | atm a =>
              exact (((ha _ (hsub _ (.inr hx))).resolve_left fun ⟨_, h⟩ => by cases h).resolve_left
                (hH _))
            | _ => exact hx.elim
  | scope hn hF hbody hφ' hφs hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | scope =>
      obtain ⟨F', _, hF', hb', hφ₁⟩ := scope_body hn (fun p hp => hbl p hp) hF hbody hφ' hcRt.yf hcRt.idem
      refine hμ.same (.inside hF' hb' hφ₁
        (Ty.kill_fresh _ _) (Row.kill_fresh _ _)
        (Stage.YF_kill hφs hφfixed) ?_)
      exact (hT.substT (kill Λ.length)).trans (.equiv ht)
  | inside hF hbody hφ' havoidT havoidE hφ hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    have h := Typ.inside hF hbody hφ' havoidT havoidE hφ hT
    cases hh with
    | close _ _ hw =>
      have hcut := inside_entails (hbl _ (.inl rfl)) hF hcRt.inΘ
      have hbody := hbody.cut hcut
      rcases hw with hw | ⟨v, hv, rfl⟩ | ⟨_, _, _, rfl⟩
      · exact hμ.same ((hbody.valueAny hw _).sub ⟨hT, Stage.le_refl _⟩)
      · obtain ⟨ε, hv', hε⟩ := raise_inv hv hbody
        obtain ⟨rout, εout, rfl, _, he⟩ := Stage.rt_le hφ
        exact hμ.same (.raise (φ := .rt rout εout) (hv'.valueAny hv _) ⟨fun _ h => h.elim,
          fun n hn => he n ((Stage.le_err (Stage.le_trans hε hφ')) n hn)⟩)
      · obtain ⟨_, _, _, h₁, h₂, _⟩ := hbody.unw_type _ _ _ _ _ rfl rfl
        exact hμ.same (.unw h₁ h₂ (Stage.none_le (Stage.rt_ne_pure hφ)))
    | finRun =>
      exact hμ.same h
  | hdl hal hn hrf hψ he hφ₀ hOb hc' hφ' hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    cases hh with
    | hdl =>
      obtain ⟨c, hcy, hcl, hb⟩ := hdl_install hal hn hrf hψ he hφ₀ hOb hc' hφ' hT hbl hcRt.yf hcRt.idem ht hφfixed
      exact ⟨[c], hμ.alloc .lam hcl, by rw [hμ.1]; exact hb, by simpa using hcy⟩
  | hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha hl hcm hφ hT hat =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    have h := Typ.hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha hl hcm hφ hT hat
    cases hh with
    | hctlRaise hEi hk _ =>
      exact hμ.same (hctl_signal hEi hk raise_any h)
    | hctlUnw hEi hk =>
      exact hμ.same (hctl_signal hEi hk unw_any h)
  | hkarm hℓ hal hop hE hk hp hφ₀ hc hnt ha hle hφ hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    have h := Typ.hkarm hℓ hal hop hE hk hp hφ₀ hc hnt ha hle hφ hT
    cases hh with
    | hkarmRaise hEi hk _ =>
      exact hμ.same (hkarm_raise hEi hk h)
    | hkarmUnw hEi hk =>
      exact hμ.same (hkarm_unw hEi hk h)
  | hres hℓ hb hA ha hp hl hcp hφ hT =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    have h := Typ.hres hℓ hb hA ha hp hl hcp hφ hT
    cases hh with
    | hresRaise hA ha _ =>
      exact hμ.same (hres_signal hA ha raise_any h)
    | hresUnw hA ha =>
      exact hμ.same (hres_signal hA ha unw_any h)
  | hwith hℓ hal hrf hψ he hφ₀ hc hφ hT hat =>
    intro Λ μ t φ Λ' μ' e' hΓ hJ hμ hcRt hbl hμb ht hφfixed hh
    cases hΓ; cases hJ
    have h := Typ.hwith hℓ hal hrf hψ he hφ₀ hc hφ hT hat
    cases hh with
    | hwithCatch =>
      exact hμ.same (hwith_catch h)
  | _ => intro _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ hh; cases hh

theorem rt_preservation {Ψ Θ Λ μ e t φ Λ' μ' e'} (h : Typ S Ψ Θ [] e (.ex t φ))
    (hμ : StoreTy S Ψ μ) (hc : RtCtx Ψ Θ Λ.length) (hbl : e.Below Λ.length)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) (ht : t.YF Λ.length)
    (hφ : φ.substT (kill Λ.length) = φ) (hh : RHead Λ μ e Λ' μ' e') :
    ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) Θ [] e' (.ex t φ) ∧ ∀ u ∈ X, u.YF Λ.length :=
  h.rtStepCore Λ μ t φ Λ' μ' e' rfl rfl hμ hc hbl hμb ht.modulo hφ hh

end Flick
