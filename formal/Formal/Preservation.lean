import Formal.PresHdlStep

/-!
# Preservation (core.md §4)

A step keeps the type and the stage, and grows the store typing. The stage is
kept exactly: (Sub) lifts a smaller one.
-/

namespace Flick

variable {S : Sig}

/-- `raise v` takes a runtime stage whose errors hold those of `v`. -/
theorem raise_stage {Ψ Θ e t φ} (h : Typ S Ψ Θ [] (.raise e) (.ex t φ)) :
    ∃ ε, (Stage.rt Row.none ε).le φ := by
  cases h with
  | raise _ hφ => exact ⟨_, hφ⟩
  | raiseAdt _ _ hφ => exact ⟨_, hφ⟩
  | errVal hv | allI hv => cases hv

theorem raise_not_pure {Ψ Θ e t} (h : Typ S Ψ Θ [] (.raise e) (.ex t .pure)) : False := by
  obtain ⟨_, hε⟩ := raise_stage h; exact hε

/-- The stage of an application of a constant whose type has a runtime stage. -/
theorem const_app_rt {Ψ Θ c w t} (h : Typ S Ψ Θ [] (.app (.const c) w) (.ex t .pure))
    (hc : ∀ ι, ∃ a r e b C, c.ty S ι = .arrow a (.rt r e) b C) : False := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | const _ hT' =>
      obtain ⟨_, _, _, _, _, hx⟩ := hc ‹_›
      rw [hx] at hT'
      obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
      cases h; exact Stage.le_trans lψ hψ
    | perform _ hT' =>
      obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
      cases h; exact Stage.le_trans lψ hψ
  | allI hv _ => cases hv with
    | constApp hc' _ =>
      cases ‹Const› <;> simp_all [Const.arity, Const.ty] <;> exact (hc default).elim
    | ctorApp _ hK _ => simp [Tm.head] at hK
  | errVal hv _ _ _ => cases hv with
    | constApp hc' _ =>
      cases ‹Const› <;> simp_all [Const.arity, Const.ty] <;> exact (hc default).elim
    | ctorApp _ hK _ => simp [Tm.head] at hK

theorem const_app2_rt {Ψ Θ c w₁ w₂ t} (h : Typ S Ψ Θ [] (.app (.app (.const c) w₁) w₂) (.ex t .pure))
    (hc : ∀ ι, ∃ a b r e d C C', c.ty S ι = .arrow a .pure (.arrow b (.rt r e) d C) C') : False := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | app hc' _ hψ₁ hT₁ =>
      cases hc' with
      | perform _ hT' =>
        obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
        cases h; exact Stage.le_trans lψ hψ₁
      | const _ hT' =>
        obtain ⟨_, _, _, _, _, _, _, hx⟩ := hc ‹_›
        rw [hx] at hT'
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h
        obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := (lt.trans hT₁).arrow_left
        cases h; exact Stage.le_trans lψ hψ
  | allI hv _ => cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK
  | errVal hv _ _ _ => cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK

theorem perform_arrow {E o c} {ι : CArg} : ∃ a r e b C, (Const.perform E o c).ty S ι = .arrow a (.rt r e) b C := by
  simp only [Const.ty]
  split <;> exact ⟨_, _, _, _, _, rfl⟩

/-- An operation takes a stage with its effect. -/
theorem perform_svc {Ψ Θ E o c w t φ} (h : Typ S Ψ Θ [] (.app (.const (.perform E o c)) w) (.ex t φ)) :
    φ.svc (.nm E) := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | perform _ hT' =>
      obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
      cases h
      exact Stage.le_svc (Stage.le_trans lψ hψ) _ rfl
    | const hok _ => exact hok.elim
  | errVal hv | allI hv => exact (perform_not_value hv).elim

/-- An operation that has a type is declared. -/
theorem perform_op' {Ψ Θ E o c w t φ} (h : Typ S Ψ Θ [] (.app (.const (.perform E o c)) w) (.ex t φ)) :
    ∃ a b, S.op E o = some (c, a, b) := by
  cases h with
  | app hf _ _ _ =>
    cases hf with
    | perform hop _ => exact ⟨_, _, hop⟩
    | const hok _ => exact hok.elim
  | errVal hv | allI hv => exact (perform_not_value hv).elim

