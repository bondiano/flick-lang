import Formal.PresSite

/-!
# Preservation (core.md §4)

A step keeps the type and the stage, and grows the store typing. The stage is
kept exactly: (Sub) lifts a smaller one.
-/

namespace Flick

variable {S : Sig}

theorem preservation {root Ψ Λ μ e t φ l Λ' μ' e'} (hroot : RootTy S root)
    (h : Typ S Ψ Facts.age [] e (.ex t φ)) (hμ : StoreTy S Ψ μ) (hbl : e.Below Λ.length)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∃ X, StoreTy S (Ψ ++ X) μ' ∧
      Typ S (Ψ ++ X) Facts.age [] e' (.ex (t.substT (kill Λ.length)) (φ.substT (kill Λ.length))) ∧
      ∀ u ∈ X, u.YF Λ.length := by
  cases hs with
  | pure hE hh =>
    obtain ⟨_, _, _, _, hr, _, _, back⟩ := ctx_typing _ hE hh.not_value h
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by
      simpa using pure_preservation (hole_kill hr hbl hΨ) hh), by simp⟩
  | rt hE hh =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE hh.not_value h
    obtain ⟨rfl, hΘ⟩ := hK.resolve_left fun hp => by subst hp; exact hh.not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    obtain ⟨X, hμ', hr'', hX⟩ := rt_preservation hr' hμ (RtCtx.hole hΘ hΨ)
      (hbl.sub fun _ h => plug_has _ h) hμb (Ty.yf_kill _ _) (Stage.kill_idem _ _) hh
    exact ⟨X, hμ', refill_kill back hbl hΨ (by simpa using hr''), hX⟩
  | performRoot hE _ hr₀ hf hv =>
    obtain ⟨K, Θ', t₁, φ₁, hr, _, hK, back⟩ := ctx_typing _ hE perform_not_value h
    obtain ⟨rfl, -⟩ := hK.resolve_left fun hp => by subst hp; exact const_app_rt hr fun _ => perform_arrow
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using root_field hroot hr₀ hf hr'),
      by simp⟩
  | glob hE hg =>
    obtain ⟨_, _, _, _, hr, _, _, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by
      simpa using glob_preservation hroot (hole_kill hr hbl hΨ) hg), by simp⟩
  | unw hE hF hni _ _ =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (not_value_plug unw_not_value _) h
    obtain ⟨rfl, -⟩ := hK.resolve_left fun hp => by
      subst hp; exact unw_not_pure (unw_frame hF hni hr)
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using unw_frame hF hni hr'), by simp⟩
  | performAbort hE hEi _ ha hv =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    obtain ⟨rfl, -⟩ := hK.resolve_left fun hp => by subst hp; exact hwith_not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using abort_pres hEi ha hv hr'),
      by simp⟩
  | performTail hE hEi _ ha hv =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    obtain ⟨rfl, hΘ⟩ := hK.resolve_left fun hp => by subst hp; exact hwith_not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by
      simpa using tail_pres (RtCtx.hole hΘ hΨ) (hbl.sub fun _ h => plug_has _ (by
        simp only [Tm.Has]; exact .inr (.inr (.inr (.inr (plug_has _ (.inr h))))))) hEi ha hv hr'),
      by simp⟩
  | performKept hE hEi _ ha hv _ =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    obtain ⟨rfl, hΘ⟩ := hK.resolve_left fun hp => by subst hp; exact hkeep_not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by
      simpa using kept_pres (RtCtx.hole hΘ hΨ) (hbl.sub fun _ h => plug_has _ (by
        simp only [Tm.Has]; exact .inr (.inr (.inr (plug_has _ (.inr h)))))) hEi ha hv hr'),
      by simp⟩
  | performCtl hE hEi _ ha hv =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    obtain ⟨rfl, hΘ⟩ := hK.resolve_left fun hp => by subst hp; exact hwith_not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    obtain ⟨c, hcy, hcl, ht'⟩ := ctl_pres (RtCtx.hole hΘ hΨ) (hbl.sub fun _ h => plug_has _ (by
        simp only [Tm.Has]; exact .inr (.inr (.inr (.inr (plug_has _ (.inr h)))))))
      (hbl _ ((plug_has _ (by simp [Tm.Has])))) hEi ha hv hr'
    exact ⟨[c], hμ.alloc .lam hcl, refill_kill back hbl hΨ (by rw [hμ.1]; simpa using ht'),
      by simpa using hcy⟩
  | resume hE hA hEi hk hn hu =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    obtain ⟨rfl, hΘ⟩ := hK.resolve_left fun hp => by subst hp; exact hctl_not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by
      simpa using resume_pres (RtCtx.hole hΘ hΨ) (hbl.sub fun _ h => plug_has _ (by
        simp only [Tm.Has]; exact .inr (.inr (.inr (.inr (.inl (plug_has _ (.inr h)))))))) hA hEi hk hu hr'),
      by simp⟩
  | raise hE hF hnt hni _ hv =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ :=
      ctx_typing _ hE (not_value_plug raise_not_value _) h
    obtain ⟨rfl, -⟩ := hK.resolve_left fun hp => by
      subst hp
      obtain ⟨_, _, _, _, hr', rel', _⟩ := frame_typing raise_not_value _ hF hr
      obtain rfl := rel'.pure rfl
      exact raise_not_pure hr'
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using raise_frame hF hnt hni hv hr'),
      by simp⟩
  | addFin hE _ _ _ =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fin_not_value (.inl rfl)) h
    obtain ⟨rfl, -⟩ := hK.resolve_left fun hp => by subst hp; exact fin_not_pure hr (.inl rfl)
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using fin_unit hr' (.inl rfl)),
      by simp⟩
  | spawn hE _ _ _ =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fin_not_value (.inr rfl)) h
    obtain ⟨rfl, -⟩ := hK.resolve_left fun hp => by subst hp; exact fin_not_pure hr (.inr rfl)
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using fin_unit hr' (.inr rfl)),
      by simp⟩

end Flick
