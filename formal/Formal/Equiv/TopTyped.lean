import Formal.Equiv.Cfg
import Formal.Preservation

/-!
# Contextual equivalence: typing keeps lazy cells away from patterns

Where a pattern reads a term without running it, typing gives the term the pure stage (a
field of a dictionary, the value of a handler), or no term judgment at all (a list of
alternatives or of arms): `PureJ`. A term that makes or forces a lazy cell has the stage `rt` at
least (`Typ.lzShape`). So a typed term has none at such a place (`Typ.noLzTop`), but in the body
of an arm, which typing does not reach when the arm is a dead one of a GADT.

Typing also guarantees that every pattern match in evaluation position binds values
(`Typ.matchVals`) and that a closed term typed at the pure stage evaluates purely without touching
the store, regions, errors, signals, effects, lazy settlements, or continuations (`Typ.pure_eval`).
-/

namespace Flick.Equiv

variable {S : Sig}

/-- A judgment of a place a pattern reads. -/
def PureJ : J → Prop
  | .ex _ φ => φ = .pure
  | _ => True

theorem not_value_lazy {e : Tm} : ¬ Value (.lazy e) := fun h => by cases h

/-- A judgment of a term with the stage `rt` at least. -/
def RtJ (J : J) : Prop := ∃ t φ, J = .ex t φ ∧ (Stage.rt Row.none Row.none).le φ

theorem rt_not_pure : ¬ (Stage.rt Row.none Row.none).le .pure := fun h => h

theorem RtJ.not_pure {J : J} (h : RtJ J) : ¬ PureJ J := by
  obtain ⟨_, _, rfl, hφ⟩ := h
  intro hp; cases hp; exact rt_not_pure hφ

theorem _root_.Flick.Typ.lazy_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {a}, e = .lazy a → RtJ J := by
  induction h with
  | lazy _ _ _ hφ => intro _ _; exact ⟨_, _, rfl, hφ⟩
  | allI hv => intro _ he; subst he; exact (not_value_lazy hv).elim
  | errVal _ _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | conv _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | _ => intro _ he; cases he

theorem _root_.Flick.Typ.let_lazy_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {a b}, e = .let_ (.lazy a) b → RtJ J := by
  induction h with
  | let_ h₁ _ _ _ =>
    intro _ _ he; cases he
    obtain ⟨_, _, he', hφ⟩ := h₁.lazy_stage rfl
    cases he'; exact ⟨_, _, rfl, hφ⟩
  | letGen h₁ _ _ _ =>
    intro _ _ he; cases he
    obtain ⟨_, _, he', hφ⟩ := h₁.lazy_stage rfl
    cases he'; exact (rt_not_pure hφ).elim
  | allI hv => intro _ _ he; subst he; cases hv
  | errVal _ _ _ ih =>
    intro _ _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | conv _ _ ih =>
    intro _ _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | _ => intro _ _ he; cases he

theorem _root_.Flick.Typ.force_lazy_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {a}, e = .force (.lazy a) → RtJ J := by
  induction h with
  | force h₁ _ _ _ =>
    intro _ he; cases he
    obtain ⟨_, _, he', hφ⟩ := h₁.lazy_stage rfl
    cases he'; exact ⟨_, _, rfl, hφ⟩
  | allI hv => intro _ he; subst he; cases hv
  | errVal _ _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | conv _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | _ => intro _ he; cases he

/-- **A term that makes or forces a lazy cell has the stage `rt` at least.** -/
theorem _root_.Flick.Typ.lzShape {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) (hs : e.LzShape) :
    ¬ PureJ J := by
  match e, hs with
  | .lazy _, _ => exact (h.lazy_stage rfl).not_pure
  | .let_ (.lazy _) _, _ => exact (h.let_lazy_stage rfl).not_pure
  | .force (.lazy _), _ => exact (h.force_lazy_stage rfl).not_pure

