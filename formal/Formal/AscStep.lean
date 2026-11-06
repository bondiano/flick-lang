import Formal.KidTy

/-!
# Steps keep the `inside` forms younger than the `with` frames around them
-/

namespace Flick

variable {S : Sig}

/-- Every thread, child and finalizer has its `inside` forms after the `with` frames
around them; cells hold no run-time form. -/
structure CfgAsc (Λ : List Rgn) (μ : List Tm) (e : Tm) : Prop where
  main : e.Asc 0
  rgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.Asc 0
  mem : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.NoIns

theorem PHead.asc {r r' : Tm} {n : Nat} (hh : PHead r r') (h : r.Asc n) : r'.Asc n := by
  have vn : ∀ {x : Tm} {n}, Value x → x.Asc n → ¬ x.Has .ins := fun hv hx => hv.ascNoIns hx
  have va : ∀ {x : Tm} {n}, Value x → x.Asc n → ∀ m, x.Asc m :=
    fun hv hx _ => Tm.Asc.of_noIns (vn hv hx)
  have plain : r.NoIns → r'.Asc n := fun hn => Tm.Asc.of_noIns fun hi => hn (hh.has hi)
  cases hh with
  | withVal hv => exact Tm.Asc.of_noIns (hv.ascNoIns h.2)
  | hwithVal hv => exact ⟨Tm.Asc.of_noIns h.2.1, va hv h.2.2 n, .inl (vn hv h.2.2)⟩
  | hctlTail hv => exact ⟨h.1, h.2.1, Tm.Asc.fill (va hv h.2.2.1) h.2.2.2⟩
  | hctlDisc hv => exact ⟨va hv h.2.2.1 n, Tm.Asc.fill (v := .unw _) (fun _ => trivial) h.2.2.2⟩
  | hresVal hv => exact Tm.Asc.fill (va hv h.2) h.1
  | hdoneVal hv => exact va hv h n
  | _ =>
    apply plain
    simp only [Tm.Asc] at h
    simp only [Tm.NoIns, Tm.Has, not_or]
    first
    | exact ⟨h.1, vn ‹_› h.2.1⟩
    | exact ⟨vn ‹_› h.1, h.2⟩
    | exact ⟨h.1, h.2⟩
    | exact vn ‹_› h
    | exact ⟨vn ‹_› h.1, vn ‹_› h.2.1⟩
    | exact ⟨vn (IsEnv.value ‹_›) h.1, vn (IsEnv.value ‹_›) h.2.1⟩
    | (have h3 := h.2; simp only [Tm.NoIns, Tm.Has, not_or] at h3; exact ⟨vn ‹_› h.1, h3⟩)
    | simp

theorem envLookup_noIns {θ v : Tm} {N : Name} (hθ : θ.NoIns) (h : θ.envLookup N = some v) : v.NoIns :=
  fun hv => hθ (envLookup_has h hv)

theorem lookupE_noIns {n : Nat} {N : Name} {q : Nat} {v : Tm} :
    ∀ {E : List Frame}, AscE n E → lookupE E N = some (q, v) → v.NoIns
  | [], _, h => by simp [lookupE] at h
  | F :: E, ⟨hE, hF⟩, h => by
    simp only [lookupE] at h
    cases hl : F.lookup N with
    | some w =>
      rw [hl] at h; cases h
      cases F with
      | with_ q' θ =>
        simp only [Frame.lookup, Option.map_eq_some_iff] at hl
        obtain ⟨w, hw, hwq⟩ := hl
        cases hwq
        exact envLookup_noIns hF hw
      | _ => simp [Frame.lookup] at hl
    | none => rw [hl] at h; exact lookupE_noIns hE h

theorem AscE.with_noIns {n q : Nat} {θ : Tm} : ∀ {E : List Frame}, AscE n E → .with_ q θ ∈ E → θ.NoIns
  | F :: E, ⟨hE, hF⟩, h => by
    rcases List.mem_cons.mp h with rfl | h
    · exact hF
    · exact AscE.with_noIns hE h

theorem plug_frame_prov {q : Nat} {θ : Tm} :
    ∀ {E : List Frame} {r : Tm}, .with_ q θ ∈ E → (plug E r).Has (.prov q)
  | F :: E, r, h => by
    rcases List.mem_cons.mp h with rfl | h
    · exact plug_has E (.inr (.inl rfl))
    · exact plug_frame_prov (E := E) (r := F.plug r) h

theorem Frame.has_of_hole {s : Site} (F : Frame) {r : Tm} (h : (F.plug (.var 0)).Has s) :
    (F.plug r).Has s := by
  cases F <;> simp only [Frame.plug, Tm.Has] at h ⊢ <;> grind

