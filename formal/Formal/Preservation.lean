import Formal.Below

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
  | errVal hv => cases hv

/-- `ask_S ()` takes a stage with `S`, and its type is above `S.Impl`. -/
theorem ask_inv {Ψ Θ N w t φ} (h : Typ S Ψ Θ [] (.app (.const (.ask N)) w) (.ex t φ)) :
    φ.svc (.nm N) ∧ Le (S.impl N) t := by
  cases h with
  | app hf _ hψ hT =>
    cases hf with
    | const _ hT' =>
      obtain ⟨_, _, _, _, h, _, lψ, lt, -⟩ := hT'.arrow_left
      cases h
      exact ⟨Stage.le_svc (Stage.le_trans lψ hψ) _ rfl, lt.trans hT⟩
  | errVal hv => exact (ask_not_value hv).elim

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
  | errVal hv _ _ _ => cases hv with
    | constApp hc' _ =>
      cases ‹Const› <;> simp_all [Const.arity, Const.layer, Const.ty] <;> exact (hc default).elim
    | ctorApp _ hK _ => simp [Tm.head] at hK

theorem const_app2_rt {Ψ Θ c w₁ w₂ t} (h : Typ S Ψ Θ [] (.app (.app (.const c) w₁) w₂) (.ex t .pure))
    (hc : ∀ ι, ∃ a b r e d C C', c.ty S ι = .arrow a .pure (.arrow b (.rt r e) d C) C') : False := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | app hc' _ _ hT₁ =>
      cases hc' with
      | const _ hT' =>
        obtain ⟨_, _, _, _, _, _, _, hx⟩ := hc ‹_›
        rw [hx] at hT'
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h
        obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := (lt.trans hT₁).arrow_left
        cases h; exact Stage.le_trans lψ hψ
  | errVal hv _ _ _ => cases hv with
    | constApp2 =>
      cases ‹Const› <;> simp_all [Const.arity, Const.layer, Const.ty] <;> exact (hc default).elim
    | ctorApp _ hK _ => simp [Tm.head] at hK

