import Formal.Progress

/-!
# Progress (core.md §4)

A closed well-typed term is a value, `raise v`, makes a step, or asks for a
service that neither a `with` frame nor `Σ₀` provides — and then the service is
in the services of its stage.
-/

namespace Flick

variable {S : Sig}

theorem progress {μ root} :
    ∀ {Ψ Γ e J}, Typ S Ψ Γ e J → μ.length = Ψ.length → Γ = [] → ∀ {t φ}, J = .ex t φ →
      Value e ∨ NV root μ e φ.svc := by
  intro Ψ Γ e J h
  have step : ∀ {e l o D}, Step root l μ e o → Value e ∨ NV root μ e D :=
    fun h => .inr (.inr (.inl ⟨_, _, h⟩))
  induction h with
  | var hi => intro _ hΓ; subst hΓ; simp at hi
  | lam => intros; exact .inl .lam
  | fix => intros; exact .inl .fix
  | lit => intros; exact .inl .lit
  | loc => intros; exact .inl .loc
  | const => intros; exact .inl .const
  | ctor => intros; exact .inl .ctor
  | rnil => intros; exact .inl .rnil
  | enil => intros; exact .inl .enil
  | task => intros; exact .inl .task
  | econs hv hθ => intros; exact .inl (.econs hv hθ.value)
  | errVal hv => intros; exact .inl hv
  | anil => intro _ _ _ _ hJ; cases hJ
  | acons => intro _ _ _ _ hJ; cases hJ
  | aguard => intro _ _ _ _ hJ; cases hJ
  | hnil => intro _ _ _ _ hJ; cases hJ
  | hcons => intro _ _ _ _ hJ; cases hJ
  | hguard => intro _ _ _ _ hJ; cases hJ
  | todo => intros; exact step (Step.todo (E := []) OkE.nil)
  | letrec => intros; exact step (Step.ofP .letrec)
  | app hf he hψ _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hvf | hn
    · rcases ih₂ hμ hΓ rfl with hve | hn
      · subst hΓ; exact app_progress hμ hvf hve hf he hψ
      · exact .inr (hn.frame' (F := .appR _) hvf (nt id))
    · exact .inr (hn.frame' (F := .appL _) trivial (nt id))
  | letGen _ _ ih₁ _ =>
    intro hμ hΓ _ _ hJ; cases hJ; subst hΓ
    rcases ih₁ (by simpa using hμ) rfl rfl with hv | hn
    · exact step (Step.ofP (.let_ hv))
    · exact .inr (hn.frame (F := .let_ _) trivial (nt id) fun _ h => h.elim)
  | let_ _ _ ih₁ _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.let_ hv))
    · exact .inr (hn.frame' (F := .let_ _) trivial (nt id))
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · rcases ih₂ hμ hΓ rfl with hr | hn
      · exact .inl (.rcons hv hr)
      · exact .inr (hn.frame' (F := .rconsR _ _) hv (nt id))
    · exact .inr (hn.frame' (F := .rconsL _ _) trivial (nt id))
  | proj he hR _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨⟨w, hw⟩, _⟩ := (he.shape hv rfl).1.field rfl hR
      exact step (Step.ofP (.proj hv hw))
    · exact .inr (hn.frame' (F := .proj _) trivial (nt id))
  | update he hR _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · rcases ih₂ hμ hΓ rfl with hw | hn
      · subst hΓ
        obtain ⟨_, hset⟩ := (he.shape hv rfl).1.field rfl hR
        obtain ⟨_, hv'⟩ := hset _
        exact step (Step.ofP (.update hv hw hv'))
      · exact .inr (hn.frame' (F := .updR _ _) hv (nt id))
    · exact .inr (hn.frame' (F := .updL _ _) trivial (nt id))
  | match_ he hA hc ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨_, hh⟩ := match_progress hv he hA hc
      exact step (Step.ofP hh)
    · exact .inr (hn.frame' (F := .match_ _) trivial (nt id))
  | await he _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨_, _, _, rfl, -⟩ := canonical_eff hv he
      exact step (Step.ofR .await)
    · exact .inr (hn.frame' (F := .await) trivial (nt id))
  | raise _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact .inr (.inl ⟨_, hv, rfl⟩)
    · exact .inr (hn.frame' (F := .raise) trivial (nt id))
  | raiseAdt _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact .inr (.inl ⟨_, hv, rfl⟩)
    · exact .inr (hn.frame' (F := .raise) trivial (nt id))
  | tryR _ _ hφ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs
    · exact step (Step.ofP (.tryVal hv))
    · exact step (Step.ofR (.tryRaise hv))
    · obtain ⟨_, _, hs'⟩ := hs.frame (F := .try_ _) trivial; exact step hs'
    · rcases hs.frame (μ := μ) (F := .try_ _) trivial
          (fun n h => .inr (Stage.le_svc hφ n h)) with h | h
      · obtain ⟨_, _, hs'⟩ := h; exact step hs'
      · exact .inr (.inr (.inr h))
  | tryP _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs
    · exact step (Step.ofP (.tryVal hv))
    · exact step (Step.ofR (.tryRaise hv))
    · obtain ⟨_, _, hs'⟩ := hs.frame (F := .try_ _) trivial; exact step hs'
    · obtain ⟨_, _, _, _, _, _, hN⟩ := hs; exact hN.elim
  | handle hv _ hA _ _ =>
    intro hμ hΓ _ _ hJ; cases hJ; subst hΓ
    obtain ⟨_, _, hh⟩ := handle_progress (μ := μ) hv hA
    exact step (Step.ofR hh)
  | provideL hl _ _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨_, hb, -⟩ := canonical_layer hv hl
      exact step (Step.ofR (.build hv hb))
    · exact .inr (hn.frame' (F := .provide _) trivial (nt id))
  | provideE hl _ _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      exact step (Step.ofR (.provideEnv (canonical_env hv hl)))
    · exact .inr (hn.frame' (F := .provide _) trivial (nt id))
  | with_ hθv hθ _ hb hφ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.withVal hv))
    · subst hΓ
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine .inr (hn.frame (F := .with_ _) hθv (nt id) fun n h => ?_)
      rcases Stage.le_svc hb n h with h | h
      · obtain ⟨w, hw⟩ := env_lookup_some (canonical_env hθv hθ) hθ h
        exact .inl (by simp [Frame.dom, Frame.lookup, hw])
      · exact .inr (hr' n h)
  | bind _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.bind hv))
    · exact .inr (hn.frame' (F := .bind _) trivial (nt id))
  | join h₁ h₂ _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · rcases ih₂ hμ hΓ rfl with hw | hn
      · subst hΓ
        exact step (Step.ofP (.join (canonical_env hv h₁) (canonical_env hw h₂)))
      · exact .inr (hn.frame' (F := .joinR _) hv (nt id))
    · exact .inr (hn.frame' (F := .joinL _) trivial (nt id))

end Flick
