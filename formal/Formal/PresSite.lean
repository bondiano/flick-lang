import Formal.PresHdlStep

/-!
# Preservation at the sites of a step

The facts `preservation` uses at the redex of each step of a context: the stage of
a redex, the frame around `raise`, registration, globals, the root's functions,
and the renaming of the regions not yet created out of a context.
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

/-- `raise v` takes a runtime stage whose errors hold those of `v`. -/
theorem raise_stage {Ψ Θ e t φ} (h : TypM S mode Ψ Θ [] (.raise e) (.ex t φ)) :
    ∃ ε, (Stage.rt Row.none ε).le φ := by
  cases h with
  | raise _ hφ => exact ⟨_, hφ⟩
  | raiseAdt _ _ hφ => exact ⟨_, hφ⟩
  | errVal hv | allI hv => cases hv

theorem raise_not_pure {Ψ Θ e t} (h : TypM S mode Ψ Θ [] (.raise e) (.ex t .pure)) : False := by
  obtain ⟨_, hε⟩ := raise_stage h; exact hε

/-- The stage of an application of a constant whose type has a runtime stage. -/
theorem const_app_rt {Ψ Θ c w t} (h : TypM S mode Ψ Θ [] (.app (.const c) w) (.ex t .pure))
    (hc : ∀ ι, ∃ a r e b C, c.ty S ι mode = .arrow a (.rt r e) b C) : False := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | const _ hT' =>
      obtain ⟨_, _, _, _, _, hx⟩ := hc ‹_›
      rw [hx] at hT'
      obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
      cases h; exact TypingMode.use_rt_le_ne_pure (Stage.le_trans lψ hψ) rfl
    | perform _ hT' =>
      obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
      cases h; exact TypingMode.use_rt_le_ne_pure (Stage.le_trans lψ hψ) rfl
  | allI hv _ => cases hv with
    | constApp hc' _ =>
      cases ‹Const› <;> simp_all [Const.arity, Const.ty] <;> exact (hc default).elim
    | ctorApp _ hK _ => simp [Tm.head] at hK
  | errVal hv _ _ _ => cases hv with
    | constApp hc' _ =>
      cases ‹Const› <;> simp_all [Const.arity, Const.ty] <;> exact (hc default).elim
    | ctorApp _ hK _ => simp [Tm.head] at hK

theorem const_app2_rt {Ψ Θ c w₁ w₂ t} (h : TypM S mode Ψ Θ [] (.app (.app (.const c) w₁) w₂) (.ex t .pure))
    (hc : ∀ ι, ∃ a b r e d C C', c.ty S ι mode = .arrow a .pure (.arrow b (.rt r e) d C) C') : False := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | app hc' _ hψ₁ hT₁ =>
      cases hc' with
      | perform _ hT' =>
        obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
        cases h; exact TypingMode.use_rt_le_ne_pure (Stage.le_trans lψ hψ₁) rfl
      | const _ hT' =>
        obtain ⟨_, _, _, _, _, _, _, hx⟩ := hc ‹_›
        rw [hx] at hT'
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h
        obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := (lt.trans hT₁).arrow_left
        cases h; exact TypingMode.use_rt_le_ne_pure (Stage.le_trans lψ hψ) rfl
  | allI hv _ => cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK
  | errVal hv _ _ _ => cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK

theorem perform_arrow {E o c} {ι : CArg} :
    ∃ a r e b C, (Const.perform E o c).ty S ι mode = .arrow a (.rt r e) b C := by
  simp only [Const.ty]
  split <;> exact ⟨_, _, _, _, _, rfl⟩

/-- An operation takes a stage with its effect. -/
theorem perform_svc {Ψ Θ E o c w t φ} (h : TypM S mode Ψ Θ [] (.app (.const (.perform E o c)) w) (.ex t φ)) :
    φ.svc (.nm E) := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | perform _ hT' =>
      obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
      cases h
      exact Stage.le_svc (Stage.le_trans (Stage.le_trans (TypingMode.le_use mode _ _) lψ) hψ) _ rfl
    | const hok _ => exact hok.elim
  | errVal hv | allI hv => exact (perform_not_value hv).elim

/-- An operation that has a type is declared. -/
theorem perform_op' {Ψ Θ E o c w t φ} (h : TypM S mode Ψ Θ [] (.app (.const (.perform E o c)) w) (.ex t φ)) :
    ∃ a b, S.op E o = some (c, a, b) := by
  cases h with
  | app hf _ _ _ =>
    cases hf with
    | perform hop _ => exact ⟨_, _, hop⟩
    | const hok _ => exact hok.elim
  | errVal hv | allI hv => exact (perform_not_value hv).elim

theorem op_rt (ι : CArg) : ∃ a r e b C, Const.ty S .op ι mode = .arrow a (.rt r e) b C := by
  cases mode <;> exact ⟨_, _, _, _, _, rfl⟩

