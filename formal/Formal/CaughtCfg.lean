import Formal.CaughtStep

/-!
# A step of a thread keeps every signal under its handler
-/

namespace Flick

variable {S : Sig} {hs : Nat → Nat → Prop} {cell : Nat → Nat}

/-- Every thread of the regions and a thread with the regions below `n` around it: the main
thread has none, so it is no signal at its top. -/
structure CfgCaught (hs : Nat → Nat → Prop) (cell : Nat → Nat) (n : Nat) (Λ : List Rgn) (e : Tm) : Prop where
  main : e.Caught hs cell (· < n)
  rgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → ∀ k ∈ g.kids ++ g.fins,
    ∃ m, m ≤ Λ.length ∧ k.Caught hs cell (· < m)

theorem Tm.Caught.ghostB {hs' : Nat → Nat → Prop} {cell' : Nat → Nat} {e : Tm} {H : Nat → Prop} {n : Nat}
    (h : e.Caught hs cell H) (hb : e.Below n) (hg : ∀ p, p < n → hs' p = hs p ∧ cell' p = cell p) :
    e.Caught hs' cell' H :=
  h.ghost fun p hp => hg p (hb p hp)

theorem CaughtE.ghostB {hs' : Nat → Nat → Prop} {cell' : Nat → Nat} {E : List Frame} {x : Tm}
    {H : Nat → Prop} {n : Nat} (h : CaughtE hs cell H E) (hb : (plug E x).Below n)
    (hg : ∀ p, p < n → hs' p = hs p ∧ cell' p = cell p) : CaughtE hs' cell' H E := by
  have h₀ := (caught_plug (hs := hs) (cell := cell) (e := .var 0) (E := E) (H := H)).mpr ⟨h, trivial⟩
  have h₁ := h₀.ghost (hs' := hs') (cell' := cell') fun p hp => hg p (hb p
    ((plug_has_inv (r := x) E hp).resolve_right (by simp [Tm.Has])))
  exact (caught_plug.mp h₁).1

theorem CfgCaught.ghostB {hs' : Nat → Nat → Prop} {cell' : Nat → Nat} {n : Nat} {Λ : List Rgn} {μ : List Tm} {e : Tm}
    (h : CfgCaught hs cell n Λ e) (hb : CfgBelow Λ μ e)
    (hg : ∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) : CfgCaught hs' cell' n Λ e :=
  ⟨h.main.ghostB hb.main hg, fun p g hp k hk =>
    let ⟨m, hm, hk'⟩ := h.rgs p g hp k hk
    ⟨m, hm, hk'.ghostB (by
      rcases List.mem_append.mp hk with hk | hk
      · exact (hb.rgs p g hp).1 k hk
      · exact (hb.rgs p g hp).2 k hk) hg⟩⟩

theorem CfgCaught.from {n : Nat} {Λ Λ' : List Rgn} {e e' : Tm} (h : CfgCaught hs cell n Λ e) (hr : RgsFrom Λ Λ')
    (hl : Λ.length ≤ Λ'.length) (he : e'.Caught hs cell (· < n)) : CfgCaught hs cell n Λ' e' :=
  ⟨he, fun p g' hg k hk => (hr p g' hg k hk).elim
    (fun ⟨g, hg, hk⟩ => let ⟨m, hm, hk'⟩ := h.rgs p g hg k hk; ⟨m, Nat.le_trans hm hl, hk'⟩)
    fun hn => ⟨0, Nat.zero_le _, Tm.Caught.noIns hn⟩⟩

