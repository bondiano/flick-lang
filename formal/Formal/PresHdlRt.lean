import Formal.ScopeStep

/-!
# Preservation of the run-time steps of handlers at the redex
-/

namespace Flick

variable {S : Sig}

/-- `raise` and unwinding have every type. -/
def AnyTy (S : Sig) (Ψ : List Ty) (Θ : Facts) (x : Tm) : Prop :=
  ∀ {t₁ t₂ φ}, Typ S Ψ Θ [] x (.ex t₁ φ) → Typ S Ψ Θ [] x (.ex t₂ φ)

theorem raise_any {Ψ Θ w} : AnyTy S Ψ Θ (.raise w) := by
  intro _ _ _ h
  cases h with
  | raise h₁ hφ => exact .raise h₁ hφ
  | raiseAdt h₁ hU hφ => exact .raiseAdt h₁ hU hφ
  | errVal hv | allI hv => cases hv

theorem unw_any {Ψ Θ q ℓ a} : AnyTy S Ψ Θ (.unw q ℓ a) := by
  intro _ _ _ h
  cases h with
  | unw h₁ h₂ h₃ => exact .unw h₁ h₂ h₃
  | errVal hv | allI hv => cases hv

/-- The handler catches its signal and runs what it holds. In the live discipline the
region of the handler is live where the answer is used. -/
theorem hwith_catch {Ψ Θ p ℓ E al r a t φ}
    (h : Typ S Ψ Θ [] (.hwith p ℓ E al r (.unw p ℓ a)) (.ex t φ)) :
    Typ S Ψ Θ [] a (.ex t φ) := by
  cases h with
  | hwith hℓ _ _ _ he _ _ hφ hT hat =>
    cases he with
    | unw hℓ' ha _ =>
      rw [hℓ] at hℓ'
      simp only [Option.some.injEq, Ty.arrow.injEq, Stage.rt.injEq] at hℓ'
      obtain ⟨-, ⟨rfl, rfl⟩, rfl, -⟩ := hℓ'
      exact ha.sub ⟨hT, Stage.use_rg_le hφ hat⟩
    | errVal hv | allI hv => cases hv
  | errVal hv | allI hv => cases hv

