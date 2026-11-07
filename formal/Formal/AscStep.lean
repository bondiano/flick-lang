import Formal.KidTy

/-!
# Steps keep the `inside` forms younger than the handlers around them
-/

namespace Flick

variable {S : Sig}

/-- Every thread, child and finalizer has its `inside` forms after the handlers
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
  | hwithVal hv => exact ⟨Tm.Asc.of_noIns h.2.1, va hv h.2.2.1 n, .inl (vn hv h.2.2.1)⟩
  | hkeepVal hv => exact va hv h.2.1 n
  | hkarmVal hE hk hv =>
    subst hk
    exact ⟨h.1, (asc_of_plug hole_not_value hE h.2.2.1).1.fill (va hv h.2.1 _), h.2.2.2⟩
  | hctlTail hE hk hv =>
    subst hk
    exact ⟨h.1, h.2.1, (asc_of_plug hole_not_value hE h.2.2.2.1).1.fill (va hv h.2.2.1 _), h.2.2.2.2⟩
  | hctlDisc hE hk hv =>
    subst hk
    exact ⟨h.1, h.2.1, (asc_of_plug hole_not_value hE h.2.2.2.1).1.fill (fun m => va hv h.2.2.1 m), h.2.2.2.2⟩
  | hresVal hA ha hv =>
    subst ha
    exact (asc_of_plug hole_not_value hA h.1).1.fill (va hv h.2 _)
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
    | (have h3 := h.2; simp only [Tm.NoIns, Tm.Has, not_or] at h3; exact ⟨vn ‹_› h.1, h3⟩)
    | simp

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
    all_goals subst hq; exact h _ (plug_frame_has hF (by simp [Frame.plug, Tm.Has]))