/-- What a context holds of a handler it keeps: its cell. -/
theorem CaughtE.keep_cell {H : Nat → Prop} {q ℓ : Nat} {E' : Name} {al : Tm} :
    ∀ {E : List Frame}, CaughtE hs cell H E → ∀ {F : Frame}, F ∈ E → (F = .hkeep q ℓ E' al ∨ ∃ r, F = .hwith q ℓ E' al r) →
      cell q = ℓ
  | G :: E, ⟨hE, hG⟩, F, hF, hf => by
    rcases List.mem_cons.mp hF with rfl | hF
    · rcases hf with rfl | ⟨r, rfl⟩
      · exact hG.2.1
      · exact hG.2.1
    · exact CaughtE.keep_cell hE hF hf

/-- A child in the handlers of its context. -/
theorem Tm.Caught.of_withs {H : Nat → Prop} {p : Nat} :
    ∀ {E : List Frame} {x : Tm}, x.Caught hs cell H →
      (∀ F ∈ E, ∀ q ℓ E' al, (F = .hkeep q ℓ E' al ∨ ∃ r, F = .hwith q ℓ E' al r) →
        H q ∧ cell q = ℓ ∧ al.NoIns) →
      (Flick.withs p E x).Caught hs cell H
  | [], _, hx, _ => hx
  | F :: E, x, hx, hE => by
    simp only [Flick.withs]
    refine Tm.Caught.of_withs (x := F.wrap p x) ?_ fun G hG => hE G (List.mem_cons_of_mem _ hG)
    cases F with
    | hwith q ℓ E' al rf =>
      simp only [Frame.wrap]
      split
      · obtain ⟨h₁, h₂, h₃⟩ := hE _ List.mem_cons_self q ℓ E' al (.inr ⟨_, rfl⟩)
        exact ⟨h₁, h₂, Tm.Caught.noIns h₃, hx⟩
      · exact hx
    | hkeep q ℓ E' al =>
      simp only [Frame.wrap]
      split
      · obtain ⟨h₁, h₂, h₃⟩ := hE _ List.mem_cons_self q ℓ E' al (.inl rfl)
        exact ⟨h₁, h₂, Tm.Caught.noIns h₃, hx⟩
      · exact hx
    | _ => exact hx

/-- The arms of a handler of the context of a thread hold no run-time form. -/
theorem AscE.keep_noIns {n q ℓ : Nat} {E' : Name} {al : Tm} :
    ∀ {E : List Frame}, AscE n E → ∀ {F : Frame}, F ∈ E → (F = .hkeep q ℓ E' al ∨ ∃ r, F = .hwith q ℓ E' al r) →
      al.NoIns
  | G :: E, ⟨hE, hG⟩, F, hF, hf => by
    rcases List.mem_cons.mp hF with rfl | hF
    · rcases hf with rfl | ⟨r, rfl⟩
      · exact hG.1
      · exact hG.1
    · exact AscE.keep_noIns hE hF hf

/-- What a child of the regions now in place keeps of its context. -/
theorem withs_caught {Λ : List Rgn} {E : List Frame} {r x : Tm} {p n : Nat} {H : Nat → Prop}
    (hE : CaughtE hs cell H E) (hA : AscE n E) (hb : (plug E r).Below Λ.length) (hx : x.NoIns) :
    (withs p E x).Caught hs cell (· < Λ.length) := by
  refine Tm.Caught.of_withs (Tm.Caught.noIns hx) fun F hF q ℓ E' al hf =>
    ⟨hb _ (plug_frame_has hF ?_), hE.keep_cell hF hf, hA.keep_noIns hF hf⟩
  rcases hf with rfl | ⟨_, rfl⟩ <;> simp [Frame.plug, Tm.Has]

/-- A signal passes a frame that is not its handler's. -/
theorem unw_pass {F : Frame} {H : Nat → Prop} {p ℓ : Nat} {a : Tm} (hcF : F.Caught hs cell H)
    (hu : (Tm.unw p ℓ a).Caught hs cell (F.holeH H))
    (hnh : ¬ F.catches p ℓ) : (Tm.unw p ℓ a).Caught hs cell H := by
  obtain ⟨hp, hℓ, ha'⟩ := hu
  refine ⟨?_, hℓ, ha'⟩
  cases F with
  | hwith q ℓ'' =>
    rcases hp with hp | rfl
    · exact hp
    · exact (hnh ⟨rfl, hcF.2.1.symm.trans hℓ⟩).elim
  | _ => exact hp

/-- A step of a thread keeps every signal under its handler. -/
theorem Step.caught {root l Λ μ e Λ' μ' e'} {mode : TypingMode} (hroot : RootTyM S mode root)
    {n : Nat} (hst : Step root l Λ μ e (.cfg Λ' μ' e')) (hc : CfgCaught hs cell n Λ e) (ha : CfgAsc Λ μ e)
    (hb : CfgBelow Λ μ e) (hn : n ≤ Λ.length) :
    ∃ hs' cell', (∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) ∧ CfgCaught hs' cell' n Λ' e' := by
  have same : ∀ {E : List Frame} {r x : Tm}, e = plug E r → (∀ H, x.Caught hs cell H) →
      ∃ hs' cell', (∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) ∧
        CfgCaught hs' cell' n Λ (plug E x) := fun he hx => ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩,
      ⟨caught_plug.mpr ⟨(caught_plug.mp (he ▸ hc.main)).1, hx _⟩, hc.rgs⟩⟩
  have hH : ∀ {E : List Frame} {r : Tm} {x : Nat}, e = plug E r → CaughtE.holeC (· < n) E x →
      x < Λ.length := fun {E r x} he h => by
    rcases CaughtE.holeC_has (r := _) h with h | h
    · exact Nat.lt_of_lt_of_le h hn
    · exact hb.main x (he ▸ h)
  cases hst with
  | pure hE hh =>
    obtain ⟨hA, hr⟩ := asc_of_plug hh.not_value hE ha.main
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    exact ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, hh.caught hcr hr⟩, hc.rgs⟩⟩
  | rt hE hh =>
    obtain ⟨hA, hr⟩ := asc_of_plug hh.not_value hE ha.main
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨hs', cell', hg, hr', hrg⟩ := hh.caught hcr hr ha.mem fun x h => hH rfl h
    exact ⟨hs', cell', hg, (CfgCaught.ghostB hc hb hg).from hrg (hh.below ⟨hb.main.sub fun _ hp => plug_has _ hp, hb.mem, hb.rgs⟩).2
      (caught_plug.mpr ⟨hcE.ghostB hb.main hg, hr'⟩)⟩
  | glob hE hg => exact same rfl fun _ => Tm.Caught.noIns ((hroot.glob _ _ hg).1 _)
  | performRoot hE _ hr₀ hf hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug perform_not_value hE ha.main
    have hfn : Tm.NoIns _ := fun h => (hroot.svc _ _ hr₀).2.noIns (hroot.svc _ _ hr₀).1 (field_has hf h)
    exact same rfl fun _ => Tm.Caught.noIns (fun h => h.elim hfn (hv.ascNoIns hr.2.1))
  | raise hE hF _ _ _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (not_value_plug raise_not_value _) hE ha.main
    obtain ⟨-, hr'⟩ := Frame.asc_of_plug hF raise_not_value hr
    exact same rfl fun _ => Tm.Caught.noIns (hv.ascNoIns hr')
  | unw hE hF _ _ hnh =>
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨hcF, hu⟩ := Frame.caught_plug.mp hcr
    exact ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, unw_pass hcF hu hnh⟩, hc.rgs⟩⟩
  | performAbort hE hEi _ ha' hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE ha.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2.1
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨h₁, h₂, h₃, h₄, h₅⟩ := hcr
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, h₁, h₂, h₃, h₄,
      caught_plug.mpr ⟨(caught_plug.mp h₅).1, CaughtE.holeC_mono (.inr rfl), h₂, Tm.Caught.noIns ?_⟩⟩,
      hc.rgs⟩⟩
    intro h
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha' h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performTail hE hEi _ ha' hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE ha.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2.1
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨h₁, h₂, h₃, h₄, h₅⟩ := hcr
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, h₁, h₂, h₃, h₄, Tm.Caught.noIns ?_,
      caught_plug.mpr ⟨(caught_plug.mp h₅).1, trivial⟩⟩, hc.rgs⟩⟩
    intro h
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha' h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | performCtl hE hEi _ ha' hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE ha.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2.1
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨h₁, h₂, h₃, h₄, h₅⟩ := hcr
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, h₁, h₂, h₃, h₄, Tm.Caught.noIns ?_,
      caught_plug.mpr ⟨(caught_plug.mp h₅).1, trivial⟩⟩, hc.rgs⟩⟩
    intro h
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha' h)
    · simp at hw
      rcases hw with hw | hw <;> rw [hw] at h
      · exact hv.ascNoIns hri.2.1 h
      · simp [Tm.Has] at h
  | performKept hE hEi _ ha' hv _ =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE ha.main
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.1
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨h₁, h₂, h₃, h₅⟩ := hcr
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, h₁, h₂, h₃, Tm.Caught.noIns ?_,
      caught_plug.mpr ⟨(caught_plug.mp h₅).1, trivial⟩⟩, hc.rgs⟩⟩
    intro h
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hr.1 (Tm.armOf_has ha' h)
    · simp at hw; rw [hw] at h; exact hv.ascNoIns hri.2.1 h
  | resume hE hA' hEi hk' _ hu =>
    subst hk'
    obtain ⟨hA, hr⟩ := asc_of_plug (fun h => by cases h) hE ha.main
    obtain ⟨hAi, hri⟩ := asc_of_plug kont_not_value hA' hr.2.2.1
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆⟩ := hcr
    exact ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, caught_plug.mpr ⟨(caught_plug.mp h₅).1,
      trivial⟩, h₁, h₂, h₃, h₄, caught_plug.mpr ⟨(caught_plug.mp h₆).1, Tm.Caught.noIns
        (hu.ascNoIns hri.2.1)⟩⟩, hc.rgs⟩⟩
  | addFin hE hg _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fin_not_value (.inl rfl)) hE ha.main
    obtain ⟨hcE, -⟩ := caught_plug.mp hc.main
    have grow : ∀ {k : Tm} {p' : Nat} {g'' : Rgn}, (∃ m, m ≤ Λ.length ∧ k.Caught hs cell (· < m)) →
        ∃ m, m ≤ (setRgn Λ p' g'').length ∧ k.Caught hs cell (· < m) := fun h => by simpa [setRgn] using h
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, trivial⟩, fun q g' hq k hk => ?_⟩⟩
    rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
    · simp only [List.mem_append] at hk
      rcases hk with hk | hk | hk
      · exact grow (hc.rgs _ _ hg k (List.mem_append_left _ hk))
      · exact grow (hc.rgs _ _ hg k (List.mem_append_right _ hk))
      · simp at hk; subst hk
        exact ⟨Λ.length, by simp [setRgn], withs_caught hcE hA hb.main fun h => hv.ascNoIns hr.2.1 h⟩
    · exact grow (hc.rgs _ _ hq k hk)
  | spawn hE hg _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fin_not_value (.inr rfl)) hE ha.main
    obtain ⟨hcE, -⟩ := caught_plug.mp hc.main
    have grow : ∀ {k : Tm} {p' : Nat} {g'' : Rgn}, (∃ m, m ≤ Λ.length ∧ k.Caught hs cell (· < m)) →
        ∃ m, m ≤ (setRgn Λ p' g'').length ∧ k.Caught hs cell (· < m) := fun h => by simpa [setRgn] using h
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, trivial⟩, fun q g' hq k hk => ?_⟩⟩
    rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
    · simp only [List.mem_append] at hk
      rcases hk with (hk | hk) | hk
      · exact grow (hc.rgs _ _ hg k (List.mem_append_left _ hk))
      · simp at hk; subst hk
        exact ⟨Λ.length, by simp [setRgn], withs_caught hcE hA hb.main fun h => hv.ascNoIns hr.2.1 h⟩
      · exact grow (hc.rgs _ _ hg k (List.mem_append_right _ hk))
    · exact grow (hc.rgs _ _ hq k hk)

end Flick