/-- A signal of an arm unwinds the parked body to the handler. -/
theorem hctl_signal {Ψ Θ p ℓ κ tl E al r x k t φ} {Ei : List Frame} (hEi : OkE Ei)
    (hk : k = plug Ei (.hole p)) (hx : ∀ {Θ'}, AnyTy S Ψ Θ' x)
    (h : Typ S Ψ Θ [] (.hctl p ℓ κ tl E al r x k) (.ex t φ)) :
    Typ S Ψ Θ [] (.hwith p ℓ E al r (plug Ei (.unw p ℓ x))) (.ex t φ) := by
  cases h with
  | hctl hℓ hal hrf hψ hop _ _ hE hk' hp hφ₀ ha _ hcm hφ hT hat =>
    obtain ⟨rfl, -⟩ := plug_hole_inj hEi hE (hk.symm.trans hk')
    refine .hwith hℓ hal hrf hψ (refill_app hE perform_not_value hp (perform_fn hop) perform_up
      fun T φ' hΘ hψ₀ _ _ => ?_) hφ₀ hcm hφ hT hat
    exact .unw hℓ ((hx (ha.cut hΘ.entails)).sub
      (J' := .ex _ (Stage.use (.rt _ _) (Ty.rg p).reach))
      ⟨LeLive.refl, Stage.use_ctl_le tl _ _ _⟩)
      (Stage.none_le (Stage.rt_ne_pure hψ₀))
  | errVal hv | allI hv => cases hv

/-- An arm of a kept handler that raises lets the parked body go. -/
theorem hkarm_raise {Ψ Θ q ℓ E al w k t φ} {Ei : List Frame} (hEi : OkE Ei)
    (hk : k = plug Ei (.hole q))
    (h : Typ S Ψ Θ [] (.hkarm q ℓ E al (.raise w) k) (.ex t φ)) :
    Typ S Ψ Θ [] (.hkeep q ℓ E al (plug Ei (.unw q ℓ (.raise w)))) (.ex t φ) := by
  cases h with
  | hkarm hℓ hal hop hE hk' hp hφ₀ hc hnt ha hle hφ hT =>
    obtain ⟨rfl, -⟩ := plug_hole_inj hEi hE (hk.symm.trans hk')
    refine .hkeep hℓ hal (refill_app hE perform_not_value hp (perform_fn hop) perform_up
      fun T φ' hΘ hψ₀ _ _ => ?_) hφ₀ hc hnt hle hφ hT
    exact .unw hℓ ((raise_any (ha.cut hΘ.entails)).sub
      (J' := .ex _ (Stage.use (.rt _ _) (Ty.rg q).reach))
      ⟨LeLive.refl, Stage.le_use _ _⟩)
      (Stage.none_le (Stage.rt_ne_pure hψ₀))
  | errVal hv | allI hv => cases hv

/-- A signal in an arm of a kept handler goes on through the parked body. -/
theorem hkarm_unw {Ψ Θ q ℓ E al q' ℓ' a k t φ} {Ei : List Frame} (hEi : OkE Ei)
    (hk : k = plug Ei (.hole q))
    (h : Typ S Ψ Θ [] (.hkarm q ℓ E al (.unw q' ℓ' a) k) (.ex t φ)) :
    Typ S Ψ Θ [] (.hkeep q ℓ E al (plug Ei (.unw q' ℓ' a))) (.ex t φ) := by
  cases h with
  | hkarm hℓ hal hop hE hk' hp hφ₀ hc hnt ha hle hφ hT =>
    obtain ⟨rfl, -⟩ := plug_hole_inj hEi hE (hk.symm.trans hk')
    refine .hkeep hℓ hal (refill_app hE perform_not_value hp (perform_fn hop) perform_up
      fun T φ' hΘ hψ₀ _ _ => ?_) hφ₀ hc hnt hle hφ hT
    cases ha with
    | unw hℓ' ha' _ => exact .unw hℓ' (ha'.cut hΘ.entails) (Stage.none_le (Stage.rt_ne_pure hψ₀))
    | errVal hv | allI hv => cases hv
  | errVal hv | allI hv => cases hv

/-- A signal of a resumed body lets the parked arm go. -/
theorem hres_signal {Ψ Θ p ℓ a x t φ} {A : List Frame} (hA : OkE A) (ha : a = plug A (.hole p))
    (hx : ∀ {Θ'}, AnyTy S Ψ Θ' x)
    (h : Typ S Ψ Θ [] (.hres p ℓ a x) (.ex t φ)) :
    Typ S Ψ Θ [] (.hdone p (plug A x)) (.ex t φ) := by
  cases h with
  | hres _ hb hA' ha' hp hl hcp hφ hT =>
    obtain ⟨rfl, -⟩ := plug_hole_inj hA hA' (ha.symm.trans ha')
    have h₁ := refill_app hA' kont_not_value hp (kont_fn_rg hl hcp) (fun _ _ h l => h.trans l)
      fun T φ' hΘ hψ₀ _ _ => hx ((hb.cut hΘ.entails).sub (J' := .ex _ φ')
        ⟨LeLive.refl, hψ₀⟩)
    exact .hdone (h₁.sub ⟨hT, Stage.use_mono hφ (Caps.Sub.refl _)⟩)
  | errVal hv | allI hv => cases hv

/-- The arms of a new handler, in its run-time region `n`: the regions not yet created are
renamed out, and `n` is put for the region of `resume`. -/
theorem arms_body {Ψ Θ E al} {t' : Ty} {φ : Stage} {n : Nat} (hn : al.NoIns) (hb : al.Below n)
    (hΨ : ∀ u ∈ Ψ, u.YF n) (hidem : Θ.substT (kill n) = Θ)
    (h : Typ S (Ψ.map (Ty.up 1)) (Θ.substT (shiftV 1)) [] al
      (.harms E (t'.up 1) (φ.substT (shiftV 1)) (.tvar 0))) :
    Typ S Ψ Θ [] al (.harms E (t'.substT (kill n)) (φ.substT (kill n)) (.rg n)) := by
  have hΨ' : Ψ.map (Ty.substT (kill n)) = Ψ := by
    conv => rhs; rw [← List.map_id Ψ]
    exact List.map_congr_left fun u hu => hΨ u hu
  have h₁ := h.substT (liftT 1 (kill n)) ((Fits.kill hb).lift 1)
  simp only [Flick.J.substT, List.map_nil] at h₁
  rw [shift_comm, Facts.up_comm, Ty.up_comm, Stage.up_comm, hΨ', hidem] at h₁
  simp only [Ty.substT, liftT, if_true, Nat.zero_lt_one] at h₁
  have h₂ := h₁.substT (instT [.rg n]) (Fits.noIns hn fun _ => rfl)
  simp only [Flick.J.substT, List.map_map, Function.comp_def, Ty.up_inst1, List.map_id_fun', id,
    List.map_nil, Facts.up_inst1, Stage.up_substT _ 1 [_] rfl] at h₂
  simpa [Ty.substT, instT] using h₂

/-- A handler is installed in the region `n` with its cell at the end of the store. -/
theorem hdl_install {Ψ Θ E al rf e₀ T φ n t ψ C φ₀} {r ε : Row} {t' : Ty}
    (hal : Typ S (Ψ.map (Ty.up 1)) (Θ.substT (shiftV 1)) [] al
      (.harms E (t'.up 1) ((Stage.rt r ε).substT (shiftV 1)) (.tvar 0))) (hn : al.NoIns)
    (hrf : Typ S Ψ Θ [] rf (.ex (.arrow t ψ t' C) (.rt r ε)))
    (hψ : (Stage.use ψ C).le (.rt r ε))
    (he : Typ S Ψ Θ [] e₀ (.ex t φ₀))
    (hφ₀ : φ₀.le (.rt (Row.union (Row.single E) (r.back (Row.single E))) ε))
    (hOb : φ₀.ObOut (Row.single E)) (hc : S.Handles E al r) (hφ' : (Stage.rt r ε).le φ)
    (hT : LeLive t' T)
    (hbl : (Tm.hdl E al rf e₀).Below n) (hΨ : ∀ u ∈ Ψ, u.YF n) (hidem : Θ.substT (kill n) = Θ)
    (ht : T.YF n) (hφ : φ.substT (kill n) = φ) :
    ∃ c, c.YF n ∧ Typ S Ψ Facts.age [] (.lam .todo) (.ex c .pure) ∧
      Typ S (Ψ ++ [c]) Θ [] (.inside n (.hwith n Ψ.length E al rf e₀)) (.ex T φ) := by
  have hΨ' : Ψ.map (Ty.substT (kill n)) = Ψ := by
    conv => rhs; rw [← List.map_id Ψ]
    exact List.map_congr_left fun u hu => hΨ u hu
  have fk := Fits.kill hbl
  have h₁ := arms_body hn (fun p hp => hbl p (.inl hp)) hΨ hidem hal
  have h₂ := hrf.substT (kill n) (fk.sub fun _ h => .inr (.inl h))
  have h₃ := he.substT (kill n) (fk.sub fun _ h => .inr (.inr h))
  rw [hΨ', hidem] at h₂ h₃
  simp only [Flick.J.substT, Ty.substT, List.map_nil] at h₂ h₃
  let c : Ty := .arrow (.base .unit) (.rt (r.substT (kill n)) (ε.substT (kill n))) (t'.substT (kill n))
    Caps.none
  refine ⟨c, ?_, lam_todo, ?_⟩
  · have := Ty.yf_kill n (.arrow (.base .unit) (.rt r ε) t' Caps.none)
    simpa [Ty.substT, Stage.substT] using this
  · have hw : Typ S (Ψ ++ [c]) Θ [] (.hwith n Ψ.length E al rf e₀)
        (.ex (t'.substT (kill n))
          (.rt (Row.union (Row.scope (.rg n)) (r.substT (kill n))) (ε.substT (kill n)))) :=
      .hwith (by simp [c]) (h₁.weakenStore _) (h₂.weakenStore _)
        (by
          rw [← Stage.use_substT ψ C (kill n)]
          simpa [Stage.substT] using Stage.le_substT (kill n) hψ)
        (h₃.weakenStore _)
        (Stage.hP_of_out (Stage.le_back_substT (kill n) hφ₀)
          (by simpa [Row.single_substT] using hOb.substT (kill n)))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩
        (Stage.le_rt_sub (Stage.le_refl _) (fun _ h => Or.inr h)) .refl
        (Or.inl (by simp [Row.scope, Row.ofCaps]))
    refine .inside (F := Facts.none) (r := r.substT (kill n)) (ε := ε.substT (kill n))
      (fun _ _ h => h.elim) (hw.cut (Entails.union_left _ _))
      ⟨fun _ h => h.elim (fun hs => .inl hs) fun hr => .inr (.inl hr), fun _ h => h⟩
      (Ty.kill_fresh _ _) (Row.kill_fresh _ _)
      (by simpa [Stage.substT] using Stage.YF_kill hφ' hφ) ?_
    have := hT.substT (kill n); rwa [ht] at this

end Flick
