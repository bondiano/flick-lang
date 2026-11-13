import Formal.PresHdlStep
import Formal.GlobalTyping

/-!
# Preservation at the sites of a step

The facts `preservation` uses at the redex of each step of a context: the stage of
a redex, the frame around `raise`, registration, globals, the root's functions,
and the renaming of the regions not yet created out of a context.
-/

namespace Flick

variable {S : Sig}

/-- `raise v` takes a runtime stage whose errors hold those of `v`. -/
theorem raise_stage {Ψ Θ e t φ} (h : Typ S Ψ Θ [] (.raise e) (.ex t φ)) :
    ∃ ε, (Stage.rt Row.none ε).le φ := by
  exact h.raise_stage e t φ rfl rfl

theorem raise_not_pure {Ψ Θ e t} (h : Typ S Ψ Θ [] (.raise e) (.ex t .pure)) : False := by
  obtain ⟨_, hε⟩ := raise_stage h; exact hε

/-- The stage of an application of a constant whose type has a runtime stage. -/
theorem const_app_rt {Ψ Θ c w t} (h : Typ S Ψ Θ [] (.app (.const c) w) (.ex t .pure))
    (hc : ∀ ι, ∃ a r e b C, c.ty S ι = .arrow a (.rt r e) b C) : False := by
  have hn : ¬ Value (.app (.const c) w) := by
    intro hv
    cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK
    | constApp hArity _ =>
      cases c <;> simp_all [Const.arity, Const.ty] <;> exact (hc default).elim
  obtain ⟨_, _, _, _, hf, _, hψ, _⟩ := h.app_nonvalue _ _ _ _ rfl rfl hn
  obtain ⟨ι, hT'⟩ := hf.const_witness
  obtain ⟨_, _, _, _, _, hx⟩ := hc ι
  rw [hx] at hT'
  obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
  cases h
  exact Stage.use_rt_le_ne_pure (Stage.le_trans lψ hψ) rfl

theorem const_app2_rt {Ψ Θ c w₁ w₂ t} (h : Typ S Ψ Θ [] (.app (.app (.const c) w₁) w₂) (.ex t .pure))
    (hc : ∀ ι, ∃ a b r e d C C', c.ty S ι = .arrow a .pure (.arrow b (.rt r e) d C) C') : False := by
  have hn : ¬ Value (.app (.app (.const c) w₁) w₂) := by
    intro hv
    cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK
  obtain ⟨_, _, _, _, hf, _, hψ, _⟩ := h.app_nonvalue _ _ _ _ rfl rfl hn
  obtain ⟨_, _, _, _, hc', _, _, hT₁⟩ := hf.app_arrow
  obtain ⟨ι, hT'⟩ := hc'.const_witness
  obtain ⟨_, _, _, _, _, _, _, hx⟩ := hc ι
  rw [hx] at hT'
  obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
  cases h
  obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := (lt.trans hT₁).arrow_left
  cases h
  exact Stage.use_rt_le_ne_pure (Stage.le_trans lψ hψ) rfl

theorem perform_arrow {E o c} {ι : CArg} :
    ∃ a r e b C, (Const.perform E o c).ty S ι = .arrow a (.rt r e) b C := by
  simp only [Const.ty]
  split <;> exact ⟨_, _, _, _, _, rfl⟩

/-- An operation takes a stage with its effect. -/
theorem perform_svc {Ψ Θ E o c w t φ} (h : Typ S Ψ Θ [] (.app (.const (.perform E o c)) w) (.ex t φ)) :
    φ.svc (.nm E) := by
  obtain ⟨_, _, _, _, hf, _, hψ, _⟩ := h.app_nonvalue _ _ _ _ rfl rfl perform_not_value
  obtain ⟨_, _, _, _, hT'⟩ := hf.perform_arrow
  obtain ⟨_, _, _, _, h, _, lψ, _, -⟩ := hT'.arrow_left
  cases h
  exact Stage.le_svc (Stage.le_trans (Stage.le_trans (Stage.le_use _ _) lψ) hψ) _ rfl

/-- An operation that has a type is declared. -/
theorem perform_op' {Ψ Θ E o c w t φ} (h : Typ S Ψ Θ [] (.app (.const (.perform E o c)) w) (.ex t φ)) :
    ∃ a b, S.op E o = some (c, a, b) := by
  obtain ⟨_, _, _, _, hf, _, _, _⟩ := h.app_nonvalue _ _ _ _ rfl rfl perform_not_value
  obtain ⟨a, b, _, hop, _⟩ := hf.perform_arrow
  exact ⟨a, b, hop⟩

theorem op_rt (ι : CArg) : ∃ a r e b C, Const.ty S .op ι = .arrow a (.rt r e) b C := by
  exact ⟨_, _, _, _, _, rfl⟩