/-- No runtime redex has a pure stage. -/
theorem RHead.not_pure {Ψ Θ Λ μ r t Λ' μ' r'} (h : Typ S Ψ Θ [] r (.ex t .pure))
    (hh : RHead Λ μ r Λ' μ' r') : False := by
  have nv := hh.not_value
  cases hh with
  | refMake => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | hctlRaise _ _ _ | hctlUnw _ _ => cases h with
    | hctl _ _ _ _ _ _ _ _ _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | hkarmRaise _ _ _ | hkarmUnw _ _ => cases h with
    | hkarm _ _ _ _ _ _ _ _ _ _ hle hφ _ => exact Stage.le_trans hle hφ
    | errVal hv | allI hv => exact nv hv
  | hresRaise _ _ _ | hresUnw _ _ => cases h with
    | hres _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | hwithCatch => cases h with
    | hwith _ _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | hdl => cases h with
    | hdl _ _ _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | refGet | scopeRun _ => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | open_ => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | op => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | refSet => exact const_app2_rt h fun _ => ⟨_, _, _, _, _, _, _, rfl⟩
  | await => cases h with
    | await _ hφ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | tryRaise => cases h with
    | tryR _ _ hφ => exact hφ
    | tryP hb => exact raise_not_pure hb
    | errVal hv | allI hv => exact nv hv
  | handleYes | handleNo | hguardYes | hguardNo | handleNil => cases h with
    | handle _ _ _ _ _ hφ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | scope => cases h with
    | scope _ _ _ _ hφ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | finRun | close => cases h with
    | inside _ _ _ _ _ hφ => exact hφ
    | errVal hv | allI hv => exact nv hv

/-- Removing a frame around `raise v` keeps the type of the hole. -/
theorem raise_frame {Ψ Θ v t φ} {F : Frame} (hF : F.ok) (hnt : ¬ F.isTry) (hni : ¬ F.isInside)
    (hv : Value v) (h : Typ S Ψ Θ [] (F.plug (.raise v)) (.ex t φ)) :
    Typ S Ψ Θ [] (.raise v) (.ex t φ) := by
  obtain ⟨k, Θ', _, φ', hr, rel, hk, _⟩ := frame_typing raise_not_value F hF h
  obtain ⟨ε, hv', hε⟩ := raise_inv hv hr
  have hne : φ' ≠ .pure := by intro h; subst h; exact hε
  obtain ⟨rfl, -, hΘ⟩ := hk.resolve_left hne
  obtain rfl := hΘ hni
  simp only [List.map_up_zero] at hv'
  obtain ⟨_, _, rfl, _, he⟩ := Stage.rt_le hε
  cases hφ : φ with
  | pure => exact (rel.rt hnt hne hφ).elim
  | rt r e =>
    subst hφ
    exact .raise (hv'.valueAny hv _) ⟨fun _ h => h.elim, fun n h => rel.err hnt n (he n h)⟩

theorem fin_not_value {c : Const} {w₁ w₂ : Tm} (hc : c = .addFin ∨ c = .spawn) :
    ¬ Value (.app (.app (.const c) w₁) w₂) := by
  intro hv
  rcases hc with rfl | rfl <;>
  · cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK

/-- Registering a finalizer or a child returns `()`. -/
theorem fin_unit {Ψ Θ c p w T φ} (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) : Typ S Ψ Θ [] (.lit .unit) (.ex T φ) := by
  cases h with
  | app hf _ _ hT =>
    cases hf with
    | app hc' _ _ hT₁ =>
      cases hc' with
      | perform => rcases hc with h | h <;> cases h
      | const _ hT' =>
        rcases hc with rfl | rfl <;>
        · obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
          cases h
          obtain ⟨_, _, _, _, h, _, _, lt', -⟩ := (lt.trans hT₁).arrow_left
          cases h; cases lt'; cases hT; exact .lit
  | errVal hv' | allI hv' => exact (fin_not_value hc hv').elim

/-- The hole of a registration is not pure. -/
theorem fin_not_pure {Ψ Θ c p w} (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T .pure))
    (hc : c = .addFin ∨ c = .spawn) : False := by
  rcases hc with rfl | rfl <;> exact const_app2_rt h fun _ => ⟨_, _, _, _, _, _, _, rfl⟩

/-- A context refilled after the regions not yet created are renamed out of the
whole derivation. -/
theorem refill_kill {Ψ K Θ' t' φ' E t φ r n X r'}
    (back : Refill S Ψ Facts.age K Θ' t' φ' (plug E) t φ r) (hb : (plug E r).Below n)
    (hΨ : ∀ u ∈ Ψ, u.YF n)
    (h' : Typ S ((Ψ ++ X).map (Ty.up K)) (Θ'.substT (liftT K (kill n))) [] r'
      (.ex (t'.substT (liftT K (kill n))) (φ'.substT (liftT K (kill n))))) :
    Typ S (Ψ ++ X) Facts.age [] (plug E r') (.ex (t.substT (kill n)) (φ.substT (kill n))) := by
  have := back (kill n) (Fits.kill hb) X r' (by rw [List.map_kill_YF hΨ]; exact h')
  rw [List.map_kill_YF hΨ] at this
  exact this.cut (Facts.age_kill n)

/-- The hole renamed with its context. -/
theorem hole_kill {Ψ : List Ty} {K : Nat} {Θ' : Facts} {r : Tm} {t' : Ty} {φ' : Stage} {n : Nat}
    {E : List Frame} (hr : Typ S (Ψ.map (Ty.up K)) Θ' [] r (.ex t' φ'))
    (hb : (plug E r).Below n) (hΨ : ∀ u ∈ Ψ, u.YF n) :
    Typ S (Ψ.map (Ty.up K)) (Θ'.substT (liftT K (kill n))) [] r
      (.ex (t'.substT (liftT K (kill n))) (φ'.substT (liftT K (kill n)))) := by
  have := hr.substT (liftT K (kill n)) ((Fits.kill (hb.sub fun _ h => plug_has E h)).lift K)
  rwa [shift_comm, List.map_kill_YF hΨ] at this

theorem Facts.kill_idem (Θ : Facts) (n : Nat) : (Θ.substT (kill n)).substT (kill n) = Θ.substT (kill n) := by
  rw [Facts.substT_comp]; congr 1; funext a; exact kill_kill n a

theorem Stage.kill_idem (φ : Stage) (n : Nat) : (φ.substT (kill n)).substT (kill n) = φ.substT (kill n) := by
  rw [Stage.substT_comp]; congr 1; funext a; exact kill_kill n a

/-- The facts of a hole, renamed: they extend those of creation and hold of it. -/
theorem RtCtx.hole {Ψ : List Ty} {Θ' : Facts} {n : Nat} (hΘ : Ext Facts.age Θ') (hΨ : ∀ u ∈ Ψ, u.YF n) :
    RtCtx Ψ (Θ'.substT (kill n)) n where
  inΘ := fun C s h => h.mono fun C' s' ⟨C₀, s₀, h₀, e₁, e₂⟩ => ⟨C₀, s₀, hΘ.1 _ _ h₀, e₁, e₂⟩
  outΘ := by
    refine Entails.of_age fun C s ⟨C₀, s₀, h₀, e₁, e₂⟩ => ?_
    subst e₁ e₂
    have hage : Facts.age C₀ s₀ := (hΘ.2 _ _ h₀).elim id id
    intro q p ⟨b, hb, hq⟩ hp
    obtain ⟨b', hb', hp'⟩ := (Ty.reach_substT _ _ _).mp hp
    obtain ⟨rfl, _⟩ := kill_reach hq
    obtain ⟨rfl, _⟩ := kill_reach hp'
    exact hage q p hb hb'
  idem := Facts.kill_idem _ _
  yf := hΨ

/-- A global steps to its body, typed at each instance of its scheme. -/
theorem glob_preservation {root Ψ Θ g T φ v} (hroot : RootTy S root)
    (h : Typ S Ψ Θ [] (.glob g) (.ex T φ)) (hg : root.glob g = some v) : Typ S Ψ Θ [] v (.ex T φ) := by
  cases h with
  | glob hs hts hH hT =>
    obtain ⟨hno, _, hs', ht⟩ := hroot.glob _ _ hg
    rw [hs] at hs'; cases hs'
    refine (Typ.instantiate (Γ := []) hts hH (hno _) ?_).sub ⟨hT, Stage.pure_le _⟩
    exact (ht.weakenStore _).cut fun _ _ h => h.mono fun _ _ h => .inr h
  | errVal hv | allI hv => cases hv

/-- A root writes no region: its type names none. -/
theorem root_no_rgn {root : Roots} (hroot : RootTy S root) {N v} (h : root.svc N = some v) (p : Nat) :
    ¬ v.Has (.rgn p) := fun hp =>
  S.impl_reach N _ (((hroot.svc N v h).2.holds (hroot.svc N v h).1 rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))

/-- A root's function for an operation has the type of the operation. -/
theorem root_field {root Ψ Θ Ef o w f c v t φ} (hroot : RootTy S root) (hr₀ : root.svc Ef = some w)
    (hf : w.field o = some f)
    (h : Typ S Ψ Θ [] (.app (.const (.perform Ef o c)) v) (.ex t φ)) :
    Typ S Ψ Θ [] (.app f v) (.ex t φ) := by
  cases h with
  | app hf' hx hψ hT =>
    cases hf' with
    | perform hop hT' =>
      obtain ⟨hwv, hwt⟩ := hroot.svc _ _ hr₀
      have hne : S.impl Ef ≠ .errors Row.none := fun he => by
        rw [he] at hwt
        obtain ⟨_, _, _, _, _, _, _, hN, _⟩ := canonical_errors hwv hwt
        exact hN.elim
      have hc : c = .tail := Classical.byContradiction fun hc => hne (S.op_impl _ _ _ _ _ hop hc)
      subst hc
      have ft := field_typ hwt hf (S.op_field _ _ _ _ hop hne)
      have ft' := (ft.cut (Entails.none Θ)).weakenStore Ψ
      simp only [List.nil_append] at ft'
      obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
      cases h
      simp only [reduceCtorEq, if_false] at lt
      exact .app (ft'.sub ⟨.refl, Stage.pure_le _⟩) (hx.sub ⟨la, Stage.le_refl _⟩)
        (Stage.none_le (Stage.rt_ne_pure (Stage.le_trans lψ hψ))) (lt.trans hT)
    | const hok _ => exact hok.elim
  | errVal hv' | allI hv' => exact (perform_not_value hv').elim

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
