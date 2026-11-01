import Formal.NestStep

/-!
# Every step of a thread keeps `⊢ Λ`
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Nest.step {root : Name → Option Tm} {Λ : List Rgn} {μ : List Tm} {m : Tm}
    {i : Option (Nat × Nat)} {e : Tm} {l : Lbl} {Λ' : List Rgn} {μ' : List Tm} {e' : Tm} {X : List Ty}
    {home : Nat → Option (Nat × Nat)} {t : Ty} {φ : Stage} (hroot : RootTy S root)
    (hn : Nest Ψ ⟨Λ, μ, m⟩ anc B home) (hi : (⟨Λ, μ, m⟩ : Cfg).at i = some e)
    (he : Typ S Ψ Facts.age [] e (.ex t φ)) (hμ : StoreTy S Ψ μ) (hμ' : StoreTy S (Ψ ++ X) μ')
    (hΨ : ∀ u ∈ Ψ, u.YF Λ.length) (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∃ anc' B' home', Nest (Ψ ++ X) ((⟨Λ', μ', m⟩ : Cfg).put i e') anc' B' home' := by
  have nil : μ'.length = μ.length → X = [] := fun h => by
    have h₁ := hμ.1; have h₂ := hμ'.1
    simp only [List.length_append] at h₂
    exact List.eq_nil_of_length_eq_zero (by omega)
  have hW := (hn.thread i e hi).1
  cases hs with
  | pure hE hh =>
    rw [nil rfl, List.append_nil]
    have hrW := (within_of_plug hh.not_value hE hW).2
    exact ⟨anc, B, home, hn.sub hi hE hh.not_value (hh.noIns hrW) fun _ h => hh.has h⟩
  | askWith hE hl =>
    rw [nil rfl, List.append_nil]
    obtain ⟨K, Θ', t₁, φ₁, hr, _, hK, -⟩ := ctx_typing _ hE ask_not_value he
    obtain ⟨rfl, -, look⟩ := hK.resolve_left fun hp => by subst hp; exact (ask_inv hr).1
    have hvT := look _ _ hl
    have hvv := lookupE_value hE hl
    obtain ⟨hvr, hvl⟩ := Typ.sites hvv hvT
    refine ⟨anc, B, home, hn.redex hi hE ask_not_value (by simp [Tm.NoIns, Tm.Has]) (hvT.noIns hvv)
      (fun _ _ h => h) (fun hw _ => ?_) hn.cells⟩
    refine Tm.Within.of_has (hvT.noIns hvv) (fun p hp => (S.impl_reach _ _ (hvr p hp)).elim)
      fun l' hl' => ?_
    obtain ⟨A', -, -, t', ht', hB⟩ := hw.loc (lookupE_has _ hl hl')
    exact ⟨t', ht', hB.trans (funext fun x => propext
      ⟨fun h => (S.impl_reach _ _ (hvl l' t' hl' ht' x h.2)).elim,
        fun h => (S.impl_reach _ _ (hvl l' t' hl' ht' x h.2)).elim⟩)⟩
  | askRoot hE _ hr₀ =>
    rw [nil rfl, List.append_nil]
    obtain ⟨hvv, hvT⟩ := hroot _ _ hr₀
    obtain ⟨hvr, -⟩ := Typ.sites hvv hvT
    exact ⟨anc, B, home, hn.redex hi hE ask_not_value (by simp [Tm.NoIns, Tm.Has]) (hvT.noIns hvv)
      (fun _ _ h => h) (fun _ _ => Tm.Within.of_has (hvT.noIns hvv)
        (fun p hp => (S.impl_reach _ _ (hvr p hp)).elim)
        fun l hl => absurd (hvT.locs l hl) (Nat.not_lt_zero _)) hn.cells⟩
  | raise hE hF hnt hni hv =>
    rw [nil rfl, List.append_nil]
    have hrW := (within_of_plug (not_value_plug raise_not_value _) hE hW).2
    obtain ⟨hFW, hrv⟩ := Frame.within_of_plug hF raise_not_value hrW
    exact ⟨anc, B, home, hn.sub hi hE (not_value_plug raise_not_value _)
      (hFW.noIns_plug hni (hv.noIns hrv)) fun _ h => Frame.plug_has _ h⟩
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
      refine ⟨anc, B', home, hn.redex hi hE hnv (fun h => hvv.noIns hvW (h.elim (fun h => h.elim) id))
        (by simp [Tm.NoIns, Tm.Has]) hl (fun _ _ => ⟨X₀, by simp [hμ.1], hB'⟩) fun l' w hw => ?_⟩
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
      exact ⟨anc, B, home, hn.redex hi hE hnv hr0 hcn (fun _ _ h => h)
        (fun _ _ => hcW.moveT hvv hvT fun x hx => by rw [hB]; exact ⟨fun h => h.1, fun h => ⟨h, hx⟩⟩)
        hn.cells⟩
    | @refSet v l₀ _ _ hv hlt =>
      have hX := nil (by simp)
      subst hX
      rw [List.append_nil] at hμ' ⊢
      have hr0 : Tm.NoIns (.app (.app (.const .refSet) (.loc l₀)) v) :=
        fun h => hv.noIns hrW.2.1 (h.elim (fun h => h.elim (fun h => h.elim) (fun h => by cases h)) id)
      obtain ⟨t', ht', hB⟩ := hrW.loc0 (l := l₀) hr0 (by simp [Tm.Has])
      obtain ⟨hvv, hvT⟩ := hμ'.2 l₀ v t' (List.getElem?_set_self hlt) ht'
      refine ⟨anc, B, home, hn.redex hi hE hnv hr0 (by simp [Tm.NoIns, Tm.Has]) (fun _ _ h => h)
        (fun _ _ => trivial) fun l' w hw => ?_⟩
      by_cases hl' : l₀ = l'
      · subst hl'; rw [List.getElem?_set_self hlt] at hw; cases hw
        refine ⟨hrW.2.1.moveT hvv hvT fun x hx => ?_, hvv.noIns hrW.2.1⟩
        rw [hB]; exact ⟨fun h => ⟨h, hx⟩, fun h => h.1⟩
      · rw [List.getElem?_set_ne hl'] at hw; exact hn.cells l' w hw
    | await =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv hrW.2 fun _ h => h⟩
    | build hv hb =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim (hv.noIns hrW.1) hrW.2.2)
        fun _ h => h.elim (fun h => .inl (build_has _ hb h)) .inr⟩
    | provideEnv hθ =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim (hθ.value.noIns hrW.1) hrW.2.2)
        fun _ h => h⟩
    | tryRaise hv =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim (hv.noIns hrW.1) hrW.2.2)
        fun _ h => h.elim .inl (.inr ·)⟩
    | handleYes _ hm =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2)
        fun _ h => (subst_match_has hm h).elim (fun h => .inr (.inl h)) .inl⟩
    | handleNo =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2)
        fun _ h => h.elim .inl (.inr ∘ .inr)⟩
    | hguardYes _ hm =>
      rw [nil rfl, List.append_nil]
      refine ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2) fun _ h => ?_⟩
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
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2)
        fun _ h => h.elim .inl (.inr ∘ .inr ∘ .inr)⟩
    | handleNil =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (fun h => h.elim hrW.2.2.1 hrW.2.2.2) fun _ h => .inl h⟩
    | scope =>
      rw [nil rfl, List.append_nil]
      obtain ⟨anc', home', h'⟩ := hn.scope hi hE hΨ
      exact ⟨anc', B, home', h'⟩
    | open_ hg hl =>
      rw [nil rfl, List.append_nil]
      refine ⟨anc, B, home, hn.grow hi hE hnv (by simp [Tm.NoIns, Tm.Has]) (fun _ h => .inr h) hg hl rfl
        ⟨[], by simp, by simp⟩ fun f hf => ?_⟩
      rcases List.mem_append.mp hf with hf | hf
      · exact hn.fins _ _ hg hl f hf
      · simp at hf; subst hf
        exact ⟨⟨trivial, hn.self _, .inl (by simp [Tm.NoIns, Tm.Has])⟩, by simp [Tm.NoIns, Tm.Has]⟩
    | op =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.sub hi hE hnv (by simp [Tm.NoIns, Tm.Has]) fun _ h => by cases h⟩
    | addFin hg hl hv =>
      rw [nil rfl, List.append_nil]
      obtain ⟨Θ', t', φ', hr, hΘ⟩ := hole_rt (RHead.addFin (μ := μ) hg hl hv) hE he
      obtain ⟨U, hU, hUp⟩ := owner_arg hr (.inl rfl) hΘ
      have htW := hrW.2.1
      have hw := hn.to_owner hi hWE hrW.1.2.1 hv hU hUp htW
      refine ⟨anc, B, home, hn.grow hi hE hnv (fun h => hv.noIns htW (h.elim (fun h => h.elim
        (fun h => h.elim) (fun h => by cases h)) id)) (fun _ h => by cases h) hg hl rfl
        ⟨[], by simp, by simp⟩ fun f hf => ?_⟩
      rcases List.mem_append.mp hf with hf | hf
      · exact hn.fins _ _ hg hl f hf
      · simp at hf; subst hf; exact ⟨hw, hv.noIns htW⟩
    | spawn hg hl hv =>
      rw [nil rfl, List.append_nil]
      obtain ⟨Θ', t', φ', hr, hΘ⟩ := hole_rt (RHead.spawn (μ := μ) hg hl hv) hE he
      obtain ⟨U, hU, hUp⟩ := owner_arg hr (.inr rfl) hΘ
      have htW := hrW.2.1
      have hw := hn.to_owner hi hWE hrW.1.2.1 hv hU hUp htW
      exact ⟨anc, B, home, hn.grow hi hE hnv (fun h => hv.noIns htW (h.elim (fun h => h.elim
        (fun h => h.elim) (fun h => by cases h)) id)) (fun _ h => by cases h) hg hl rfl
        ⟨[_], rfl, fun k hk => by simp at hk; subst hk; exact ⟨hw, hv.noIns htW⟩⟩
        fun f hf => hn.fins _ _ hg hl f hf⟩
    | finRun hg hl _ _ hfs =>
      rw [nil rfl, List.append_nil]
      exact ⟨anc, B, home, hn.grow0 hi hg hl rfl
        ⟨[_], rfl, fun k hk => by simp at hk; subst hk; exact hn.fins _ _ hg hl _ (by rw [hfs]; simp)⟩
        fun f hf => hn.fins _ _ hg hl f (by rw [hfs]; exact List.mem_append_left _ hf)⟩
    | close hg hl hw hk hfs =>
      rw [nil rfl, List.append_nil]
      obtain ⟨Θ', t', φ', hr, -⟩ := hole_rt (RHead.close (μ := μ) hg hl hw hk hfs) hE he
      exact ⟨anc, B, home, hn.close hi hE hw hg hk fun hw' hπ => close_move hr hw hw' hπ⟩

end Flick