private theorem Typ.runtimeStage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ Λ μ Λ' μ' e' t, Γ = [] → J = .ex t .pure → RHead Λ μ e Λ' μ' e' → False := by
  induction h with
  | conv _ _ ih =>
    intro Λ μ Λ' μ' e' t hΓ hJ hh; cases hJ
    exact ih Λ μ Λ' μ' e' _ hΓ rfl hh
  | allI hv | errVal hv => intro _ _ _ _ _ _ _ _ hh; exact hh.not_value hv
  | app hf hx hψ hT =>
    intro Λ μ Λ' μ' e' t hΓ hJ hh; cases hΓ; cases hJ
    have h := Typ.app hf hx hψ hT
    cases hh with
    | refSet => exact const_app2_rt h fun _ => ⟨_, _, _, _, _, _, _, rfl⟩
    | op => exact const_app_rt h fun ι => op_rt ι
    | _ => exact const_app_rt h fun _ => ⟨_, _, _, _, _, rfl⟩
  | hwith _ _ _ _ _ _ _ hφ _ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | hctl _ _ _ _ _ _ _ _ _ _ _ _ _ _ hφ _ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | hkeep _ _ _ _ _ _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | hkarm _ _ _ _ _ _ _ _ _ _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | hres _ _ _ _ _ _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | hdl _ _ _ _ _ _ _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | scope _ _ _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | inside _ _ _ _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | handle _ _ _ _ _ hφ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | tryR _ _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | unw _ _ hφ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ; exact hφ
  | await _ hφ _ =>
    intro _ _ _ _ _ _ _ hJ _; cases hJ
    exact Stage.use_rt_le_ne_pure hφ rfl
  | tryP hb _ =>
    intro _ _ _ _ _ _ _ hJ hh; cases hJ
    cases hh with
    | tryRaise =>
      obtain ⟨_, hε⟩ := hb.raise_stage _ _ _ rfl rfl
      exact hε
  | _ => intro _ _ _ _ _ _ _ _ hh; cases hh

/-- No runtime redex has a pure stage. -/
theorem RHead.not_pure {Ψ Θ Λ μ r t Λ' μ' r'} (h : Typ S Ψ Θ [] r (.ex t .pure))
    (hh : RHead Λ μ r Λ' μ' r') : False :=
  h.runtimeStage Λ μ Λ' μ' r' t rfl rfl hh

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
  obtain ⟨_, _, _, _, hf, _, _, hT⟩ := h.app_nonvalue _ _ _ _ rfl rfl (fin_not_value hc)
  obtain ⟨_, _, _, _, hc', _, _, hT₁⟩ := hf.app_arrow
  obtain ⟨_, hT'⟩ := hc'.const_witness
  rcases hc with rfl | rfl <;>
  · obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
    cases h
    obtain ⟨_, _, _, _, h, _, _, lt', -⟩ := (lt.trans hT₁).arrow_left
    cases h
    cases LeLive.eq_base_left lt'
    cases LeLive.eq_base_left hT
    exact .lit

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
  obtain ⟨_, _, hs, hts, hH, hT⟩ := h.glob_type _ _ _ rfl rfl
  obtain ⟨hno, _, hs', ht⟩ := hroot.glob _ _ hg
  rw [hs] at hs'; cases hs'
  refine (Typ.instantiate (Γ := []) hts hH (hno _) ?_).sub ⟨hT, Stage.pure_le _⟩
  exact (ht.weakenStore _).cut fun _ _ h => h.mono fun _ _ h => .inr h

/-- A root writes no region: its type names none. -/
theorem root_no_rgn {root : Roots} (hroot : RootTy S root) {N v} (h : root.svc N = some v)
    (p : Nat) : ¬ v.Has (.rgn p) :=
  hroot.svcBelow N v h p

/-- A root's function for an operation has the type of the operation. -/
theorem root_field {root Ψ Θ Ef o w f c v t φ} (hroot : RootTy S root) (hr₀ : root.svc Ef = some w)
    (hf : w.field o = some f)
    (h : Typ S Ψ Θ [] (.app (.const (.perform Ef o c)) v) (.ex t φ)) :
    Typ S Ψ Θ [] (.app f v) (.ex t φ) := by
  obtain ⟨_, _, _, _, hf', hx, hψ, hT⟩ := h.app_nonvalue _ _ _ _ rfl rfl perform_not_value
  obtain ⟨_, _, _, hop, hT'⟩ := hf'.perform_arrow
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
  exact .app (ft'.sub ⟨LeLive.refl, Stage.pure_le _⟩) (hx.sub ⟨la, Stage.le_refl _⟩)
    ((Stage.use_none _).symm ▸
      Stage.none_le (Stage.use_rt_le_ne_pure (Stage.le_trans lψ hψ))) (lt.trans hT)

end Flick