/-- No runtime redex has a pure stage. -/
theorem RHead.not_pure {Ψ Θ Λ μ r t Λ' μ' r'} (h : TypM S mode Ψ Θ [] r (.ex t .pure))
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
    | hres _ _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | hwithCatch => cases h with
    | hwith _ _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | hdl => cases h with
    | hdl _ _ _ _ _ _ _ _ hφ _ => exact hφ
    | errVal hv | allI hv => exact nv hv
  | refGet | scopeRun _ => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | open_ => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | op => exact const_app_rt h fun ι => op_rt ι
  | refSet => exact const_app2_rt h fun _ => ⟨_, _, _, _, _, _, _, rfl⟩
  | await => cases h with
    | await _ hφ => exact TypingMode.use_rt_le_ne_pure hφ rfl
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
    (hv : Value v) (h : TypM S mode Ψ Θ [] (F.plug (.raise v)) (.ex t φ)) :
    TypM S mode Ψ Θ [] (.raise v) (.ex t φ) := by
  obtain ⟨k, Θ', _, φ', hr, rel, hk, _⟩ := frame_typing_mode raise_not_value F hF h
  obtain ⟨ε, hv', hε⟩ := raise_inv_mode hv hr
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
theorem fin_unit {Ψ Θ c p w T φ} (h : TypM S mode Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) : TypM S mode Ψ Θ [] (.lit .unit) (.ex T φ) := by
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
          cases h
          cases TypingMode.subtype.eq_base_left lt'
          cases TypingMode.subtype.eq_base_left hT; exact .lit
  | errVal hv' | allI hv' => exact (fin_not_value hc hv').elim

/-- The hole of a registration is not pure. -/
theorem fin_not_pure {Ψ Θ c p w} (h : TypM S mode Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T .pure))
    (hc : c = .addFin ∨ c = .spawn) : False := by
  rcases hc with rfl | rfl <;> exact const_app2_rt h fun _ => ⟨_, _, _, _, _, _, _, rfl⟩

/-- A context refilled after the regions not yet created are renamed out of the
whole derivation. -/
theorem refill_kill {Ψ K Θ' t' φ' E t φ r n X r'}
    (back : RefillM S mode Ψ Facts.age K Θ' t' φ' (plug E) t φ r) (hb : (plug E r).Below n)
    (hΨ : ∀ u ∈ Ψ, u.YF n)
    (h' : TypM S mode ((Ψ ++ X).map (Ty.up K)) (Θ'.substT (liftT K (kill n))) [] r'
      (.ex (t'.substT (liftT K (kill n))) (φ'.substT (liftT K (kill n))))) :
    TypM S mode (Ψ ++ X) Facts.age [] (plug E r') (.ex (t.substT (kill n)) (φ.substT (kill n))) := by
  have := back (kill n) (Fits.kill hb) X r' (by rw [List.map_kill_YF hΨ]; exact h')
  rw [List.map_kill_YF hΨ] at this
  exact this.cut (Facts.age_kill n)

/-- The hole renamed with its context. -/
theorem hole_kill {Ψ : List Ty} {K : Nat} {Θ' : Facts} {r : Tm} {t' : Ty} {φ' : Stage} {n : Nat}
    {E : List Frame} (hr : TypM S mode (Ψ.map (Ty.up K)) Θ' [] r (.ex t' φ'))
    (hb : (plug E r).Below n) (hΨ : ∀ u ∈ Ψ, u.YF n) :
    TypM S mode (Ψ.map (Ty.up K)) (Θ'.substT (liftT K (kill n))) [] r
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
theorem glob_preservation {root Ψ Θ g T φ v} (hroot : RootTyM S mode root)
    (h : TypM S mode Ψ Θ [] (.glob g) (.ex T φ)) (hg : root.glob g = some v) : TypM S mode Ψ Θ [] v (.ex T φ) := by
  cases h with
  | glob hs hts hH hT =>
    obtain ⟨hno, _, hs', ht⟩ := hroot.glob _ _ hg
    rw [hs] at hs'; cases hs'
    refine (TypM.instantiate (Γ := []) hts hH (hno _) ?_).sub ⟨hT, Stage.pure_le _⟩
    exact (ht.weakenStore _).cut fun _ _ h => h.mono fun _ _ h => .inr h
  | errVal hv | allI hv => cases hv

/-- A root writes no region: its type names none. -/
theorem root_no_rgn {root : Roots} (hroot : RootTy S root) {N v} (h : root.svc N = some v) (p : Nat) :
    ¬ v.Has (.rgn p) := fun hp =>
  S.impl_reach N _ ((Typ.holds (hroot.svc N v h).2 (hroot.svc N v h).1 rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))

/-- A root's function for an operation has the type of the operation. -/
theorem root_field {root Ψ Θ Ef o w f c v t φ} (hroot : RootTyM S mode root) (hr₀ : root.svc Ef = some w)
    (hf : w.field o = some f)
    (h : TypM S mode Ψ Θ [] (.app (.const (.perform Ef o c)) v) (.ex t φ)) :
    TypM S mode Ψ Θ [] (.app f v) (.ex t φ) := by
  cases h with
  | app hf' hx hψ hT =>
    cases hf' with
    | perform hop hT' =>
      obtain ⟨hwv, hwt⟩ := hroot.svc _ _ hr₀
      have hne : S.impl Ef ≠ .errors Row.none := fun he => by
        rw [he] at hwt
        obtain ⟨_, _, _, _, _, _, _, hN, _⟩ := canonical_errors_mode hwv hwt
        exact hN.elim
      have hc : c = .tail := Classical.byContradiction fun hc => hne (S.op_impl _ _ _ _ _ hop hc)
      subst hc
      have ft := field_typ_mode hwt hf (S.op_field _ _ _ _ hop hne)
      have ft' := (ft.cut (Entails.none Θ)).weakenStore Ψ
      simp only [List.nil_append] at ft'
      obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
      cases h
      simp only [reduceCtorEq, if_false] at lt
      exact .app (ft'.sub ⟨TypingMode.subtype.refl, Stage.pure_le _⟩) (hx.sub ⟨la, Stage.le_refl _⟩)
        ((TypingMode.use_none mode _).symm ▸
          Stage.none_le (TypingMode.use_rt_le_ne_pure (Stage.le_trans lψ hψ))) (lt.trans hT)
    | const hok _ => exact hok.elim
  | errVal hv' | allI hv' => exact (perform_not_value hv').elim

end Flick
