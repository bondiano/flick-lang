import Formal.NestStep

/-!
# Every step of a thread keeps `⊢ Λ`
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Nest.step {root : Roots} {Λ : List Rgn} {μ : List Tm} {m : Tm}
    {i : Option (Nat × Nat)} {e : Tm} {l : Lbl} {Λ' : List Rgn} {μ' : List Tm} {e' : Tm} {X : List Ty}
    {home : Nat → Option (Nat × Nat)} {t : Ty} {φ : Stage} (hroot : RootTy S root)
    (hn : Nest Ψ ⟨Λ, μ, m⟩ anc B home) (hi : (⟨Λ, μ, m⟩ : Cfg).at i = some e)
    (he : Typ S Ψ Facts.age [] e (.ex t φ)) (hμ : StoreTy S Ψ μ) (hμ' : StoreTy S (Ψ ++ X) μ')
    (hΨ : ∀ u ∈ Ψ, u.YF Λ.length) (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∃ anc' B' home', Nest (Ψ ++ X) ((⟨Λ', μ', m⟩ : Cfg).put i e') anc' B' home' ∧
      ∀ q, q ≠ Λ.length → anc' q = anc q := by
  have keep : ∀ {Ψ' c' B' home'}, Nest Ψ' c' anc B' home' →
      ∃ anc' B'' home'', Nest Ψ' c' anc' B'' home'' ∧ ∀ q, q ≠ Λ.length → anc' q = anc q :=
    fun h => ⟨anc, _, _, h, fun _ _ => rfl⟩
  have nil : μ'.length = μ.length → X = [] := fun h => by
    have h₁ := hμ.1; have h₂ := hμ'.1
    simp only [List.length_append] at h₂
    exact List.eq_nil_of_length_eq_zero (by omega)
  have hW := (hn.thread i e hi).1
  have nf : ∀ {x : Tm}, x.NoIns → ∀ q, ¬ x.Has (.frame q) := fun h q => h.frame q
  cases hs with
  | @pure E r₀ r₁ _ _ hE hh =>
    rw [nil rfl, List.append_nil]
    have hrW := (within_of_plug hh.not_value hE hW).2
    by_cases hw : ∃ q θ v, r₀ = .with_ q θ v
    · obtain ⟨q, θ, v, rfl⟩ := hw
      have hnv := hh.not_value
      cases hh with
      | withVal hv =>
        have hvn := hv.noIns hrW.2.2
        refine keep (hn.redex hi hE hnv (fun q' h => ?_) (nf hvn) (fun _ _ h => h)
          (fun _ h => h.2.2) hn.cells)
        rcases h with h | h | h | h
        · cases h
        · cases h
        · exact hrW.1.frame q' h
        · exact hvn.frame q' h
    · obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing _ hE hh.not_value he
      exact keep (hn.sub hi hE hh.not_value
        (hh.noIns hrW hr fun q θ v h => hw ⟨q, θ, v, h⟩) fun _ h => hh.has h)
  | askWith hE hl _ =>
    rw [nil rfl, List.append_nil]
    obtain ⟨K, Θ', t₁, φ₁, hr, _, hK, -⟩ := ctx_typing _ hE ask_not_value he
    obtain ⟨rfl, -, look⟩ := hK.resolve_left fun hp => by subst hp; exact (ask_inv hr).1
    have hvT := look _ _ _ hl
    have hvv := lookupE_value hE hl
    obtain ⟨hvr, hvl⟩ := Typ.sites hvv hvT
    refine keep (hn.redex hi hE ask_not_value (fun q h => by simp [Tm.Has] at h)
      (nf (hvT.noIns hvv)) (fun _ _ h => h) (fun hw _ => ?_) hn.cells)
    refine Tm.Within.of_has (hvT.noIns hvv) (fun p hp => (S.impl_reach _ _ (hvr p hp)).elim)
      fun l' hl' => ?_
    obtain ⟨A', -, t', ht', hB⟩ := hw.loc (lookupE_has _ hl hl')
    exact ⟨t', ht', hB.trans (funext fun x => propext
      ⟨fun h => (S.impl_reach _ _ (hvl l' t' hl' ht' x h.2)).elim,
        fun h => (S.impl_reach _ _ (hvl l' t' hl' ht' x h.2)).elim⟩)⟩
  | askRoot hE _ hr₀ =>
    rw [nil rfl, List.append_nil]
    obtain ⟨hvv, hvT⟩ := hroot.svc _ _ hr₀
    obtain ⟨hvr, -⟩ := Typ.sites hvv hvT
    exact keep (hn.redex hi hE ask_not_value (fun q h => by simp [Tm.Has] at h)
      (nf (hvT.noIns hvv)) (fun _ _ h => h) (fun _ _ => Tm.Within.of_has (hvT.noIns hvv)
        (fun p hp => (S.impl_reach _ _ (hvr p hp)).elim)
        fun l hl => absurd (hvT.locs l hl) (Nat.not_lt_zero _)) hn.cells)
  | glob hE hg =>
    rw [nil rfl, List.append_nil]
    have hno := (hroot.glob _ _ hg).1
    exact keep (hn.redex hi hE (fun h => by cases h) (fun q h => by simp [Tm.Has] at h)
      (nf (hno _)) (fun _ _ h => h) (fun _ _ => Tm.Within.of_has (hno _)
        (fun p hp => (hno _ hp).elim) fun l hl => (hno _ hl).elim) hn.cells)
  | raise hE hF hnt hni hnc hv =>
    rw [nil rfl, List.append_nil]
    have hrW := (within_of_plug (not_value_plug raise_not_value _) hE hW).2
    obtain ⟨hFW, hrv⟩ := Frame.within_of_plug hF raise_not_value hrW
    rw [Frame.hole_eq hni] at hrv
    exact keep (hn.redex hi hE (not_value_plug raise_not_value _)
      (hFW.noFrame_plug hni hnc (nf (hv.noIns hrv))) (nf (hv.noIns hrv)) (fun _ _ h => h)
      (fun _ _ => hrv) hn.cells)
  | @addFin E _ p₀ _ t₀ _ hE hg hl hv =>
    rw [nil rfl, List.append_nil]
    have hnv := fin_not_value (w₁ := .own p₀) (w₂ := t₀) (.inl rfl)
    obtain ⟨hWE, hrW⟩ := within_of_plug hnv hE hW
    obtain ⟨K, Θ', _, _, hr, _, hK, -⟩ := ctx_typing _ hE hnv he
    obtain ⟨rfl, hΘ, -⟩ := hK.resolve_left fun hp => by subst hp; exact fin_not_pure hr (.inl rfl)
    rw [List.map_up_zero] at hr
    obtain ⟨U, hU, hUp⟩ := owner_arg hr (.inl rfl) fun C s h => (hΘ.2 C s h).elim id id
    have htW := hrW.2.1
    have hw := hn.to_owner hi hWE hrW.1.2.1 hv hU hUp htW
    have htn := hv.noIns htW
    refine keep (hn.grow hi hE hnv (fun h => htn (h.elim (fun h => h.elim
      (fun h => h.elim) (fun h => by cases h)) id)) (fun _ h => by cases h) hg hl rfl
      ⟨[], by simp, by simp⟩ fun f hf => ?_)
    rcases List.mem_append.mp hf with hf | hf
    · exact hn.fins _ _ hg hl f hf
    · simp at hf; subst hf
      exact withs_within (fun q θ h => hWE.with_frame h) (x := .await _) hw (htn.frame)
  | @spawn E _ p₀ _ t₀ _ hE hg hl hv =>
    rw [nil rfl, List.append_nil]
    have hnv := fin_not_value (w₁ := .own p₀) (w₂ := t₀) (.inr rfl)
    obtain ⟨hWE, hrW⟩ := within_of_plug hnv hE hW
    obtain ⟨K, Θ', _, _, hr, _, hK, -⟩ := ctx_typing _ hE hnv he
    obtain ⟨rfl, hΘ, -⟩ := hK.resolve_left fun hp => by subst hp; exact fin_not_pure hr (.inr rfl)
    rw [List.map_up_zero] at hr
    obtain ⟨U, hU, hUp⟩ := owner_arg hr (.inr rfl) fun C s h => (hΘ.2 C s h).elim id id
    have htW := hrW.2.1
    have hw := hn.to_owner hi hWE hrW.1.2.1 hv hU hUp htW
    have htn := hv.noIns htW
    exact keep (hn.grow hi hE hnv (fun h => htn (h.elim (fun h => h.elim
      (fun h => h.elim) (fun h => by cases h)) id)) (fun _ h => by cases h) hg hl rfl
      ⟨[_], rfl, fun k hk => by
        simp at hk; subst hk
        exact withs_within (fun q θ h => hWE.with_frame h) (x := .await _) hw (htn.frame)⟩
      fun f hf => hn.fins _ _ hg hl f hf)
  | @rt E _ _ _ _ _ _ hE hh =>
    have hnv := hh.not_value
    obtain ⟨hWE, hrW⟩ := within_of_plug hnv hE hW
    cases hh with
    | @refMake v _ _ hv =>
      have hlen : X.length = 1 := by
        have h₁ := hμ.1; have h₂ := hμ'.1; simp only [List.length_append, List.length_singleton] at h₂; omega
      obtain ⟨X₀, rfl⟩ := List.length_eq_one_iff.mp hlen
      obtain ⟨hvv, hvT⟩ := hμ'.2 μ.length v X₀ (by simp) (by simp [hμ.1])
      let B' : Nat → Nat → Prop := fun l =>
        if l = μ.length then (fun x => holeE (baseOf anc i) E x ∧ X₀.reach (.r x)) else B l
      have hB' : B' μ.length = fun x => holeE (baseOf anc i) E x ∧ X₀.reach (.r x) := if_pos rfl
      have hl : ∀ A l, LocOk Ψ B A l → LocOk (Ψ ++ [X₀]) B' A l := fun A l ⟨t, ht, hB⟩ => by
        have hlt : l < μ.length := hμ.1 ▸ getElem?_lt ht
        exact ⟨t, by rw [List.getElem?_append_left (getElem?_lt ht)]; exact ht,
          (if_neg (Nat.ne_of_lt hlt)).trans hB⟩
      have hvW := hrW.2.1
      refine keep (hn.redex hi hE hnv (nf fun h => hvv.noIns hvW (h.elim (fun h => h.elim) id))
        (fun q h => by simp [Tm.Has] at h) hl (fun _ _ => ⟨X₀, by simp [hμ.1], hB'⟩) fun l' w hw => ?_)
      by_cases hl' : l' < μ.length
      · rw [List.getElem?_append_left hl'] at hw
        obtain ⟨h₁, h₂⟩ := hn.cells l' w hw
        have hBl : B' l' = B l' := if_neg (Nat.ne_of_lt hl')
        rw [hBl]; exact ⟨h₁.mono hl fun _ _ => rfl, h₂⟩
      · rw [List.getElem?_append_right (by omega)] at hw
        cases hj : l' - μ.length with
        | zero =>
          rw [hj] at hw; simp at hw; subst hw
          have hl'' : l' = μ.length := by omega
          subst hl''
          refine ⟨?_, hvv.noIns hvW⟩
          rw [hB']
          exact (hvW.mono hl fun _ _ => rfl).moveT hvv hvT fun x hx => ⟨fun h => ⟨h, hx⟩, fun h => h.1⟩
        | succ => rw [hj] at hw; simp at hw
    | @refGet _ l₀ _ _ hl' =>
      rw [nil rfl, List.append_nil]
      obtain ⟨hcW, hcn⟩ := hn.cells _ _ hl'
      have hr0 : Tm.NoIns (.app (.const .refGet) (.loc l₀)) := by simp [Tm.NoIns, Tm.Has]
      obtain ⟨t', ht', hB⟩ := hrW.loc0 (l := l₀) hr0 (by simp [Tm.Has])
      obtain ⟨hvv, hvT⟩ := hμ.2 _ _ _ hl' ht'
      exact keep (hn.redex hi hE hnv (nf hr0) (nf hcn) (fun _ _ h => h)
        (fun _ _ => hcW.moveT hvv hvT fun x hx => by rw [hB]; exact ⟨fun h => h.1, fun h => ⟨h, hx⟩⟩)
        hn.cells)
    | @refSet v l₀ _ _ hv hlt =>
      have hX := nil (by simp)
      subst hX
      rw [List.append_nil] at hμ' ⊢
      have hr0 : Tm.NoIns (.app (.app (.const .refSet) (.loc l₀)) v) :=
        fun h => hv.noIns hrW.2.1 (h.elim (fun h => h.elim (fun h => h.elim) (fun h => by cases h)) id)
      obtain ⟨t', ht', hB⟩ := hrW.loc0 (l := l₀) hr0 (by simp [Tm.Has])
      obtain ⟨hvv, hvT⟩ := hμ'.2 l₀ v t' (List.getElem?_set_self hlt) ht'
      refine keep (hn.redex hi hE hnv (nf hr0) (fun q h => by simp [Tm.Has] at h) (fun _ _ h => h)
        (fun _ _ => trivial) fun l' w hw => ?_)
      by_cases hl' : l₀ = l'
      · subst hl'; rw [List.getElem?_set_self hlt] at hw; cases hw
        refine ⟨hrW.2.1.moveT hvv hvT fun x hx => ?_, hvv.noIns hrW.2.1⟩
        rw [hB]; exact ⟨fun h => ⟨h, hx⟩, fun h => h.1⟩
      · rw [List.getElem?_set_ne hl'] at hw; exact hn.cells l' w hw
    | await =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv hrW.2 fun _ h => h)
    | build hv hb =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim (hv.noIns hrW.1) hrW.2.2)
        fun _ h => h.elim (fun h => .inl (build_has _ hb h)) .inr)
    | @provideEnv θ₀ _ _ e₀ hθ =>
      rw [nil rfl, List.append_nil]
      obtain ⟨Θ', t', φ', hr, -⟩ := hole_rt (RHead.provideEnv (Λ := Λ) (μ := μ) (e := e₀) hθ) hE he
      obtain ⟨o, hθT⟩ : ∃ o, Typ S Ψ Θ' [] θ₀ (.ex (.env o) .pure) := by
        cases hr with
        | provideL hl =>
          obtain ⟨_, hb, -⟩ := canonical_layer hθ.value hl
          cases hθ <;> simp [Tm.build] at hb
        | provideE hl => exact ⟨_, hl.valueAny hθ.value _⟩
        | errVal hv => exact (hnv hv).elim
      obtain ⟨hθr, hθl⟩ := Typ.sites hθ.value hθT
      have hθn := hθ.value.noIns hrW.1
      have hθ0 : θ₀.Within Ψ B anc (fun _ => False) :=
        Tm.Within.of_has hθn (fun p hp => by simpa [Ty.reach, Caps.none] using hθr p hp) fun l hl => by
          obtain ⟨t, ht, hB⟩ := hrW.1.loc0 hθn hl
          refine ⟨t, ht, hB.trans (funext fun x => propext ⟨fun h => ?_, fun h => h.1.elim⟩)⟩
          have := hθl l t hl ht x h.2
          simp [Ty.reach, Caps.none] at this
      obtain ⟨anc', home', h', hag⟩ := hn.fresh (b := .with_ Λ.length θ₀ e₀) hi hE hnv
        (fun h => h.elim hθn hrW.2.2) (fun q h => by
          rcases h with h | h | h | h
          · cases h
          · cases h
          · exact hθn.frame q h
          · exact hrW.2.2.frame q h)
        ⟨hθn, hθ0, hrW.2.1.extend hrW.2.2 fun l t ht h =>
          Nat.lt_irrefl _ ((hΨ t (List.mem_of_getElem? ht)).reach h)⟩
      exact ⟨anc', B, home', h', hag⟩
    | tryRaise hv =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim (hv.noIns hrW.1) hrW.2.2)
        fun _ h => h.elim .inl (.inr ·))
    | handleYes _ hm =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2)
        fun _ h => (subst_match_has hm h).elim (fun h => .inr (.inl h)) .inl)
    | handleNo =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2)
        fun _ h => h.elim .inl (.inr ∘ .inr))
    | hguardYes _ hm =>
      rw [nil rfl, List.append_nil]
      refine keep (hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2) fun _ h => ?_)
      simp only [Tm.Has] at h ⊢
      rcases h with h | h | h
      · rcases subst_match_has hm h with h | h
        · exact .inr (.inl h)
        · exact .inl h
      · rcases subst_match_has hm h with h | h
        · exact .inr (.inr (.inl h))
        · exact .inl h
      · simp at h; grind
    | hguardNo =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2)
        fun _ h => h.elim .inl (.inr ∘ .inr ∘ .inr))
    | handleNil =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2) fun _ h => .inl h)
    | scope =>
      rw [nil rfl, List.append_nil]
      obtain ⟨anc', home', h', hag⟩ := hn.scope hi hE hΨ
      exact ⟨anc', B, home', h', hag⟩
    | open_ hg hl =>
      rw [nil rfl, List.append_nil]
      refine keep (hn.grow hi hE hnv (by simp [Tm.NoIns, Tm.Has]) (fun _ h => .inr h) hg hl rfl
        ⟨[], by simp, by simp⟩ fun f hf => ?_)
      rcases List.mem_append.mp hf with hf | hf
      · exact hn.fins _ _ hg hl f hf
      · simp at hf; subst hf
        exact ⟨⟨trivial, hn.self _, .inl (by simp [Tm.NoIns, Tm.Has])⟩, fun q h => by simp [Tm.Has] at h⟩
    | op =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (by simp [Tm.NoIns, Tm.Has]) fun _ h => by cases h)
    | finRun hg hl _ _ hfs =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.grow0 hi hg hl rfl
        ⟨[_], rfl, fun k hk => by simp at hk; subst hk; exact hn.fins _ _ hg hl _ (by rw [hfs]; simp)⟩
        fun f hf => hn.fins _ _ hg hl f (by rw [hfs]; exact List.mem_append_left _ hf))
    | close hg hl hw hk hfs =>
      rw [nil rfl, List.append_nil]
      obtain ⟨Θ', t', φ', hr, -⟩ := hole_rt (RHead.close (μ := μ) hg hl hw hk hfs) hE he
      exact keep (hn.close hi hE hw hg hk fun hw' hπ => close_move hr hw hw' hπ)
    | hctlRaise _ | hctlUnw | hresRaise _ | hresUnw | habCatch | hdl =>
      obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing _ hE hnv he
      cases hr with | errVal hv => exact (hnv hv).elim
  | unw hE hF _ _ _ =>
    have nv : ∀ q, ¬ Value (.unw q) := fun _ h => by cases h
    obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing _ hE (not_value_plug (nv _) _) he
    obtain ⟨_, _, _, _, hr', -⟩ := frame_typing (nv _) _ hF hr
    cases hr' with | errVal hv => cases hv
  | performAbort hE _ _ _ _ | performTail hE _ _ _ _ | performCtl hE _ _ _ _ | resume hE _ _ =>
    obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing _ hE (fun h => by cases h) he
    cases hr with | errVal hv => cases hv

end Flick
