import Formal.Progress
import Formal.DictCanon

/-!
# Progress (core.md §4)

A closed well-typed term is a value, `raise v`, makes a step, or asks for a
service that neither a `with` frame nor `Σ₀` provides — and then the service is
in the services of its stage.
-/

namespace Flick

variable {S : Sig}

/-- A region whose body has finished closes, runs a finalizer, waits for a child,
or is not live. -/
theorem inside_final {root Λ μ p w D} (hw : Final w) :
    Value (.inside p w) ∨ NV root Λ μ (.inside p w) D := by
  have step : ∀ {l o}, Step root l Λ μ (.inside p w) o →
      Value (.inside p w) ∨ NV root Λ μ (.inside p w) D := fun h => .inr (.inr (.inl ⟨_, _, h⟩))
  rcases live_or_dead Λ p with ⟨g, hg, hl⟩ | hd
  · rcases Classical.em (∃ k ∈ g.kids, ¬ Final k) with ⟨k, hk₁, hk₂⟩ | hk
    · exact .inr (.inr (.inr (.inr (.inl ⟨[], _, OkE.nil, rfl, .inl ⟨p, w, g, rfl, hw, hg, hl, k, hk₁, hk₂⟩⟩))))
    · have hk' : ∀ k ∈ g.kids, Final k := fun k hk₁ =>
        Classical.byContradiction fun hk₂ => hk ⟨k, hk₁, hk₂⟩
      rcases List.eq_nil_or_concat g.fins with hf | ⟨fs, f, hf⟩
      · exact step (Step.ofR (.close hg hl hw hk' hf))
      · exact step (Step.ofR (.finRun hg hl hw hk' (by rw [hf, List.concat_eq_append])))
  · exact .inr (.inr (.inr (.inr (.inl ⟨[], _, OkE.nil, rfl,
      .inr ⟨p, hd, .inr (.inr (.inr (.inr ⟨w, rfl, hw⟩)))⟩⟩))))

theorem progress {Λ μ root} (hroot : RootTy S root) :
    ∀ {Ψ Θ Γ e J}, Typ S Ψ Θ Γ e J → μ.length = Ψ.length → Γ = [] → ∀ {t φ}, J = .ex t φ →
      Value e ∨ NV root Λ μ e φ.svc := by
  intro Ψ Θ Γ e J h
  have step : ∀ {e l o D}, Step root l Λ μ e o → Value e ∨ NV root Λ μ e D :=
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
  | cap => intros; exact .inl .cap
  | own => intros; exact .inl .own
  | drec => intros; exact .inl .drec
  | dnil => intro _ _ _ _ hJ; cases hJ
  | dcons => intro _ _ _ _ hJ; cases hJ
  | glob hg =>
    intros
    obtain ⟨_, hv⟩ := Option.isSome_iff_exists.mp (hroot.globs _ _ hg)
    exact step (Step.glob (E := []) OkE.nil hv)
  | scope => intros; exact step (Step.ofR .scope)
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
      · exact .inr (hn.frame' (F := .appR _) hvf (nt id) (ni id))
    · exact .inr (hn.frame' (F := .appL _) trivial (nt id) (ni id))
  | letGen _ _ ih₁ _ =>
    intro hμ hΓ _ _ hJ; cases hJ; subst hΓ
    rcases ih₁ (by simpa using hμ) rfl rfl with hv | hn
    · exact step (Step.ofP (.let_ hv))
    · exact .inr (hn.frame (F := .let_ _) trivial (nt id) (ni id) fun _ h => h.elim)
  | let_ _ _ ih₁ _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.let_ hv))
    · exact .inr (hn.frame' (F := .let_ _) trivial (nt id) (ni id))
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · rcases ih₂ hμ hΓ rfl with hr | hn
      · exact .inl (.rcons hv hr)
      · exact .inr (hn.frame' (F := .rconsR _ _) hv (nt id) (ni id))
    · exact .inr (hn.frame' (F := .rconsL _ _) trivial (nt id) (ni id))
  | proj he hR _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨⟨w, hw⟩, _⟩ := (he.shape hv rfl).1.field rfl hR
      exact step (Step.ofP (.proj hv hw))
    · exact .inr (hn.frame' (F := .proj _) trivial (nt id) (ni id))
  | dproj he hc₀ _ hl _ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨r, _, _, _, rfl, hU, hc, _, hr, -⟩ := canonical_dict hv he
      obtain rfl := Ty.spine_inj hU
      rw [hc₀] at hc; cases hc
      obtain ⟨w, hw, -⟩ := hr.dfs_field (by rw [List.lookup_map_snd, hl]; rfl)
      exact step (Step.ofP (.proj hv hw))
    · exact .inr (hn.frame' (F := .proj _) trivial (nt id) (ni id))
  | update he hR _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · rcases ih₂ hμ hΓ rfl with hw | hn
      · subst hΓ
        obtain ⟨_, hset⟩ := (he.shape hv rfl).1.field rfl hR
        obtain ⟨_, hv'⟩ := hset _
        exact step (Step.ofP (.update hv hw hv'))
      · exact .inr (hn.frame' (F := .updR _ _) hv (nt id) (ni id))
    · exact .inr (hn.frame' (F := .updL _ _) trivial (nt id) (ni id))
  | match_ he hA hc ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨_, hh⟩ := match_progress hv he hA hc
      exact step (Step.ofP hh)
    · exact .inr (hn.frame' (F := .match_ _) trivial (nt id) (ni id))
  | await he _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨_, _, _, rfl, -⟩ := canonical_eff hv he
      exact step (Step.ofR .await)
    · exact .inr (hn.frame' (F := .await) trivial (nt id) (ni id))
  | raise _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact .inr (.inl ⟨_, hv, rfl⟩)
    · exact .inr (hn.frame' (F := .raise) trivial (nt id) (ni id))
  | raiseAdt _ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact .inr (.inl ⟨_, hv, rfl⟩)
    · exact .inr (hn.frame' (F := .raise) trivial (nt id) (ni id))
  | tryR _ _ hφ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs | hb
    · exact step (Step.ofP (.tryVal hv))
    · exact step (Step.ofR (.tryRaise hv))
    · rcases hs.frame (F := .try_ _) trivial with ⟨_, _, hs'⟩ | hd
      · exact step hs'
      · exact .inr (.inr (.inr (.inr (.inr hd))))
    · rcases hs.frame (Λ := Λ) (μ := μ) (F := .try_ _) trivial
          (fun n h => .inr (Stage.le_svc hφ n h)) with h | h | h
      · obtain ⟨_, _, hs'⟩ := h; exact step hs'
      · exact .inr (.inr (.inr (.inl h)))
      · exact .inr (.inr (.inr (.inr (.inr h))))
    · exact .inr (.inr (.inr (.inr (hb.frame (F := .try_ _) trivial))))
  | tryP _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs | hb
    · exact step (Step.ofP (.tryVal hv))
    · exact step (Step.ofR (.tryRaise hv))
    · rcases hs.frame (F := .try_ _) trivial with ⟨_, _, hs'⟩ | hd
      · exact step hs'
      · exact .inr (.inr (.inr (.inr (.inr hd))))
    · obtain ⟨_, _, _, _, _, _, hN⟩ := hs; exact hN.elim
    · exact .inr (.inr (.inr (.inr (hb.frame (F := .try_ _) trivial))))
  | handle hv _ hA _ _ _ =>
    intro hμ hΓ _ _ hJ; cases hJ; subst hΓ
    obtain ⟨_, _, _, hh⟩ := handle_progress (Λ := Λ) (μ := μ) hv hA
    exact step (Step.ofR hh)
  | provideL hl _ _ _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      obtain ⟨_, hb, -⟩ := canonical_layer hv hl
      exact step (Step.ofR (.build hv hb))
    · exact .inr (hn.frame' (F := .provide _) trivial (nt id) (ni id))
  | provideE hl _ _ _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · subst hΓ
      exact step (Step.ofR (.provideEnv (canonical_env hv hl)))
    · exact .inr (hn.frame' (F := .provide _) trivial (nt id) (ni id))
  | @with_ θ _ _ _ ρ _ _ _ q _ _ _ hθv hθ _ hb hφ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.withVal hv))
    · subst hΓ
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine .inr (hn.frame (F := .with_ q θ) hθv (nt id) (ni id) fun n h => ?_)
      have dom : ∀ N, ρ (.nm N) → ((Frame.with_ q θ).lookup N).isSome := fun N h => by
        obtain ⟨w, hw⟩ := env_lookup_some (canonical_env hθv hθ) hθ h
        simp [Frame.lookup, hw]
      have h := Stage.le_svc hb n h
      cases n with
      | nm N =>
        by_cases hN : ρ (.nm N)
        · exact .inl (dom N hN)
        · exact .inr (hr' _ (h.2 hN))
      | ob N a =>
        by_cases hN : ρ (.nm N)
        · exact .inl (.inl (dom N hN))
        · exact .inr (hr' _ (h.2 hN))
      | atm a | need _ a | rv a | obv _ a => exact .inr (hr' _ h)
  | bind _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.bind hv))
    · exact .inr (hn.frame' (F := .bind _) trivial (nt id) (ni id))
  | join h₁ h₂ _ ih₁ ih₂ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih₁ hμ hΓ rfl with hv | hn
    · rcases ih₂ hμ hΓ rfl with hw | hn
      · subst hΓ
        exact step (Step.ofP (.join (canonical_env hv h₁) (canonical_env hw h₂)))
      · exact .inr (hn.frame' (F := .joinR _) hv (nt id) (ni id))
    · exact .inr (hn.frame' (F := .joinL _) trivial (nt id) (ni id))
  | inside _ _ hφ' _ _ hφ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
    rcases ih hμ hΓ rfl with hv | ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs | hb
    · exact inside_final (.inl hv)
    · exact inside_final (.inr ⟨v, hv, rfl⟩)
    · rcases hs.frame (F := .inside _) trivial with ⟨_, _, hs'⟩ | hd
      · exact step hs'
      · exact .inr (.inr (.inr (.inr (.inr hd))))
    · have hD : ∀ n, _ → Frame.dom (.inside _) n ∨ _ := fun n h => by
        rcases Stage.le_svc hφ' n h with h | h | h | h
        · cases n with
          | atm a => exact .inl ⟨_, rfl, h⟩
          | _ => exact h.elim
        · exact .inr (hr' n h)
        · cases n with
          | ob N a => exact .inl (.inr ⟨_, rfl, h.1⟩)
          | obv v a => exact .inl ⟨_, rfl, h.1⟩
          | _ => exact h.elim
        · cases n with
          | need N a => exact .inl ⟨_, rfl, h⟩
          | _ => exact h.elim
      rcases hs.frame (Λ := Λ) (μ := μ) (F := .inside _) trivial hD with h | h | h
      · obtain ⟨_, _, hs'⟩ := h; exact step hs'
      · exact .inr (.inr (.inr (.inl h)))
      · exact .inr (.inr (.inr (.inr (.inr h))))
    · exact .inr (.inr (.inr (.inr (hb.frame (F := .inside _) trivial))))

end Flick