theorem noLzTop_aux {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Typ S Ψ Θ Γ e J → ∀ {x : Pos}, (x = .top → PureJ J) → ¬ e.LzTop x := by
  induction h with
  | allI _ h₁ ih => exact fun _ {_} _ => ih h₁ (fun _ => rfl)
  | conv h₁ _ ih => exact fun _ {_} hx => ih h₁ hx
  | errVal h₁ _ _ ih => exact fun _ {_} hx => ih h₁ hx
  | _ =>
    intro hty x hx hz
    cases hz
    all_goals first
      | (rename_i hs; exact Typ.lzShape hty hs (hx rfl))
      | (apply absurd ‹Tm.LzTop _ _›; apply_assumption <;>
         first | assumption | (intro h; cases h; done) | (intro _; trivial) | (intro _; rfl) | exact hx)

/-- **A typed term has no lazy cell where a pattern reads**, outside the bodies of arms. -/
theorem _root_.Flick.Typ.noLzTop {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) {x : Pos}
    (hx : x = .top → PureJ J) : ¬ e.LzTop x :=
  noLzTop_aux h h hx

theorem _root_.Flick.Inst.values {Ψ Θ Γ δ Δ} (h : Inst S Ψ Θ Γ δ Δ) : ∀ w ∈ δ, Value w := by
  intro w hw
  obtain ⟨i, hi⟩ := List.mem_iff_getElem?.mp hw
  have hlt : i < Δ.length := h.1 ▸ (List.getElem?_eq_some_iff.mp hi).1
  exact (h.2 i w _ hi (List.getElem?_eq_getElem hlt)).1

private theorem match_head_vals {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {v p b rest δ t φ}, Γ = [] → J = .ex t φ → Value v → matchP p v = some δ →
      (e = .match_ v (.acons p b rest) ∨
       (∃ g, e = .match_ v (.aguard p g b rest)) ∨
       e = .handle v (.acons p b rest) ∨
       (∃ g, e = .handle v (.aguard p g b rest))) →
      ∀ w ∈ δ, Value w := by
  induction h with
  | conv _ _ ih =>
    intro v p b rest δ t φ hΓ hJ hv hm he
    cases hJ; exact ih hΓ rfl hv hm he
  | errVal _ _ _ ih =>
    intro v p b rest δ t φ hΓ hJ hv hm he
    cases hJ; exact ih hΓ rfl hv hm he
  | allI hv' =>
    intro _ _ _ _ _ _ _ _ _ _ _ he
    rcases he with rfl | ⟨_, rfl⟩ | rfl | ⟨_, rfl⟩ <;> cases hv'
  | @match_ _ _ _ _ ts₀ _ _ _ h₁ hA _ =>
    intro v p b rest δ t φ hΓ hJ hv hm he
    cases hΓ; cases hJ
    rcases he with he | ⟨g, he⟩ | he | ⟨g, he⟩
    · cases he
      cases hA with
      | acons hp _ _ => exact (hp.inst hv (h₁.valueAny hv _) hm).values
      | aref hK hU hn _ _ hpA _ _ =>
        have hU7 : ts₀.kind ≠ 7 := by rw [Ty.kind_of_hd hU]; decide
        obtain ⟨us₀, hk₀, ⟨ws, hr, hw⟩, hus⟩ :=
          (h₁.valueAny hv .pure).spine_match hv (matchP_spine_head hm) hK rfl hU7
            (Ty.not_all_of_hd hU)
        rw [Ty.res_of_hd hU] at hr
        obtain ⟨us, hk, hmap, hwu⟩ := Sig.Vary.lift hK hk₀ hw
        have hF : Fits (instT us) b := ⟨fun _ _ => rfl, fun h => (hn h).elim,
          fun h => (hn h).elim, fun h => (hn h).elim⟩
        have hsp := (hpA (instT us) hF (by
          rw [Ty.up_substT ts₀ _ us hk, Ty.tcon_substT, hr]
          exact hmap.tcon _)).spTy (us := us) (.head hK)
        exact (hus hwu hsp hm).2.values
    · cases he
      cases hA with
      | aguard hp _ _ _ => exact (hp.inst hv (h₁.valueAny hv _) hm).values
    · cases he
    · cases he
  | handle _ hvU hU hE hA _ _ _ =>
    intro v p b rest δ t φ hΓ hJ hv hm he
    cases hΓ; cases hJ
    have hv' := Typ.errVal hvU hU hE
    rcases he with he | ⟨g, he⟩ | he | ⟨g, he⟩
    · cases he
    · cases he
    · cases he
      cases hA with
      | hcons hp _ _ _ => exact (hp.inst hv hv' hm).1.values
    · cases he
      cases hA with
      | hguard hp _ _ _ _ => exact (hp.inst hv hv' hm).1.values
  | _ =>
    intro _ _ _ _ _ _ _ _ _ _ _ he
    rcases he with he | ⟨_, he⟩ | he | ⟨_, he⟩ <;> cases he

/-- Every pattern match in evaluation position of a typed term binds only values. -/
theorem _root_.Flick.Typ.matchVals {Ψ Θ e t φ} (h : Typ S Ψ Θ [] e (.ex t φ)) :
    e.MatchVals := by
  intro E v p b rest δ hE hv hm he
  rcases he with rfl | ⟨g, rfl⟩ | rfl | ⟨g, rfl⟩
  · obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing E hE (by intro h; cases h) h
    exact match_head_vals hr rfl rfl hv hm (.inl rfl)
  · obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing E hE (by intro h; cases h) h
    exact match_head_vals hr rfl rfl hv hm (.inr (.inl ⟨g, rfl⟩))
  · obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing E hE (by intro h; cases h) h
    exact match_head_vals hr rfl rfl hv hm (.inr (.inr (.inl rfl)))
  · obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing E hE (by intro h; cases h) h
    exact match_head_vals hr rfl rfl hv hm (.inr (.inr (.inr ⟨g, rfl⟩)))

private theorem settle_not_pure_aux {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {l a t}, Γ = [] → e = .settle l a → J = .ex t .pure → False := by
  induction h with
  | conv _ _ ih => intro _ _ _ hΓ he hJ; cases hJ; exact ih hΓ he rfl
  | errVal _ _ _ ih => intro _ _ _ hΓ he hJ; cases hJ; exact ih hΓ he rfl
  | allI hv => intro _ _ _ _ he _; subst he; cases hv
  | settle _ _ _ hφ _ => intro _ _ _ _ _ hJ; cases hJ; exact Stage.use_rt_le_ne_pure hφ rfl
  | _ => intro _ _ _ _ he _; cases he

theorem settle_not_pure {Ψ Θ l a t} (h : Typ S Ψ Θ [] (.settle l a) (.ex t .pure)) : False :=
  settle_not_pure_aux h rfl rfl rfl

theorem kont_app_not_pure {Θ p ℓ u t} (h : Typ S [] Θ [] (.app (.kont p ℓ) u) (.ex t .pure)) :
    False := by
  have hnv : ¬ Value (.app (.kont p ℓ) u) := by
    intro hv; cases hv with | ctorApp _ hK _ => simp [Tm.head] at hK
  obtain ⟨_, _, _, _, hf, -⟩ := h.app_nonvalue _ _ _ _ rfl rfl hnv
  obtain ⟨_, _, _, _, hl, -⟩ := hf.kont_arrow
  simp at hl

/-- A pure step preserves typing at the pure stage in the empty store. -/
theorem _root_.Flick.Typ.pstep {root Θ e e' t} (hroot : RootTy S root)
    (h : Typ S [] Θ [] e (.ex t .pure)) (hs : PStep root e e') :
    Typ S [] Θ [] e' (.ex t .pure) := by
  cases hs with
  | head hE hh =>
    obtain ⟨_, _, _, _, hr, _, _, back, -⟩ := ctx_typing _ hE hh.not_value h
    have h' := back.id [] _ (by simpa using pure_preservation hr hh)
    simpa using h'
  | glob hE hg =>
    obtain ⟨_, _, _, _, hr, _, _, back, -⟩ := ctx_typing _ hE (fun h => by cases h) h
    have h' := back.id [] _ (by simpa using glob_preservation hroot hr hg)
    simpa using h'

theorem _root_.Flick.Typ.psteps {root Θ e e' t} (hroot : RootTy S root)
    (h : Typ S [] Θ [] e (.ex t .pure)) (hs : PSteps root e e') :
    Typ S [] Θ [] e' (.ex t .pure) := by
  induction hs with
  | refl => exact h
  | step s _ ih => exact ih (h.pstep hroot s)

private theorem pure_step_only {root Θ e t Λ μ l Λ' μ' e'}
    (h : Typ S [] Θ [] e (.ex t .pure)) (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    l = .pure ∧ Λ' = Λ ∧ μ' = μ ∧ PStep root e e' := by
  cases hs with
  | pure hE hh => exact ⟨rfl, rfl, rfl, .head hE hh⟩
  | glob hE hg => exact ⟨rfl, rfl, rfl, .glob hE hg⟩
  | rt hE hh =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE hh.not_value h
    rw [rel.pure rfl] at hr
    exact (hh.not_pure hr).elim
  | addFin hE =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (fin_not_value (.inl rfl)) h
    rw [rel.pure rfl] at hr
    exact (fin_not_pure hr (.inl rfl)).elim
  | spawn hE =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (fin_not_value (.inr rfl)) h
    rw [rel.pure rfl] at hr
    exact (fin_not_pure hr (.inr rfl)).elim
  | performRoot hE =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE perform_not_value h
    rw [rel.pure rfl] at hr
    exact (const_app_rt hr fun _ => perform_arrow).elim
  | raise hE hF =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (not_value_plug raise_not_value _) h
    rw [rel.pure rfl] at hr
    obtain ⟨_, _, _, _, hr', rel', -⟩ := frame_typing raise_not_value _ hF hr
    rw [rel'.pure rfl] at hr'
    exact (raise_not_pure hr').elim
  | unw hE hF hni =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (not_value_plug unw_not_value _) h
    rw [rel.pure rfl] at hr
    exact (unw_not_pure (unw_frame hF hni hr)).elim
  | performAbort hE | performTail hE | performCtl hE =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (fun h => by cases h) h
    rw [rel.pure rfl] at hr
    exact (hwith_not_pure hr).elim
  | performKept hE =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (fun h => by cases h) h
    rw [rel.pure rfl] at hr
    exact (hkeep_not_pure hr).elim
  | resume hE =>
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing _ hE (fun h => by cases h) h
    rw [rel.pure rfl] at hr
    exact (hctl_not_pure hr).elim

/-- **Typing at the pure stage guarantees operational purity** (`PureEval`): every reachable step
is a pure step leaving the store and regions unchanged, with no runtime error, signal, effect,
lazy settlement, or continuation application in evaluation position. -/
theorem _root_.Flick.Typ.pure_eval {root Θ e τ} (hroot : RootTy S root)
    (h : Typ S [] Θ [] e (.ex τ .pure)) : PureEval root e := by
  intro t ht
  have hty := h.psteps hroot ht
  refine ⟨fun Λ μ l Λ' μ' t' hs => pure_step_only hty hs, ?_, ?_, ?_, ?_, ?_,
    hty.noLzTop (fun h => nomatch h), hty.matchVals⟩
  · intro E v hE _ he; subst he
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing E hE raise_not_value hty
    rw [rel.pure rfl] at hr; exact raise_not_pure hr
  · intro E p ℓ a hE he; subst he
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing E hE unw_not_value hty
    rw [rel.pure rfl] at hr; exact unw_not_pure hr
  · intro E l a hE he; subst he
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing E hE (fun h => by cases h) hty
    rw [rel.pure rfl] at hr; exact settle_not_pure hr
  · intro E Ef o c v hE _ he; subst he
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing E hE perform_not_value hty
    rw [rel.pure rfl] at hr; exact const_app_rt hr fun _ => perform_arrow
  · intro E p ℓ u hE _ he; subst he
    have hnv : ¬ Value (.app (.kont p ℓ) u) := by
      intro hv; cases hv with | ctorApp _ hK _ => simp [Tm.head] at hK
    obtain ⟨_, _, _, _, hr, rel, -⟩ := ctx_typing E hE hnv hty
    rw [rel.pure rfl] at hr; rw [List.map_nil] at hr
    exact kont_app_not_pure hr

end Flick.Equiv
