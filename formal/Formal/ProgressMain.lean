import Formal.Progress
import Formal.DictCanon

/-!
# Progress (core.md §4)

A closed well-typed term is a value, `raise v`, makes a step, or performs an operation
that neither a handler nor `Σ₀` answers — and then its effect is in the services of its
stage.
-/

namespace Flick

variable {S : Sig}

/-- A region whose body has finished closes, runs a finalizer, waits for a child,
or is not live. -/
theorem inside_final {root Λ μ p w D} (hw : Final w) :
    Value (.inside p w) ∨ NV S root Λ μ (.inside p w) D := by
  have step : ∀ {l o}, Step root l Λ μ (.inside p w) o →
      Value (.inside p w) ∨ NV S root Λ μ (.inside p w) D := fun h => .inr (.step ⟨_, _, h⟩)
  rcases live_or_dead Λ p with ⟨g, hg, hl⟩ | hd
  · rcases Classical.em (∃ k ∈ g.kids, ¬ Final k) with ⟨k, hk₁, hk₂⟩ | hk
    · exact .inr (.blocked ⟨[], _, OkE.nil, rfl, .inl ⟨p, w, g, rfl, hw, hg, hl, k, hk₁, hk₂⟩⟩)
    · have hk' : ∀ k ∈ g.kids, Final k := fun k hk₁ =>
        Classical.byContradiction fun hk₂ => hk ⟨k, hk₁, hk₂⟩
      rcases List.eq_nil_or_concat g.fins with hf | ⟨fs, f, hf⟩
      · exact step (Step.ofR (.close hg hl hw hk' hf))
      · exact step (Step.ofR (.finRun hg hl hw hk' (by rw [hf, List.concat_eq_append])))
  · exact .inr (.blocked ⟨[], _, OkE.nil, rfl,
      .inr ⟨p, hd, .inr (.inr (.inr (.inr (.inl ⟨w, rfl, hw⟩))))⟩⟩)

/-- A finished arm resumes the body or lets it go. -/
theorem hctl_progress {p ℓ κ E al r w k} {Ei : List Frame} {tl : Bool} (hE : OkE Ei)
    (hk : k = plug Ei (.hole p)) (hv : Value w) : ∃ e', PHead (.hctl p ℓ κ tl E al r w k) e' := by
  cases tl
  · exact ⟨_, .hctlDisc hE hk hv⟩
  · exact ⟨_, .hctlTail hE hk hv⟩

theorem progress {Λ μ root} (hroot : RootTy S root) :
    ∀ {Ψ Θ Γ e J}, Typ S Ψ Θ Γ e J → μ.length = Ψ.length → Γ = [] → ∀ {t φ}, J = .ex t φ →
      Value e ∨ NV S root Λ μ e φ.svc := by
  intro Ψ Θ Γ e J h
  have step : ∀ {e l o D}, Step root l Λ μ e o → Value e ∨ NV S root Λ μ e D :=
    fun h => .inr (.step ⟨_, _, h⟩)
  induction h with
  | var hi | allVar hi => intro _ hΓ; subst hΓ; simp at hi
  | lam => intros; exact .inl .lam
  | fix => intros; exact .inl .fix
  | lit => intros; exact .inl .lit
  | loc => intros; exact .inl .loc
  | const | perform => intros; exact .inl .const
  | kont => intros; exact .inl .kont
  | ctor => intros; exact .inl .ctor
  | rnil => intros; exact .inl .rnil
  | task => intros; exact .inl .task
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
  | errVal hv | allI hv => intros; exact .inl hv
  | anil => intro _ _ _ _ hJ; cases hJ
  | acons => intro _ _ _ _ hJ; cases hJ
  | aguard => intro _ _ _ _ hJ; cases hJ
  | aref => intro _ _ _ _ hJ; cases hJ
  | hnil => intro _ _ _ _ hJ; cases hJ
  | hcons => intro _ _ _ _ hJ; cases hJ
  | hguard | hanil | hatail | haabort | hactl => intro _ _ _ _ hJ; cases hJ
  | unw => intros; exact .inr (.unw ⟨_, _, _, rfl⟩)
  | hdl => intros; exact step (Step.ofR .hdl)
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
    · exact .inr (.raise ⟨_, hv, rfl⟩)
    · exact .inr (hn.frame' (F := .raise) trivial (nt id) (ni id))
  | raiseAdt _ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact .inr (.raise ⟨_, hv, rfl⟩)
    · exact .inr (hn.frame' (F := .raise) trivial (nt id) (ni id))
  | tryR _ _ hφ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.tryVal hv))
    cases hn with
    | raise h => obtain ⟨v, hv, rfl⟩ := h; exact step (Step.ofR (.tryRaise hv))
    | step h =>
      obtain ⟨_, _, hs⟩ := h
      rcases hs.frame (F := .try_ _) trivial with ⟨_, _, hs'⟩ | hb
      · exact step hs'
      · exact .inr (.blocked hb)
    | blocked hb => exact .inr (.blocked (hb.frame (F := .try_ _) trivial))
    | perform hp =>
      rcases hp.frame (root := root) (Λ := Λ) (μ := μ) (F := .try_ _) trivial
          (fun n h => .inr (Stage.le_svc hφ n h))
          (fun _ _ _ _ h => by simp [Frame.installs] at h) with h | h | h
      · obtain ⟨_, _, hs'⟩ := h; exact step hs'
      · exact .inr (.perform h)
      · exact .inr (.blocked h)
    | unw h =>
      obtain ⟨_, _, _, rfl⟩ := h
      exact step (Step.unw (E := []) (F := .try_ _) OkE.nil trivial (ni id) id id)
  | tryP _ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.tryVal hv))
    cases hn with
    | raise h => obtain ⟨v, hv, rfl⟩ := h; exact step (Step.ofR (.tryRaise hv))
    | step h =>
      obtain ⟨_, _, hs⟩ := h
      rcases hs.frame (F := .try_ _) trivial with ⟨_, _, hs'⟩ | hb
      · exact step hs'
      · exact .inr (.blocked hb)
    | blocked hb => exact .inr (.blocked (hb.frame (F := .try_ _) trivial))
    | perform hp => obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, hN⟩ := hp; exact hN.elim
    | unw h =>
      obtain ⟨_, _, _, rfl⟩ := h
      exact step (Step.unw (E := []) (F := .try_ _) OkE.nil trivial (ni id) id id)
  | handle hv _ hA _ _ _ =>
    intro hμ hΓ _ _ hJ; cases hJ; subst hΓ
    obtain ⟨_, _, _, hh⟩ := handle_progress (Λ := Λ) (μ := μ) hv hA
    exact step (Step.ofR hh)
  | inside _ _ hφ' _ _ hφ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
    have hD : ∀ n, _ → Frame.dom (.inside _) n ∨ _ := fun n h => by
        rcases Stage.le_svc hφ' n h with h | h | h
        · cases n with
          | atm a => exact .inl ⟨_, rfl, h⟩
          | _ => exact h.elim
        · exact .inr (hr' n h)
        · cases n with
          | ob N a => exact .inl (.inr ⟨_, rfl, h.1⟩)
          | obv v a => exact .inl ⟨_, rfl, h.1⟩
          | _ => exact h.elim
    rcases ih hμ hΓ rfl with hv | hn
    · exact inside_final (.inl hv)
    cases hn with
    | raise h => exact inside_final (.inr (.inl h))
    | unw h => exact inside_final (.inr (.inr h))
    | step h =>
      obtain ⟨_, _, hs⟩ := h
      rcases hs.frame (F := .inside _) trivial with ⟨_, _, hs'⟩ | hb
      · exact step hs'
      · exact .inr (.blocked hb)
    | blocked hb => exact .inr (.blocked (hb.frame (F := .inside _) trivial))
    | perform hp =>
      rcases hp.frame (root := root) (Λ := Λ) (μ := μ) (F := .inside _) trivial hD
          (fun _ _ _ _ h => by simp [Frame.installs] at h) with h | h | h
      · obtain ⟨_, _, hs'⟩ := h; exact step hs'
      · exact .inr (.perform h)
      · exact .inr (.blocked h)
  | hwith _ _ _ _ _ hφ₀ hc hφ _ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.hwithVal hv))
    obtain ⟨_, _, rfl, hr, _⟩ := Stage.rt_le hφ
    refine .inr (hn.frame (F := .hwith _ _ _ _ _) trivial (nt id) (ni id) (fun n h => ?_)
      (hpart := fun N o c ⟨a, b, hop⟩ hN hh _ => ?_))
    · exact ((FRel.installs (F := .hwith _ _ _ _ _) rfl hφ₀ hφ).svc n h).symm
    · cases hN
      exact hr _ (hc.2 fun hcomp => by
        have h₁ := hcomp o c a b hop
        simp [Frame.handles, h₁] at hh)
  | hctl _ _ _ _ _ _ _ hE hk _ _ _ _ _ hφ _ _ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · obtain ⟨_, hh⟩ := hctl_progress hE hk hv
      exact step (Step.ofP hh)
    obtain ⟨_, _, rfl, hr, _⟩ := Stage.rt_le hφ
    exact .inr (hn.frame (F := .hctlA _ _ _ _ _ _ _ _) trivial (nt id) (ni id) (fun n h => .inr (hr n h))
      ⟨_, hE, hk⟩)
  | hres _ _ hA ha _ _ hφ _ ih _ =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.hresVal hA ha hv))
    obtain ⟨_, _, rfl, hr, _⟩ := Stage.rt_le hφ
    exact .inr (hn.frame (F := .hresB _ _ _) trivial (nt id) (ni id) (fun n h => .inr (hr n h)) ⟨_, hA, ha⟩)
  | hkeep _ _ _ hφ₀ hc hnt hle hφ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.hkeepVal hv))
    obtain ⟨_, _, rfl, hr, _⟩ := Stage.rt_le hφ
    refine .inr (hn.frame (F := .hkeep _ _ _ _) trivial (nt id) (ni id)
      (fun n h => ((FRel.installs (F := .hkeep _ _ _ _) rfl hφ₀ hφ).svc n h).symm)
      (hpart := fun N o c ⟨a, b, hop⟩ hN hh _ => ?_))
    cases hN
    refine hr _ ?_
    by_cases hct : c = .tail
    · subst hct
      exact hc.2 fun hcomp => by
        have h₁ := hcomp o _ a b hop
        simp [Frame.handles, h₁] at hh
    · exact hnt ⟨o, c, a, b, hop, hct⟩
  | hkarm _ _ _ hE hk _ _ _ _ _ hle hφ _ _ _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.hkarmVal hE hk hv))
    obtain ⟨_, _, rfl, hr, _⟩ := Stage.rt_le (Stage.le_trans hle hφ)
    exact .inr (hn.frame (F := .hkarmA _ _ _ _ _) trivial (nt id) (ni id) (fun n h => .inr (hr n h))
      ⟨_, hE, hk⟩)
  | hdone _ ih =>
    intro hμ hΓ _ _ hJ; cases hJ
    rcases ih hμ hΓ rfl with hv | hn
    · exact step (Step.ofP (.hdoneVal hv))
    · exact .inr (hn.frame' (F := .hdone _) trivial (nt id) (ni id))

end Flick