/-- No runtime redex has a pure stage. -/
theorem RHead.not_pure {Ψ Θ Λ μ r t Λ' μ' r'} (h : Typ S Ψ Θ [] r (.ex t .pure))
    (hh : RHead Λ μ r Λ' μ' r') : False := by
  have nv := hh.not_value
  cases hh with
  | refMake => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | refGet => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | open_ => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | op => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | refSet => exact const_app2_rt h fun _ => ⟨_, _, _, _, _, _, _, rfl⟩
  | await => cases h with
    | await _ hφ => exact hφ
    | errVal hv => exact nv hv
  | build => cases h with
    | provideL _ _ _ _ hφ => exact hφ
    | provideE _ _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv
  | provideEnv => cases h with
    | provideL _ _ _ _ hφ => exact hφ
    | provideE _ _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv
  | tryRaise => cases h with
    | tryR _ _ hφ => exact hφ
    | tryP hb => exact raise_not_pure hb
    | errVal hv => exact nv hv
  | handleYes | handleNo | hguardYes | hguardNo | handleNil => cases h with
    | handle _ _ _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv
  | scope => cases h with
    | scope _ _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv
  | finRun | close => cases h with
    | inside _ _ _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv

/-- Removing a frame around `raise v` keeps the type of the hole. -/
theorem raise_frame {Ψ Θ v t φ} {F : Frame} (hF : F.ok) (hnt : ¬ F.isTry) (hni : ¬ F.isInside)
    (hv : Value v) (h : Typ S Ψ Θ [] (F.plug (.raise v)) (.ex t φ)) :
    Typ S Ψ Θ [] (.raise v) (.ex t φ) := by
  obtain ⟨k, Θ', _, φ', hr, rel, hk, _, _⟩ := frame_typing raise_not_value F hF h
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

theorem List.map_kill_YF {Ψ : List Ty} {n : Nat} (h : ∀ u ∈ Ψ, u.YF n) :
    Ψ.map (Ty.substT (kill n)) = Ψ := by
  conv => rhs; rw [← List.map_id Ψ]
  exact List.map_congr_left fun u hu => h u hu

theorem fin_not_value {c : Const} {w₁ w₂ : Tm} (hc : c = .addFin ∨ c = .spawn) :
    ¬ Value (.app (.app (.const c) w₁) w₂) := by
  intro hv
  rcases hc with rfl | rfl <;>
  · cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK
    | constApp2 _ hl => simp [Const.layer] at hl

/-- Registering a finalizer or a child returns `()`. -/
theorem fin_unit {Ψ Θ c p w T φ} (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) : Typ S Ψ Θ [] (.lit .unit) (.ex T φ) := by
  cases h with
  | app hf _ _ hT =>
    cases hf with
    | app hc' _ _ hT₁ =>
      cases hc' with
      | const _ hT' =>
        rcases hc with rfl | rfl <;>
        · obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
          cases h
          obtain ⟨_, _, _, _, h, _, _, lt', -⟩ := (lt.trans hT₁).arrow_left
          cases h; cases lt'; cases hT; exact .lit
  | errVal hv' => exact (fin_not_value hc hv').elim

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
  | errVal hv => cases hv

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
    obtain ⟨rfl, hΘ, -⟩ := hK.resolve_left fun hp => by subst hp; exact hh.not_pure hr
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    obtain ⟨X, hμ', hr'', hX⟩ := rt_preservation hr' hμ (RtCtx.hole hΘ hΨ)
      (hbl.sub fun _ h => plug_has _ h) hμb (Ty.yf_kill _ _) (Stage.kill_idem _ _) hh
    exact ⟨X, hμ', refill_kill back hbl hΨ (by simpa using hr''), hX⟩
  | askWith hE hl =>
    obtain ⟨K, Θ', t₁, φ₁, hr, _, hK, back⟩ := ctx_typing _ hE ask_not_value h
    obtain ⟨rfl, -, look⟩ := hK.resolve_left fun hp => by subst hp; exact (ask_inv hr).1
    simp only [List.map_up_zero] at hr
    have hv := (look _ _ _ hl).sub (J' := .ex t₁ φ₁) ⟨(ask_inv hr).2, Stage.pure_le _⟩
    have hv' := hv.substT (kill Λ.length) (Fits.kill fun p hp => hbl p (hp.imp (lookupE_has _ hl) (lookupE_has _ hl)))
    rw [List.map_kill_YF hΨ] at hv'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using hv'), by simp⟩
  | askRoot hE _ hr₀ =>
    obtain ⟨K, Θ', t₁, φ₁, hr, _, hK, back⟩ := ctx_typing _ hE ask_not_value h
    obtain ⟨rfl, -, -⟩ := hK.resolve_left fun hp => by subst hp; exact (ask_inv hr).1
    simp only [List.map_up_zero] at hr
    have hv := (((hroot.svc _ _ hr₀).2.cut (Entails.none (Θ'.substT (kill Λ.length)))).weakenStore Ψ).sub
      (J' := .ex (t₁.substT (kill Λ.length)) (φ₁.substT (kill Λ.length)))
      ⟨by have := (ask_inv hr).2.substT (kill Λ.length); rwa [S.impl_closed] at this, Stage.pure_le _⟩
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using hv), by simp⟩
  | glob hE hg =>
    obtain ⟨_, _, _, _, hr, _, _, back⟩ := ctx_typing _ hE (fun h => by cases h) h
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by
      simpa using glob_preservation hroot (hole_kill hr hbl hΨ) hg), by simp⟩
  | raise hE hF hnt hni hv =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ :=
      ctx_typing _ hE (not_value_plug raise_not_value _) h
    obtain ⟨rfl, -, -⟩ := hK.resolve_left fun hp => by
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
    obtain ⟨rfl, -, -⟩ := hK.resolve_left fun hp => by subst hp; exact fin_not_pure hr (.inl rfl)
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using fin_unit hr' (.inl rfl)),
      by simp⟩
  | spawn hE _ _ _ =>
    obtain ⟨K, Θ', _, _, hr, _, hK, back⟩ := ctx_typing _ hE (fin_not_value (.inr rfl)) h
    obtain ⟨rfl, -, -⟩ := hK.resolve_left fun hp => by subst hp; exact fin_not_pure hr (.inr rfl)
    have hr' := hole_kill hr hbl hΨ
    simp only [List.map_up_zero, liftT_zero] at hr'
    exact ⟨[], by simpa using hμ, refill_kill back hbl hΨ (by simpa using fin_unit hr' (.inr rfl)),
      by simp⟩

end Flick
