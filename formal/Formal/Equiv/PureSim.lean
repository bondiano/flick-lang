import Formal.Equiv.PureSimHead

/-!
# Contextual equivalence: multi-step pure simulation
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

private theorem pure_base_step {μ₁ μ₂ : List Tm} (hst : StR W μ₁ μ₂)
    (hglob : ∀ g v, W.R.glob g = some v → v.Src)
    {r r' t : Tm} (hp : PHead r r' ∨ ∃ g, r = .glob g ∧ W.R.glob g = some r')
    (hpa : PureEval W.R r) (hpb : PureEval W.R t) (hs : Sim W .any r t) :
    ∃ t', (t' = t ∨ PStep W.R t t') ∧ Sim W .any r' t' := by
  have hnv : ¬ Value r := by
    rcases hp with hh | ⟨g, rfl, -⟩
    · exact hh.not_value
    · intro hv; cases hv
  generalize hx : Pos.any = x at hs
  cases hs with
  | dropA | forceA | jL | bornA | bornV | o4B | o4Alloc1 | o4Settle =>
    first | exact hpa.not_let_lazy.elim | exact hpa.not_force_lazy.elim
          | exact hpa.not_settle.elim | exact hpa.not_lazy.elim
  | dropC | forceC | jR | bornB | o4A | o4AllocR =>
    first | exact hpb.not_let_lazy.elim | exact hpb.not_force_lazy.elim
          | exact hpb.not_settle.elim | exact hpb.not_lazy.elim
  | dead =>
    rcases hp with hh | ⟨_, h, -⟩
    · cases hh
    · cases h
  | dropB _ hb hc =>
    rcases hp with hh | ⟨_, h, -⟩
    · cases hh
      subst hb; rw [subst_shift]
      exact ⟨t, .inl rfl, hc⟩
    · cases h
  | o4Bind _ hc =>
    rcases hp with hh | ⟨_, h, -⟩
    · cases hh
      exact ⟨t, .inl rfl, hc⟩
    · cases h
  | bornS _ hl _ hbs =>
    obtain ⟨c, c', -, -, hi⟩ := hst.cell _ _ _ hl
    have hv : Value _ := (show BornInv W _ _ c c' from hi).val
    rcases hbs with rfl | rfl | rfl | rfl | rfl
    · have hp2 := hpa.step (.head OkE.nil (.let_ .loc))
      simp only [Tm.subst, inst, liftS] at hp2
      exact hp2.not_let_force_loc.elim
    · exact hpa.not_let_force_loc.elim
    · exact (hpa.no_settle .refl [.let_ _] _ _ (OkE.nil.cons' trivial) rfl).elim
    · rcases hp with hh | ⟨_, h, -⟩
      · cases hh; exact ⟨_, .inl rfl, .loc hl⟩
      · cases h
    · rcases hp with hh | ⟨_, h, -⟩
      · cases hh with | let_ hv' => cases hv'
      · cases h
  | @app _ f f' a₀ a₀' _ h₁ h₂ =>
    subst hx
    rcases hp with hh | ⟨_, h, -⟩
    · have hfa : Value f ∧ Value a₀ := by
        have hr := hh.redex
        have hf := hr.2 (.appL a₀) f trivial rfl
        exact ⟨hf, hr.2 (.appR f) a₀ hf rfl⟩
      have ⟨hf', s₁⟩ := pure_val h₁ rfl hfa.1
        (hpa.of_frame (F := .appL a₀) trivial) (hpb.of_frame (F := .appL a₀') trivial)
      have ⟨ha', s₂⟩ := pure_val h₂ rfl hfa.2
        (hpa.of_frame (F := .appR f) hfa.1) (hpb.of_frame (F := .appR f') hf')
      obtain ⟨t', ht', hs'⟩ := pure_phead hh (.app (by decide) s₁ s₂) hpa hpb
      exact ⟨t', .inr (.head OkE.nil ht'), hs'⟩
    · cases h
  | rcons | acons | aconsD | aguard =>
    rcases hp with hh | ⟨_, h, -⟩
    · cases hh
    · cases h
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | glob =>
    subst hx
    rcases hp with hh | ⟨_, he, hg⟩
    · cases hh
    · cases he
      exact ⟨r', .inr (.glob OkE.nil hg), Sim.refl (hglob _ r' hg) .any (by decide)⟩
  | hloc => exact (hnv .loc).elim
  | o4Fix1 | o4Fix2 => exact (hnv .fix).elim
  | _ =>
    subst hx
    rcases hp with hh | ⟨_, h, -⟩
    · first
        | exact (hnv (by constructor)).elim
        | (obtain ⟨t', ht', hs'⟩ :=
            pure_phead hh (by constructor <;> first | assumption | decide) hpa hpb
           exact ⟨t', .inr (.head OkE.nil ht'), hs'⟩)
    · cases h

private theorem pure_plug_step {μ₁ μ₂ : List Tm} (hst : StR W μ₁ μ₂)
    (hglob : ∀ g v, W.R.glob g = some v → v.Src)
    {r r' : Tm} (hp : PHead r r' ∨ ∃ g, r = .glob g ∧ W.R.glob g = some r') :
    ∀ (m : Nat) {E : List Frame} {t : Tm}, E.length = m → OkE E →
      PureEval W.R (plug E r) → PureEval W.R t → Sim W .any (plug E r) t →
      ∃ t', (t' = t ∨ PStep W.R t t') ∧ Sim W .any (plug E r') t' := by
  have hnv : ¬ Value r := by
    rcases hp with hh | ⟨_, rfl, -⟩
    · exact hh.not_value
    · intro hv; cases hv
  intro m
  induction m with
  | zero =>
    intro E t hl _ hpa hpb hs
    cases E with
    | nil => exact pure_base_step hst hglob hp hpa hpb hs
    | cons => cases hl
  | succ m ih =>
    intro E t hl hE hpa hpb hs
    rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
    · cases hl
    · rw [plug_snoc] at hpa hs ⊢
      have hE₀ : OkE E₀ := fun G hG => hE G (List.mem_append_left _ hG)
      have hF : F.ok := hE F (List.mem_append_right _ (List.mem_singleton_self _))
      have hm : E₀.length = m := by simpa using hl
      have hnv₀ : ¬ Value (plug E₀ r) := not_value_plugs hnv E₀
      have hpa₀ : PureEval W.R (plug E₀ r) := hpa.of_frame hF
      have wrap : ∀ {F' : Frame} {a' : Tm}, F'.ok →
          (∀ {b b'}, Sim W .any b b' → Sim W .any (F.plug b) (F'.plug b')) →
          Sim W .any (plug E₀ r) a' → PureEval W.R (F'.plug a') →
          ∃ t', (t' = F'.plug a' ∨ PStep W.R (F'.plug a') t') ∧ Sim W .any (F.plug (plug E₀ r')) t' := by
        intro F' a' hF' hcong ha hpb'
        obtain ⟨b', hb', hsb⟩ := ih hm hE₀ hpa₀ (hpb'.of_frame hF') ha
        rcases hb' with rfl | hst'
        · exact ⟨_, .inl rfl, hcong hsb⟩
        · exact ⟨F'.plug b', .inr (hst'.frame hF'), hcong hsb⟩
      generalize hx : Pos.any = x at hs
      generalize hsf : F.plug (plug E₀ r) = s at hs
      cases hs with
      | dropA | forceA | jL | bornA | bornV | o4B | o4Alloc1 | o4Settle =>
        rw [hsf] at hpa
        first | exact hpa.not_let_lazy.elim | exact hpa.not_force_lazy.elim
              | exact hpa.not_settle.elim | exact hpa.not_lazy.elim
      | dropC | forceC | jR | bornB | o4A | o4AllocR =>
        first | exact hpb.not_let_lazy.elim | exact hpb.not_force_lazy.elim
              | exact hpb.not_settle.elim | exact hpb.not_lazy.elim
      | dead => cases F <;> simp [Frame.plug] at hsf
      | dropB | o4Bind =>
        cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at hsf
        exact (hnv₀ (hsf.1 ▸ .loc)).elim
      | bornS _ hl' _ hbs =>
        rw [hsf] at hpa
        obtain ⟨c, c', -, -, hi⟩ := hst.cell _ _ _ hl'
        have hv : Value _ := (show BornInv W _ _ c c' from hi).val
        rcases hbs with rfl | rfl | rfl | rfl | rfl
        · cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at hsf
          exact (hnv₀ (hsf.1 ▸ .loc)).elim
        · exact hpa.not_let_force_loc.elim
        · exact (hpa.no_settle .refl [.let_ _] _ _ (OkE.nil.cons' trivial) rfl).elim
        · cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at hsf
          exact (hnv₀ (hsf.1 ▸ hv)).elim
        · cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at hsf
          have htd : r = .todo := by
            rcases snoc_or E₀ with rfl | ⟨_, G, rfl⟩
            · exact hsf.1
            · rw [plug_snoc] at hsf; cases G <;> simp [Frame.plug] at hsf
          rcases hp with hh | ⟨_, h, -⟩
          · rw [htd] at hh; cases hh
          · rw [htd] at h; cases h
      | app _ h₁ h₂ =>
        subst hx
        cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        · exact wrap (F' := .appL _) trivial (fun hb => .app (by decide) hb h₂) h₁ hpb
        · have ⟨hf', _⟩ := pure_val h₁ rfl hF
            (hpa.of_frame (F := .appL _) trivial) (hpb.of_frame (F := .appL _) trivial)
          exact wrap (F' := .appR _) hf' (fun hb => .app (by decide) h₁ hb) h₂ hpb
      | rcons _ h₁ h₂ =>
        subst hx
        cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        · exact wrap (F' := .rconsL _ _) trivial (fun hb => .rcons (by decide) hb h₂) h₁ hpb
        · have ⟨he', _⟩ := pure_val h₁ rfl hF
            (hpa.of_frame (F := .rconsL _ _) trivial) (hpb.of_frame (F := .rconsL _ _) trivial)
          exact wrap (F' := .rconsR _ _) he' (fun hb => .rcons (by decide) h₁ hb) h₂ hpb
      | update _ h₁ h₂ =>
        subst hx
        cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        · exact wrap (F' := .updL _ _) trivial (fun hb => .update (by decide) hb h₂) h₁ hpb
        · have ⟨he', _⟩ := pure_val h₁ rfl hF
            (hpa.of_frame (F := .updL _ _) trivial) (hpb.of_frame (F := .updL _ _) trivial)
          exact wrap (F' := .updR _ _) he' (fun hb => .update (by decide) h₁ hb) h₂ hpb
      | appK | appV | rconsK | rconsV | updK | updV => cases hx
      | let_ h₁ h₂ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .let_ _) trivial (fun hb => .let_ hb h₂) h₁ hpb
      | proj h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .proj _) trivial (fun hb => .proj hb) h₁ hpb
      | match_ h₁ h₂ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .match_ _) trivial (fun hb => .match_ hb h₂) h₁ hpb
      | await h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .await) trivial (fun hb => .await hb) h₁ hpb
      | force h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .force) trivial (fun hb => .force hb) h₁ hpb
      | raise h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .raise) trivial (fun hb => .raise hb) h₁ hpb
      | try_ h₁ h₂ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .try_ _) trivial (fun hb => .try_ hb h₂) h₁ hpb
      | inside h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .inside _) trivial (fun hb => .inside hb) h₁ hpb
      | hwith hl' ha hr h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .hwith _ _ _ _ _) trivial (fun hb => .hwith hl' ha hr hb) h₁ hpb
      | hkeep hl' ha h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .hkeep _ _ _ _) trivial (fun hb => .hkeep hl' ha hb) h₁ hpb
      | hkarm hl' ha h₁ hc =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .hkarmA _ _ _ _ _) trivial (fun hb => .hkarm hl' ha hb hc) h₁ hpb
      | hctl hl' hk ha hr h₁ hc =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .hctlA _ _ _ _ _ _ _ _) trivial (fun hb => .hctl hl' hk ha hr hb hc) h₁ hpb
      | hres hl' ha h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .hresB _ _ _) trivial (fun hb => .hres hl' ha hb) h₁ hpb
      | hdone h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .hdone _) trivial (fun hb => .hdone hb) h₁ hpb
      | settle hl' h₁ =>
        subst hx; cases F <;> simp only [Frame.plug] at hsf <;> cases hsf
        exact wrap (F' := .settle _) trivial (fun hb => .settle hl' hb) h₁ hpb
      | _ => cases F <;> simp only [Frame.plug] at hsf <;> cases hsf