/-- What a frame of a context writes, besides its hole, is in the term. -/
theorem plug_frame_has {s : Site} :
    ∀ {E : List Frame} {r : Tm} {F : Frame}, F ∈ E → (F.plug (.var 0)).Has s → (plug E r).Has s
  | G :: E, r, F, h, hs => by
    rcases List.mem_cons.mp h with h | h
    · subst h; exact plug_has E (F.has_of_hole hs)
    · exact plug_frame_has (E := E) (r := G.plug r) h hs

/-- The bound at the hole of a thread is below the number of regions. -/
theorem ascBE_below {E : List Frame} {r : Tm} {n : Nat} (h : (plug E r).Below n) : ascBE 0 E ≤ n :=
  ascBE_le (Nat.zero_le _) fun F hF q hq => by
    cases F <;> simp only [Frame.bump, reduceCtorEq, Option.some.injEq] at hq
    case with_ => subst hq; exact h _ (.inr (plug_frame_prov hF))
    all_goals subst hq; exact h _ (.inl (plug_frame_has hF (by simp [Frame.plug, Tm.Has])))

theorem CfgAsc.set {Λ : List Rgn} {μ : List Tm} {e e' : Tm} {p : Nat} {g g' : Rgn} (h : CfgAsc Λ μ e)
    (hg : Λ[p]? = some g) (hk : ∀ k ∈ g'.kids ++ g'.fins, k ∈ g.kids ++ g.fins ∨ k.Asc 0)
    (he : e'.Asc 0) : CfgAsc (setRgn Λ p g') μ e' := by
  refine ⟨he, fun q g'' hq k hk' => ?_, h.mem⟩
  rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
  · exact (hk k hk').elim (h.rgs _ _ hg k) id
  · exact h.rgs q g'' hq k hk'

/-- A step at the redex keeps the order of `inside` forms and `with` frames. -/
theorem RHead.asc {Λ μ r Λ' μ' r'} {n : Nat} {e : Tm} (hh : RHead Λ μ r Λ' μ' r')
    (hc : CfgAsc Λ μ e) (hr : r.Asc n) (hn : n ≤ Λ.length) :
    r'.Asc n ∧ ∀ e', e'.Asc 0 → CfgAsc Λ' μ' e' := by
  have same : ∀ {x : Tm}, x.Asc n → x.Asc n ∧ ∀ e', e'.Asc 0 → CfgAsc Λ μ e' :=
    fun hx => ⟨hx, fun e' he => ⟨he, hc.rgs, hc.mem⟩⟩
  have noins : ∀ {x : Tm}, x.NoIns → x.Asc n ∧ ∀ e', e'.Asc 0 → CfgAsc Λ μ e' :=
    fun hx => same (Tm.Asc.of_noIns hx)
  have vn : ∀ {x : Tm} {m}, Value x → x.Asc m → x.NoIns := fun hv hx => hv.ascNoIns hx
  cases hh with
  | refMake hv =>
    refine ⟨Tm.Asc.of_noIns (by simp [Tm.NoIns, Tm.Has]), fun e' he => ⟨he, hc.rgs, fun l v' hv' => ?_⟩⟩
    by_cases hl : l < μ.length
    · rw [List.getElem?_append_left hl] at hv'; exact hc.mem l v' hv'
    · rw [List.getElem?_append_right (by omega)] at hv'
      cases hj : l - μ.length with
      | zero => simp [hj] at hv'; subst hv'; exact vn hv hr.2.1
      | succ => simp [hj] at hv'
  | refGet hl => exact noins (hc.mem _ _ hl)
  | @refSet v l₀ _ _ hv hlt =>
    refine ⟨Tm.Asc.of_noIns (by simp [Tm.NoIns, Tm.Has]), fun e' he => ⟨he, hc.rgs, fun l v' hv' => ?_⟩⟩
    by_cases hi : l₀ = l
    · subst hi; rw [List.getElem?_set_self hlt] at hv'; cases hv'; exact vn hv hr.2.1
    · rw [List.getElem?_set_ne hi] at hv'; exact hc.mem l v' hv'
  | await => exact noins hr
  | build hv hb =>
    refine same ⟨Tm.Asc.of_noIns fun h => vn hv hr.1 (build_has _ hb h), hr.2⟩
  | provideEnv hθ =>
    refine ⟨⟨hn, vn hθ.value hr.1, Tm.Asc.of_noIns hr.2⟩, fun e' he => ⟨he, fun q g hq k hk => ?_, hc.mem⟩⟩
    by_cases hql : q < Λ.length
    · rw [List.getElem?_append_left hql] at hq; exact hc.rgs q g hq k hk
    · rw [List.getElem?_append_right (by omega)] at hq
      cases hj : q - Λ.length with
      | zero => simp [hj] at hq; subst hq; simp [Rgn.new] at hk
      | succ => simp [hj] at hq
  | tryRaise hv => exact noins fun h => h.elim (vn hv hr.1) hr.2
  | hctlRaise hv =>
    exact same ⟨Tm.Asc.of_noIns (vn hv hr.2.2.1 : Tm.NoIns _), Tm.Asc.fill (v := .unw _) (fun _ => trivial) hr.2.2.2⟩
  | hctlUnw => exact same ⟨trivial, Tm.Asc.fill (v := .unw _) (fun _ => trivial) hr.2.2.2⟩
  | hresRaise hv =>
    refine same (Tm.Asc.fill ?_ hr.1)
    exact fun _ => Tm.Asc.of_noIns fun h' => vn hv hr.2 h'
  | hresUnw => exact same (Tm.Asc.fill (v := .unw _) (fun _ => trivial) hr.1)
  | habCatch => exact same hr.1
  | hdl =>
    refine ⟨⟨hn, hr.1, hr.2.1, Tm.Asc.of_noIns hr.2.2⟩, fun e' he => ⟨he, fun q g hq k hk => ?_, hc.mem⟩⟩
    by_cases hql : q < Λ.length
    · rw [List.getElem?_append_left hql] at hq; exact hc.rgs q g hq k hk
    · rw [List.getElem?_append_right (by omega)] at hq
      cases hj : q - Λ.length with
      | zero => simp [hj] at hq; subst hq; simp [Rgn.new] at hk
      | succ => simp [hj] at hq
  | handleYes _ hm =>
    exact noins fun h => (subst_match_has hm h).elim (fun h => hr.2 (.inl h)) hr.1
  | handleNo => exact noins fun h => h.elim hr.1 fun h => hr.2 (.inr h)
  | hguardYes _ hm =>
    refine noins fun h => ?_
    have hn₂ : ¬ (Tm.aguard _ _ _ _).Has .ins := hr.2
    simp only [Tm.Has, not_or] at hn₂
    simp only [Tm.Has] at h
    rcases h with h | h | h
    · rcases subst_match_has hm h with h | h
      · exact hn₂.1 h
      · exact hr.1 h
    · rcases subst_match_has hm h with h | h
      · exact hn₂.2.1 h
      · exact hr.1 h
    · simp at h; rcases h with h | h
      · exact hr.1 h
      · exact hn₂.2.2 h
  | hguardNo => exact noins fun h => h.elim hr.1 fun h => hr.2 (.inr (.inr h))
  | handleNil => exact noins hr.1
  | scope =>
    refine ⟨⟨hn, Tm.Asc.of_noIns fun h => ?_⟩, fun e' he => ⟨he, fun q g hq k hk => ?_, hc.mem⟩⟩
    · rcases subst_inst_has h with h | ⟨w, hw, h⟩
      · exact hr h
      · simp at hw; subst hw; cases h
    · by_cases hql : q < Λ.length
      · rw [List.getElem?_append_left hql] at hq; exact hc.rgs q g hq k hk
      · rw [List.getElem?_append_right (by omega)] at hq
        cases hj : q - Λ.length with
        | zero => simp [hj] at hq; subst hq; simp [Rgn.new] at hk
        | succ => simp [hj] at hq
  | open_ hg =>
    refine ⟨trivial, fun e' he => hc.set hg (fun k hk => ?_) he⟩
    simp only [List.mem_append] at hk
    rcases hk with hk | hk | hk
    · exact .inl (List.mem_append_left _ hk)
    · exact .inl (List.mem_append_right _ hk)
    · simp at hk; subst hk; exact .inr (Tm.Asc.of_noIns (by simp [Tm.NoIns, Tm.Has]))
  | op => exact noins (by simp [Tm.NoIns, Tm.Has])
  | finRun hg _ _ _ hfs =>
    refine ⟨hr, fun e' he => hc.set hg (fun k hk => .inl ?_) he⟩
    simp only [List.mem_append, hfs] at hk ⊢
    rcases hk with (hk | hk) | hk
    · exact .inl hk
    · simp at hk; subst hk; simp
    · simp [hk]
  | close hg =>
    exact ⟨hr.2, fun e' he => hc.set hg (fun k hk => .inl (by simp at hk ⊢; exact .inl hk)) he⟩

/-- A step of a thread keeps the order of `inside` forms and `with` frames. -/
theorem Step.asc {root l Λ μ e Λ' μ' e'} (hroot : RootTy S root)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) (hc : CfgAsc Λ μ e) (hb : e.Below Λ.length) :
    e'.Asc 0 ∧ ∀ x, x.Asc 0 → CfgAsc Λ' μ' x := by
  have keep : ∀ {x}, x.Asc 0 → CfgAsc Λ μ x := fun hx => ⟨hx, hc.rgs, hc.mem⟩
  cases hs with
  | pure hE hh =>
    obtain ⟨hA, hr⟩ := asc_of_plug hh.not_value hE hc.main
    exact ⟨hA.fill (hh.asc hr), fun _ => keep⟩
  | rt hE hh =>
    obtain ⟨hA, hr⟩ := asc_of_plug hh.not_value hE hc.main
    obtain ⟨hr', k⟩ := hh.asc hc hr (ascBE_below hb)
    exact ⟨hA.fill hr', k⟩
  | askWith hE hl _ =>
    obtain ⟨hA, -⟩ := asc_of_plug ask_not_value hE hc.main
    exact ⟨hA.fill (Tm.Asc.of_noIns (lookupE_noIns hA hl)), fun _ => keep⟩
  | askRoot hE _ hr₀ =>
    obtain ⟨hA, -⟩ := asc_of_plug ask_not_value hE hc.main
    exact ⟨hA.fill (Tm.Asc.of_noIns ((hroot.svc _ _ hr₀).2.noIns (hroot.svc _ _ hr₀).1)), fun _ => keep⟩
  | glob hE hg =>
    obtain ⟨hA, -⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    exact ⟨hA.fill (Tm.Asc.of_noIns ((hroot.glob _ _ hg).1 _)), fun _ => keep⟩
  | unw hE =>
    obtain ⟨hA, -⟩ := asc_of_plug (not_value_plug (fun h => by cases h) _) hE hc.main
    exact ⟨hA.fill trivial, fun _ => keep⟩
  | performAbort hE hEi _ ha hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2
    refine ⟨hA.fill ⟨Tm.Asc.of_noIns fun h => ?_, hAi.fill trivial⟩, fun _ => keep⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performTail hE hEi _ ha hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2
    refine ⟨hA.fill ⟨hr.1, hr.2.1, Tm.Asc.of_noIns fun h => ?_, hAi.fill trivial⟩, fun _ => keep⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performCtl hE hEi _ ha hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2
    refine ⟨hA.fill ⟨hr.1, hr.2.1, Tm.Asc.of_noIns fun h => ?_, hAi.fill trivial⟩, fun _ => keep⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw
      rcases hw with hw | hw <;> rw [hw] at h
      · exact hv.ascNoIns hri.2.1 h
      · simp [Tm.Has] at h
  | resume hE hA' hu =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug kont_not_value hA' hr.2.2.1
    have hk := Tm.Asc.fill (fun _ => Tm.Asc.of_noIns (hu.ascNoIns hri.2.1)) hr.2.2.2
    refine ⟨hA.fill ⟨hAi.fill trivial, hr.1, hr.2.1, ?_⟩, fun _ => keep⟩
    rwa [show ∀ a b : Nat, max (max a b) b = max a b from fun a b => by omega]
  | raise hE hF _ _ _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (not_value_plug raise_not_value _) hE hc.main
    obtain ⟨-, hr'⟩ := Frame.asc_of_plug hF raise_not_value hr
    exact ⟨hA.fill (Tm.Asc.of_noIns (hv.ascNoIns hr')), fun _ => keep⟩
  | addFin hE hg _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fin_not_value (.inl rfl)) hE hc.main
    refine ⟨hA.fill (Tm.Asc.of_noIns (by simp [Tm.NoIns, Tm.Has])), fun x hx => hc.set hg (fun k hk => ?_) hx⟩
    simp only [List.mem_append] at hk
    rcases hk with hk | hk | hk
    · exact .inl (List.mem_append_left _ hk)
    · exact .inl (List.mem_append_right _ hk)
    · simp at hk; subst hk
      exact .inr (withs_asc (x := .await _) (fun q θ h => hA.with_noIns h)
        (fun m => Tm.Asc.of_noIns (hv.ascNoIns hr.2.1 : Tm.NoIns _)) 0)
  | spawn hE hg _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fin_not_value (.inr rfl)) hE hc.main
    refine ⟨hA.fill (Tm.Asc.of_noIns (by simp [Tm.NoIns, Tm.Has])), fun x hx => hc.set hg (fun k hk => ?_) hx⟩
    simp only [List.mem_append] at hk
    rcases hk with (hk | hk) | hk
    · exact .inl (List.mem_append_left _ hk)
    · simp at hk; subst hk
      exact .inr (withs_asc (x := .await _) (fun q θ h => hA.with_noIns h)
        (fun m => Tm.Asc.of_noIns (hv.ascNoIns hr.2.1 : Tm.NoIns _)) 0)
    · exact .inl (List.mem_append_right _ hk)

end Flick
