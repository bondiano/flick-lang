import Formal.HotSwap.Compat

/-!
# Typing transfer across compatible hot swaps (Theorem 1)
-/

namespace Flick.HotSwap

/-! ## 3. Transferring typing derivations across compatible swaps -/

theorem leLive_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {a b : Ty}
    (h : LeLive S a b) : LeLive S' a b := by
  induction h with
  | refl => exact .refl
  | equiv he => exact .equiv he
  | arrow _ hφ _ hrc ih₁ ih₂ => exact .arrow ih₁ hφ ih₂ hrc
  | eff hφ _ hrc ih => exact .eff hφ ih hrc
  | rcons _ _ hrc ih₁ ih₂ => exact .rcons ih₁ ih₂ hrc
  | hide hi hl he hr ha hb =>
    exact .hide (hc.hidden.symm ▸ hi) hl he hr ha hb
  | data hl hr _ _ hinv ha hb ihp ihn =>
    exact .data hl hr
      (fun j u u' hu hu' hh hv => ihp j u u' hu hu' (by rwa [hc.hidden] at hh) (by rwa [hc.var] at hv))
      (fun j u u' hu hu' hh hv => ihn j u u' hu hu' (by rwa [hc.hidden] at hh) (by rwa [hc.var] at hv))
      (fun j u u' hu hu' hh hv => hinv j u u' hu hu' (by rwa [hc.hidden] at hh) (by rwa [hc.var] at hv))
      ha hb
  | dict hl _ _ hnr hinv ihp ihn =>
    exact .dict hl
      (fun j u u' hu hu' hv => ihp j u u' hu hu' (by rwa [hc.dictVar] at hv))
      (fun j u u' hu hu' hv => ihn j u u' hu hu' (by rwa [hc.dictVar] at hv))
      (fun j u u' hu hu' hv => hnr j u u' hu hu' (by rwa [hc.dictVar] at hv))
      (fun j u u' hu hu' hv => hinv j u u' hu hu' (by rwa [hc.dictVar] at hv))

theorem leLiveI_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {a b : Ty}
    (h : LeLiveI S a b) : LeLiveI S' a b := by
  induction h with
  | le h => exact .le (leLive_of_swapCompat hc h)
  | inst hus _ ih => exact .inst hus ih