theorem CfgAsc.set {Λ : List Rgn} {μ : List Tm} {e e' : Tm} {p : Nat} {g g' : Rgn} (h : CfgAsc Λ μ e)
    (hg : Λ[p]? = some g) (hk : ∀ k ∈ g'.kids ++ g'.fins, k ∈ g.kids ++ g.fins ∨ k.Asc 0)
    (he : e'.Asc 0) : CfgAsc (setRgn Λ p g') μ e' := by
  refine ⟨he, fun q g'' hq k hk' => ?_, h.mem⟩
  rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
  · exact (hk k hk').elim (h.rgs _ _ hg k) id
  · exact h.rgs q g'' hq k hk'

/-- A step at the redex keeps the order of `inside` forms and handlers. -/
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
  | scopeRun hv =>
    exact noins fun h => vn hv hr.2.1 (Tm.has_scopeRun' h)
  | tryRaise hv => exact noins fun h => h.elim (vn hv hr.1) hr.2
  | hctlRaise hE hk hv =>
    subst hk
    exact same ⟨hr.1, hr.2.1, (asc_of_plug hole_not_value hE hr.2.2.2.1).1.fill
      (fun _ => Tm.Asc.of_noIns fun h => vn hv hr.2.2.1 h), hr.2.2.2.2⟩
  | hctlUnw hE hk =>
    subst hk
    exact same ⟨hr.1, hr.2.1, (asc_of_plug hole_not_value hE hr.2.2.2.1).1.fill (fun _ => hr.2.2.1), hr.2.2.2.2⟩
  | hkarmRaise hE hk hv =>
    subst hk
    exact same ⟨hr.1, (asc_of_plug hole_not_value hE hr.2.2.1).1.fill
      (fun _ => Tm.Asc.of_noIns fun h => vn hv hr.2.1 h), hr.2.2.2⟩
  | hkarmUnw hE hk =>
    subst hk
    exact same ⟨hr.1, (asc_of_plug hole_not_value hE hr.2.2.1).1.fill hr.2.1, hr.2.2.2⟩
  | hresRaise hA ha hv =>
    subst ha
    exact same ((asc_of_plug hole_not_value hA hr.1).1.fill
      (Tm.Asc.of_noIns fun h' => vn hv hr.2 h'))
  | hresUnw hA ha =>
    subst ha
    exact same ((asc_of_plug hole_not_value hA hr.1).1.fill hr.2)
  | hwithCatch => exact same (hr.2.2.1 n)
  | hdl =>
    have hm : ∀ (l : Nat) (v' : Tm), (μ ++ [.lam .todo])[l]? = some v' → v'.NoIns := by
      intro l v' hv'
      by_cases hl : l < μ.length
      · rw [List.getElem?_append_left hl] at hv'; exact hc.mem l v' hv'
      · rw [List.getElem?_append_right (by omega)] at hv'
        cases hj : l - μ.length with
        | zero => simp [hj] at hv'; subst hv'; simp [Tm.NoIns, Tm.Has]
        | succ => simp [hj] at hv'
    refine ⟨⟨hn, hr.1, hr.2.1, Tm.Asc.of_noIns hr.2.2, hn⟩, fun e' he => ⟨he, fun q g hq k hk => ?_, hm⟩⟩
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

/-- A step of a thread keeps the order of `inside` forms and handlers. -/
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
  | performRoot hE _ hr₀ hf hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug perform_not_value hE hc.main
    obtain ⟨hwv, hwt⟩ := hroot.svc _ _ hr₀
    have hfn : Tm.NoIns _ := fun h => hwt.noIns hwv (field_has hf h)
    exact ⟨hA.fill ⟨Tm.Asc.of_noIns hfn, Tm.Asc.of_noIns (hv.ascNoIns hr.2.1), .inl (hv.ascNoIns hr.2.1)⟩,
      fun _ => keep⟩
  | glob hE hg =>
    obtain ⟨hA, -⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    exact ⟨hA.fill (Tm.Asc.of_noIns ((hroot.glob _ _ hg).1 _)), fun _ => keep⟩
  | unw hE hF =>
    obtain ⟨hA, hr⟩ := asc_of_plug (not_value_plug (fun h => by cases h) _) hE hc.main
    obtain ⟨-, hr'⟩ := Frame.asc_of_plug hF (fun h => by cases h) hr
    exact ⟨hA.fill hr', fun _ => keep⟩
  | performAbort hE hEi _ ha hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2.1
    refine ⟨hA.fill ⟨hr.1, hr.2.1, hAi.fill (fun _ => Tm.Asc.of_noIns fun h => ?_), hr.2.2.2⟩, fun _ => keep⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performTail hE hEi _ ha hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2.1
    refine ⟨hA.fill ⟨hr.1, hr.2.1, Tm.Asc.of_noIns fun h => ?_, hAi.fill trivial, hr.2.2.2⟩, fun _ => keep⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performKept hE hEi _ ha hv _ =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.1
    refine ⟨hA.fill ⟨hr.1, Tm.Asc.of_noIns fun h => ?_, hAi.fill trivial, hr.2.2⟩, fun _ => keep⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performCtl hE hEi _ ha hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2.1
    have hm : ∀ (l : Nat) (v' : Tm), (μ ++ [.lam .todo])[l]? = some v' → v'.NoIns := by
      intro l v' hv'
      by_cases hl : l < μ.length
      · rw [List.getElem?_append_left hl] at hv'; exact hc.mem l v' hv'
      · rw [List.getElem?_append_right (by omega)] at hv'
        cases hj : l - μ.length with
        | zero => simp [hj] at hv'; subst hv'; simp [Tm.NoIns, Tm.Has]
        | succ => simp [hj] at hv'
    refine ⟨hA.fill ⟨hr.1, hr.2.1, Tm.Asc.of_noIns fun h => ?_, hAi.fill trivial, hr.2.2.2⟩,
      fun x hx => ⟨hx, hc.rgs, hm⟩⟩
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha h)
    · simp at hw
      rcases hw with hw | hw <;> rw [hw] at h
      · exact hv.ascNoIns hri.2.1 h
      · simp [Tm.Has] at h
  | resume hE hA' hEi hk' _ hu =>
    subst hk'
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE hc.main
    obtain ⟨hAi, hri⟩ := asc_of_plug kont_not_value hA' hr.2.2.1
    have hk := (asc_of_plug hole_not_value hEi hr.2.2.2.1).1.fill (Tm.Asc.of_noIns (hu.ascNoIns hri.2.1))
    exact ⟨hA.fill ⟨hAi.fill trivial, hr.1, hr.2.1, hk, hr.2.2.2.2⟩, fun _ => keep⟩
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
      exact .inr (withs_asc hA (Tm.Asc.of_noIns (hv.ascNoIns hr.2.1 : Tm.NoIns _)))
  | spawn hE hg _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fin_not_value (.inr rfl)) hE hc.main
    refine ⟨hA.fill (Tm.Asc.of_noIns (by simp [Tm.NoIns, Tm.Has])), fun x hx => hc.set hg (fun k hk => ?_) hx⟩
    simp only [List.mem_append] at hk
    rcases hk with (hk | hk) | hk
    · exact .inl (List.mem_append_left _ hk)
    · simp at hk; subst hk
      exact .inr (withs_asc hA (Tm.Asc.of_noIns (hv.ascNoIns hr.2.1 : Tm.NoIns _)))
    · exact .inl (List.mem_append_right _ hk)

end Flick