/-- **Pure step simulation**: a `PStep` of a pure left term is matched by zero or one `PStep` of
the pure right term in the same world `W`. -/
theorem pure_step_sim {μ₁ μ₂ : List Tm} (hst : StR W μ₁ μ₂)
    (hglob : ∀ g v, W.R.glob g = some v → v.Src)
    {e e₁ t : Tm} (hpa : PureEval W.R e) (hpb : PureEval W.R t) (hs : Sim W .any e t)
    (hstep : PStep W.R e e₁) :
    ∃ t₁, (t₁ = t ∨ PStep W.R t t₁) ∧ Sim W .any e₁ t₁ := by
  cases hstep with
  | head hE hh => exact pure_plug_step hst hglob (.inl hh) _ rfl hE hpa hpb hs
  | glob hE hg => exact pure_plug_step hst hglob (.inr ⟨_, rfl, hg⟩) _ rfl hE hpa hpb hs

/-- **Pure evaluation simulation**: if a pure term reaches a value `v`, a related pure term
reaches a related value `v'` in the same world `W`. -/
theorem pure_sim_val {μ₁ μ₂ : List Tm} (hst : StR W μ₁ μ₂)
    (hglob : ∀ g v, W.R.glob g = some v → v.Src)
    {e e' v : Tm} (hpe : PureEval W.R e) (hpe' : PureEval W.R e') (hs : Sim W .any e e')
    (hp : PSteps W.R e v) (hv : Value v) :
    ∃ v', PSteps W.R e' v' ∧ Value v' ∧ Sim W .any v v' := by
  induction hp generalizing e' with
  | refl =>
    have ⟨hv', _⟩ := pure_val hs rfl hv hpe hpe'
    exact ⟨e', .refl, hv', hs⟩
  | step s _ ih =>
    obtain ⟨t₁, ht₁, hs₁⟩ := pure_step_sim hst hglob hpe hpe' hs s
    rcases ht₁ with rfl | s'
    · exact ih (hpe.step s) hpe' hs₁ hv
    · obtain ⟨v', hp', hv', hsv⟩ := ih (hpe.step s) (hpe'.step s') hs₁ hv
      exact ⟨v', .step s' hp', hv', hsv⟩

end Flick.Equiv