theorem patTy_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {b p t Δ}
    (h : PatTy S b p t Δ) : PatTy S' b p t Δ := by
  induction h with
  | conv _ he ih => exact .conv ih he
  | wild => exact .wild
  | var => exact .var
  | lit => exact .lit
  | head hK hinj hk =>
    have h' : PatTy S' false (.ctor _) (Sig.ctorTyR _ _ (S'.res _) _) [] :=
      PatTy.head (hc.ctor.symm ▸ hK) (by rw [hc.res]; exact hinj) hk
    simpa [hc.res] using h'
  | app _ _ ih₁ ih₂ => exact .app ih₁ ih₂
  | adt _ hU ih => exact .adt ih hU
  | rany => exact .rany
  | rfield hf _ _ ih₁ ih₂ => exact .rfield hf ih₁ ih₂
  | or _ _ ih₁ ih₂ => exact .or ih₁ ih₂
  | err _ hε ih => exact .err ih hε

theorem patsTy_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {qs ts Δ}
    (h : PatsTy S qs ts Δ) : PatsTy S' qs ts Δ := by
  induction h with
  | nil => exact .nil
  | cons hp _ ih => exact .cons (patTy_of_swapCompat hc hp) ih

theorem shp_of_swapCompat_rev {S S' : Sig} (hc : SigSwapCompat S S') {b t v}
    (h : Shp S' b t v) : Shp S b t v := by
  induction h with
  | hidden ho => exact .hidden ho
  | lit => exact .lit
  | rnil => exact .rnil
  | rcons _ _ ih₁ ih₂ => exact .rcons ih₁ ih₂
  | head hK =>
    rw [hc.ctor] at hK
    have h' := Shp.head (S := S) hK
    simpa [hc.res] using h'
  | app _ _ ih₁ ih₂ => exact .app ih₁ ih₂
  | adt _ hU hT hb ih =>
    refine .adt ih hU hT fun K hK => ?_
    intro k ts T us hctor ht hlen
    obtain ⟨ρ, hρ⟩ := hb K hK k ts T us (hc.ctor.symm ▸ hctor) ht (hc.res.symm ▸ hlen)
    exact ⟨ρ, by simpa [hc.res] using hρ⟩
  | err _ hU hε ih => exact .err ih hU hε
  | errOpen ho => exact .errOpen ho

theorem exh_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {t ps}
    (h : Exh S t ps) : Exh S' t ps :=
  fun v hv => h v (shp_of_swapCompat_rev hc hv)

theorem covers_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {t ps e}
    (h : Covers S t ps e) : Covers S' t ps e :=
  h.imp (exh_of_swapCompat hc) id

theorem hpat_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {εb p θ Δ}
    (h : HPat S εb p θ Δ) : HPat S' εb p θ Δ := by
  cases h with
  | wild => exact .wild
  | var => exact .var
  | ctor hp => exact .ctor (patTy_of_swapCompat hc hp)

theorem catches_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {εb p θ x}
    (h : Catches S εb p θ x) : Catches S' εb p θ x := by
  cases x with
  | nm n c => exact h.imp id fun ⟨h1, h2⟩ => ⟨h1, exh_of_swapCompat hc h2⟩
  | _ => exact h

theorem answers_eq_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') (E : Name) (us : List Ty) :
    S'.answers E us = S.answers E us := by
  funext c
  simp [Sig.answers, hc.arity]

theorem handles_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {E us al r}
    (h : S.Handles E us al r) : S'.Handles E us al r := by
  refine ⟨by rw [hc.op]; exact h.1, fun hnot => h.2 fun hcomp => hnot fun o c a b ho => ?_⟩
  rw [hc.op] at ho
  exact hcomp o c a b ho

/-- Core transfer theorem: every typing derivation under `S` remains valid
under any compatible signature `S'`. -/
theorem typ_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {Ψ Θ Γ e J}
    (h : Typ S Ψ Θ Γ e J) : Typ S' Ψ Θ Γ e J := by
  induction h with
  | var hi hts hH hT => exact .var hi hts hH (leLiveI_of_swapCompat hc hT)
  | allI hv _ ih => exact .allI hv ih
  | allVar hi hk hcs => exact .allVar hi hk hcs
  | lam _ hn hT ih => exact .lam ih hn (leLive_of_swapCompat hc hT)
  | app _ _ hψ hT ih₁ ih₂ => exact .app ih₁ ih₂ hψ (leLive_of_swapCompat hc hT)
  | let_ _ _ ih₁ ih₂ => exact .let_ ih₁ ih₂
  | letGen _ _ ih₁ ih₂ => exact .letGen ih₁ ih₂
  | letrec _ hn _ ih₁ ih₂ => exact .letrec ih₁ hn ih₂
  | fix _ hn hT ih => exact .fix ih hn (leLive_of_swapCompat hc hT)
  | lit => exact .lit
  | loc hl => exact .loc hl
  | const hok hT => exact .const hok (leLive_of_swapCompat hc hT)
  | ctor hK hk hH hT =>
    exact .ctor (hc.ctor.symm ▸ hK) hk (by rw [hc.res]; exact hH)
      (by rw [hc.res]; exact leLive_of_swapCompat hc hT)
  | rnil => exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => exact .rcons hR hl ih₁ ih₂
  | proj _ hf hT ih => exact .proj ih hf (leLive_of_swapCompat hc hT)
  | update _ hf _ ih₁ ih₂ => exact .update ih₁ hf ih₂
  | match_ _ _ hcov ih₁ ih₂ => exact .match_ ih₁ ih₂ (covers_of_swapCompat hc hcov)
  | anil => exact .anil
  | acons hp _ _ ih₁ ih₂ => exact .acons (patTy_of_swapCompat hc hp) ih₁ ih₂
  | aguard hp _ _ _ ih₁ ih₂ ih₃ => exact .aguard (patTy_of_swapCompat hc hp) ih₁ ih₂ ih₃
  | aref hK hU hn hfr hlc hpA _ _ ihA ihr =>
    exact .aref (hc.ctor.symm ▸ hK) hU hn hfr hlc
      (fun θ hθ hu => patsTy_of_swapCompat hc (hpA θ hθ (by simpa [hc.res] using hu)))
      (fun θ hθ hu => ihA θ hθ (by simpa [hc.res] using hu)) ihr
  | todo => exact .todo
  | task _ hψ hn hT ih => exact .task ih hψ hn (leLive_of_swapCompat hc hT)
  | await _ hφ hT ih => exact .await ih hφ (leLive_of_swapCompat hc hT)
  | lazy _ hψ hn hφ ih => exact .lazy ih hψ hn hφ
  | force _ hφ hT ih => exact .force ih hφ (leLive_of_swapCompat hc hT)
  | raise _ hφ ih => exact .raise ih hφ
  | raiseAdt _ hU hφ ih => exact .raiseAdt ih hU hφ
  | errVal _ hU hE ih => exact .errVal ih hU hE
  | funcRef _ hcl ih => exact .funcRef ih hcl
  | tryR _ _ hφ hsub ih₁ ih₂ => exact .tryR ih₁ ih₂ hφ hsub
  | tryP _ _ ih₁ ih₂ => exact .tryP ih₁ ih₂
  | handle hv _ hU hE _ hcov hall hφ ih₁ ih₂ => exact .handle hv ih₁ hU hE ih₂ hcov hall hφ
  | hnil hH => exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    exact .hcons (hpat_of_swapCompat hc hp) ih₁ ih₂ fun n hn =>
      (hH n hn).imp (catches_of_swapCompat hc) id
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ => exact .hguard (hpat_of_swapCompat hc hp) ih₁ ih₂ ih₃ hH
  | scope hn hF _ hφ' hφ hT ih => exact .scope hn hF ih hφ' hφ (leLive_of_swapCompat hc hT)
  | inside hF _ hφ' ht hε hφ hT ih => exact .inside hF ih hφ' ht hε hφ (leLive_of_swapCompat hc hT)
  | cap => exact .cap
  | own => exact .own
  | drec hdict hk _ hn hT ih =>
    exact .drec (hc.dict.symm ▸ hdict) hk ih hn (leLive_of_swapCompat hc hT)
  | dnil => exact .dnil
  | dcons _ _ ih₁ ih₂ => exact .dcons ih₁ ih₂
  | dproj _ hdict hk hl hts hH hT ih =>
    exact .dproj ih (hc.dict.symm ▸ hdict) hk hl hts hH (leLive_of_swapCompat hc hT)
  | glob hg hts hH hT =>
    obtain ⟨σ_new, hg', hsub⟩ := hc.glob _ _ hg
    obtain ⟨us, hus, hH', hle⟩ := hsub _ _ hts hH
    exact .glob hg' hus hH' (leLive_of_swapCompat hc (hle.trans hT))
  | perform hus hop hT =>
    exact .perform (hc.arity.symm ▸ hus) (hc.op.symm ▸ hop) (leLive_of_swapCompat hc hT)
  | hanil => exact .hanil
  | hatail hus hop _ _ ih₁ ih₂ =>
    exact .hatail (hc.arity.symm ▸ hus) (hc.op.symm ▸ hop) ih₁ ih₂
  | haabort hus hop _ _ ih₁ ih₂ =>
    exact .haabort (hc.arity.symm ▸ hus) (hc.op.symm ▸ hop) ih₁ ih₂
  | hactl hus hop _ _ ih₁ ih₂ =>
    exact .hactl (hc.arity.symm ▸ hus) (hc.op.symm ▸ hop) ih₁ ih₂
  | hdl hus _ hn _ hψ _ hφ₀ hOb hhand hφ hT ih₁ ih₂ ih₃ =>
    exact .hdl (hc.arity.symm ▸ hus) ih₁ hn ih₂ hψ ih₃
      (by rw [answers_eq_of_swapCompat hc]; exact hφ₀) hOb
      (handles_of_swapCompat hc hhand) hφ (leLive_of_swapCompat hc hT)
  | hwith hus hℓ _ _ hψ _ hφ₀ hhand hφ hT hat ih₁ ih₂ ih₃ =>
    exact .hwith (hc.arity.symm ▸ hus) hℓ ih₁ ih₂ hψ ih₃
      (by rw [answers_eq_of_swapCompat hc]; exact hφ₀)
      (handles_of_swapCompat hc hhand) hφ (leLive_of_swapCompat hc hT) hat
  | hctl hus hℓ _ _ hψ hop hcab htl hE hk hno _ hφ₀ _ hκ hhand hφ hT hat ih₁ ih₂ ih₃ ih₄ =>
    exact .hctl (hc.arity.symm ▸ hus) hℓ ih₁ ih₂ hψ (hc.op.symm ▸ hop) hcab htl hE hk hno ih₃
      (by rw [answers_eq_of_swapCompat hc]; exact hφ₀) ih₄ hκ
      (handles_of_swapCompat hc hhand) hφ (leLive_of_swapCompat hc hT) hat
  | hres hℓ _ hA ha _ hκ hC hφ hT ih₁ ih₂ =>
    exact .hres hℓ ih₁ hA ha ih₂ hκ hC hφ (leLive_of_swapCompat hc hT)
  | hdone _ ih => exact .hdone ih
  | unw hℓ _ hφ ih => exact .unw hℓ ih hφ
  | hkeep hus hℓ _ _ hφ₀ hhand hle hφ hT ih₁ ih₂ =>
    exact .hkeep (hc.arity.symm ▸ hus) hℓ ih₁ ih₂
      (by rw [answers_eq_of_swapCompat hc]; exact hφ₀)
      (handles_of_swapCompat hc hhand) hle hφ (leLive_of_swapCompat hc hT)
  | hkarm hus hℓ _ hop hE hk hno _ hφ₀ hhand _ hle hφ hT ih₁ ih₂ ih₃ =>
    exact .hkarm (hc.arity.symm ▸ hus) hℓ ih₁ (hc.op.symm ▸ hop) hE hk hno ih₂
      (by rw [answers_eq_of_swapCompat hc]; exact hφ₀)
      (handles_of_swapCompat hc hhand) ih₃ hle hφ (leLive_of_swapCompat hc hT)
  | kont hl hC hT => exact .kont hl hC (leLive_of_swapCompat hc hT)
  | lazyLoc hl => exact .lazyLoc hl
  | settle hl _ hψ hφ hT ih => exact .settle hl ih hψ hφ (leLive_of_swapCompat hc hT)
  | conv _ he ih => exact .conv ih he

theorem cellTy_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {Ψ r ε t C c}
    (h : CellTy S Ψ r ε t C c) : CellTy S' Ψ r ε t C c := by
  cases h with
  | pending ht hψ hn => exact .pending (typ_of_swapCompat hc ht) hψ hn
  | running ht hψ hn => exact .running (typ_of_swapCompat hc ht) hψ hn
  | ok hv ht => exact .ok hv (typ_of_swapCompat hc ht)
  | err hv ht => exact .err hv (typ_of_swapCompat hc ht)
  | defect => exact .defect

theorem storeTy_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {Ψ μ}
    (h : StoreTy S Ψ μ) : StoreTy S' Ψ μ := by
  refine ⟨h.1, fun i v t hv ht => ?_⟩
  rcases h.2 i v t hv ht with ⟨hval, htyp⟩ | ⟨r, ε, a, C, hEq, hcell⟩
  · exact .inl ⟨hval, typ_of_swapCompat hc htyp⟩
  · exact .inr ⟨r, ε, a, C, hEq, cellTy_of_swapCompat hc hcell⟩

theorem rgsTy_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {Ψ Λ}
    (h : RgsTy S Ψ Λ) : RgsTy S' Ψ Λ := by
  intro p g hg
  obtain ⟨hk, hf⟩ := h p g hg
  exact ⟨fun k hkm => let ⟨t, φ, ht⟩ := hk k hkm; ⟨t, φ, typ_of_swapCompat hc ht⟩,
         fun f hfm => let ⟨t, φ, ht⟩ := hf f hfm; ⟨t, φ, typ_of_swapCompat hc ht⟩⟩

theorem cfgTy_of_swapCompat {S S' : Sig} (hc : SigSwapCompat S S') {Ψ c t φ}
    (h : CfgTy S Ψ c t φ) : CfgTy S' Ψ c t φ := by
  obtain ⟨hμ, hm, hΛ, hb, hΨ, ha, hs, cell, hcg⟩ := h
  exact ⟨storeTy_of_swapCompat hc hμ, typ_of_swapCompat hc hm,
         rgsTy_of_swapCompat hc hΛ, hb, hΨ, ha, hs, cell, hcg⟩

/-- Replacing a global cell `g` with a closed value `v` typed at a subsuming
scheme `σ_new` preserves `RootTy`. -/
theorem rootTy_swap_glob {S : Sig} {root : Roots} (hroot : RootTy S root)
    (g : Nat) (σ_new : Scheme) (hclosed : σ_new.Closed σ_new.k)
    (hsub : ∀ σ_old, S.glob g = some σ_old → SchemeSubsumes S σ_new σ_old)
    (v : Tm) (hvClosed : ∀ x, ¬ v.Has x)
    (hvTyp : Typ (sigWithGlob S g σ_new hclosed) [] σ_new.cs [] v (.ex σ_new.t .pure)) :
    RootTy (sigWithGlob S g σ_new hclosed) (rootsWithGlob root g v) := by
  have hc := sigWithGlob_compat S g σ_new hclosed hsub
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro N w hw
    obtain ⟨hval, htyp⟩ := hroot.svc N w hw
    exact ⟨hval, by rw [hc.impl]; exact typ_of_swapCompat hc htyp⟩
  · intro g' w hw
    by_cases h : g' = g
    · subst h
      simp only [rootsWithGlob, ↓reduceIte, Option.some.injEq] at hw
      subst hw
      exact ⟨hvClosed, σ_new, by simp [sigWithGlob], hvTyp⟩
    · simp only [rootsWithGlob, h, ↓reduceIte] at hw
      obtain ⟨hno, s, hs, ht⟩ := hroot.glob g' w hw
      exact ⟨hno, s, by simp [sigWithGlob, h, hs], typ_of_swapCompat hc ht⟩
  · intro g' s hs
    by_cases h : g' = g
    · subst h; simp [rootsWithGlob]
    · simp only [sigWithGlob, h, ↓reduceIte] at hs
      simp only [rootsWithGlob, h, ↓reduceIte]
      exact hroot.globs g' s hs
  · exact hroot.svcBelow

/-- **Theorem 1 (Preservation under a compatible swap).**
Replacing a global cell `g` with a closed value `v` of a compatible (subsuming)
scheme `σ_new` preserves both `RootTy` and `CfgTy` on the running configuration
`c` at the exact same store typing `Ψ`, result type `t`, and stage `φ`. -/
theorem swap_preserves_cfgTy {S : Sig} {root : Roots} {Ψ c t φ}
    (hroot : RootTy S root) (hc : CfgTy S Ψ c t φ)
    (g : Nat) (σ_new : Scheme) (hclosed : σ_new.Closed σ_new.k)
    (hsub : ∀ σ_old, S.glob g = some σ_old → SchemeSubsumes S σ_new σ_old)
    (v : Tm) (hvClosed : ∀ x, ¬ v.Has x)
    (hvTyp : Typ (sigWithGlob S g σ_new hclosed) [] σ_new.cs [] v (.ex σ_new.t .pure)) :
    RootTy (sigWithGlob S g σ_new hclosed) (rootsWithGlob root g v) ∧
      CfgTy (sigWithGlob S g σ_new hclosed) Ψ c t φ :=
  ⟨rootTy_swap_glob hroot g σ_new hclosed hsub v hvClosed hvTyp,
   cfgTy_of_swapCompat (sigWithGlob_compat S g σ_new hclosed hsub) hc⟩

/-- **Two-version dispatch under a swap:**
after swapping global `g` to `v_new`,
(1) any already-entered frame holding captured old code `e_old` remains well
typed under the swapped signature `S'` without changing `e_old`, while
(2) any fresh reference `plug E (.glob g)` steps to `plug E v_new`. -/
theorem swap_old_frame_and_new_call {S : Sig} {root : Roots} {Ψ Θ Γ Λ μ E e_old T φ}
    (g : Nat) (σ_new : Scheme) (hclosed : σ_new.Closed σ_new.k)
    (hsub : ∀ σ_old, S.glob g = some σ_old → SchemeSubsumes S σ_new σ_old)
    (v_new : Tm) (hE : OkE E)
    (hOld : Typ S Ψ Θ Γ (plug E e_old) (.ex T φ)) :
    Typ (sigWithGlob S g σ_new hclosed) Ψ Θ Γ (plug E e_old) (.ex T φ) ∧
      Step (rootsWithGlob root g v_new) .pure Λ μ (plug E (.glob g)) (.cfg Λ μ (plug E v_new)) :=
  ⟨typ_of_swapCompat (sigWithGlob_compat S g σ_new hclosed hsub) hOld,
   .glob hE (by simp [rootsWithGlob])⟩


end Flick.HotSwap
