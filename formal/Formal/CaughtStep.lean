import Formal.Caught

/-!
# A step keeps every signal under its handler
-/

namespace Flick

variable {S : Sig} {hs : Nat → Nat → Prop} {cell : Nat → Nat}

theorem CaughtE.holeC_mono {H : Nat → Prop} {x : Nat} : ∀ {E : List Frame}, H x → CaughtE.holeC H E x
  | [], h => h
  | F :: E, h => by cases F <;> simp only [CaughtE.holeC] <;> first | exact holeC_mono h | exact .inl (holeC_mono h)

theorem Tm.Caught.noIns {e : Tm} {H : Nat → Prop} (hn : e.NoIns) : e.Caught hs cell H := of_noIns hn H

/-- A pure step at the redex keeps its signals under their handlers. -/
theorem PHead.caught {r r' : Tm} {n : Nat} {H : Nat → Prop} (hh : PHead r r') (h : r.Caught hs cell H)
    (ha : r.Asc n) : r'.Caught hs cell H := by
  have vn : ∀ {x : Tm} {m}, Value x → x.Asc m → x.NoIns := fun hv hx => hv.ascNoIns hx
  have plain : r.NoIns → r'.Caught hs cell H := fun hn => Tm.Caught.noIns fun hi => hn (hh.has hi)
  cases hh with
  | hwithVal hv =>
    simp only [Tm.Caught] at h ⊢
    exact ⟨h.2.2.2.1, Tm.Caught.noIns (vn hv ha.2.2)⟩
  | hctlTail hE hk hv =>
    subst hk
    simp only [Tm.Caught] at h ⊢
    obtain ⟨h₁, h₂, h₃, h₄, -, h₆⟩ := h
    exact ⟨h₁, h₂, h₃, h₄, caught_plug.mpr ⟨(caught_plug.mp h₆).1, Tm.Caught.noIns (vn hv ha.2.2.1)⟩⟩
  | hctlDisc hE hk hv =>
    subst hk
    simp only [Tm.Caught] at h ⊢
    obtain ⟨h₁, h₂, h₃, h₄, -, h₆⟩ := h
    refine ⟨h₁, h₂, h₃, h₄, caught_plug.mpr ⟨(caught_plug.mp h₆).1, ?_⟩⟩
    simp only [Tm.Caught]
    exact ⟨CaughtE.holeC_mono (.inr rfl), h₂, Tm.Caught.noIns (vn hv ha.2.2.1)⟩
  | hresVal hA ha' hv =>
    subst ha'
    simp only [Tm.Caught] at h ⊢
    exact caught_plug.mpr ⟨(caught_plug.mp h.1).1, Tm.Caught.noIns (vn hv ha.2)⟩
  | hdoneVal hv => exact h
  | _ =>
    apply plain
    simp only [Tm.Asc] at ha
    simp only [Tm.NoIns, Tm.Has, not_or]
    first
    | exact ⟨ha.1, vn ‹_› ha.2.1⟩
    | exact ⟨vn ‹_› ha.1, ha.2⟩
    | exact ⟨ha.1, ha.2⟩
    | exact vn ‹_› ha
    | exact ⟨vn ‹_› ha.1, vn ‹_› ha.2.1⟩
    | (have h3 := ha.2; simp only [Tm.NoIns, Tm.Has, not_or] at h3; exact ⟨vn ‹_› ha.1, h3⟩)
    | simp

theorem subst_noIns {x : Tm} {δ : List Tm} (hx : x.NoIns) (hδ : ∀ w ∈ δ, w.NoIns) :
    (x.subst (inst δ)).NoIns := fun hi => by
  rcases subst_inst_has hi with h | ⟨w, hw, h⟩
  · exact hx h
  · exact hδ w hw h

theorem rgs_new {Λ : List Rgn} {p : Nat} {g' : Rgn} (hg : (Λ ++ [Rgn.new])[p]? = some g') :
    ∀ k ∈ g'.kids ++ g'.fins, ∃ g, Λ[p]? = some g ∧ k ∈ g.kids ++ g.fins := by
  intro k hk
  by_cases hp : p < Λ.length
  · rw [List.getElem?_append_left hp] at hg; exact ⟨g', hg, hk⟩
  · rw [List.getElem?_append_right (by omega)] at hg
    cases hj : p - Λ.length with
    | zero => simp [hj] at hg; subst hg; simp [Rgn.new] at hk
    | succ => simp [hj] at hg

/-- The regions after a step at the redex: a child or finalizer is one from before, or
holds no run-time form. -/
def RgsFrom (Λ Λ' : List Rgn) : Prop :=
  ∀ (p : Nat) (g' : Rgn), Λ'[p]? = some g' → ∀ k ∈ g'.kids ++ g'.fins,
    (∃ g, Λ[p]? = some g ∧ k ∈ g.kids ++ g.fins) ∨ Tm.NoIns k

theorem RgsFrom.refl (Λ : List Rgn) : RgsFrom Λ Λ := fun _ g' hg _ hk => .inl ⟨g', hg, hk⟩

theorem RgsFrom.new (Λ : List Rgn) : RgsFrom Λ (Λ ++ [Rgn.new]) := fun _ _ hg k hk => .inl (rgs_new hg k hk)

theorem RgsFrom.set {Λ : List Rgn} {p : Nat} {g g'' : Rgn} (hg : Λ[p]? = some g)
    (hk : ∀ k ∈ g''.kids ++ g''.fins, k ∈ g.kids ++ g.fins ∨ Tm.NoIns k) : RgsFrom Λ (setRgn Λ p g'') := by
  intro q g' hq k hk'
  rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
  · exact (hk k hk').imp (fun h => ⟨g, hg, h⟩) id
  · exact .inl ⟨g', hq, hk'⟩

/-- A run-time step at the redex keeps its signals under their handlers; installing a
handler records its handlers around and its cell. -/
theorem RHead.caught {Λ : List Rgn} {μ : List Tm} {r Λ' μ' r'} {n : Nat} {H : Nat → Prop} (hh : RHead Λ μ r Λ' μ' r')
    (h : r.Caught hs cell H) (ha : r.Asc n) (hμ : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.NoIns)
    (hH : ∀ x, H x → x < Λ.length) :
    ∃ hs' cell', (∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) ∧ r'.Caught hs' cell' H ∧
      RgsFrom Λ Λ' := by
  have vn : ∀ {x : Tm} {m}, Value x → x.Asc m → x.NoIns := fun hv hx => hv.ascNoIns hx
  have keep : r'.Caught hs cell H → RgsFrom Λ Λ' →
      ∃ hs' cell', (∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) ∧ r'.Caught hs' cell' H ∧
        RgsFrom Λ Λ' := fun h' hr => ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, h', hr⟩
  have fresh : ¬ H Λ.length := fun h' => Nat.lt_irrefl _ (hH _ h')
  cases hh with
  | refMake hv => exact keep (Tm.Caught.noIns (by simp [Tm.NoIns, Tm.Has])) (.refl _)
  | refGet hl => exact keep (Tm.Caught.noIns (hμ _ _ hl)) (.refl _)
  | refSet => exact keep (Tm.Caught.noIns (by simp [Tm.NoIns, Tm.Has])) (.refl _)
  | await => exact keep h (.refl _)
  | tryRaise => exact keep h (.refl _)
  | handleYes _ hm =>
    exact keep (Tm.Caught.noIns fun hi => (subst_match_has hm hi).elim (fun h' => ha.2 (.inl h')) ha.1)
      (.refl _)
  | handleNo => exact keep ⟨h.1, h.2.2⟩ (.refl _)
  | hguardYes _ hm =>
    refine keep (Tm.Caught.noIns ?_) (.refl _)
    have hv := ha.1
    have hal := ha.2
    have hg : (Tm.subst (inst _) _).NoIns := fun hi => (subst_match_has hm hi).elim
      (fun h' => hal (.inl h')) hv
    have hb : (Tm.subst (inst _) _).NoIns := fun hi => (subst_match_has hm hi).elim
      (fun h' => hal (.inr (.inl h'))) hv
    simp only [Tm.NoIns, Tm.Has, not_or] at hal hv hg hb ⊢
    grind
  | hguardNo => exact keep ⟨h.1, h.2.2.2⟩ (.refl _)
  | handleNil => exact keep h.1 (.refl _)
  | scope =>
    exact keep ⟨fresh, Tm.Caught.noIns (subst_noIns ha fun w hw => by
      simp at hw; subst hw; simp [Tm.NoIns, Tm.Has])⟩ (.new _)
  | open_ hg =>
    refine keep trivial (.set hg fun k hk => ?_)
    simp only [List.mem_append] at hk
    rcases hk with hk | hk | hk
    · exact .inl (List.mem_append_left _ hk)
    · exact .inl (List.mem_append_right _ hk)
    · simp at hk; subst hk; exact .inr (by simp [Tm.NoIns, Tm.Has])
  | op => exact keep trivial (.refl _)
  | finRun hg _ _ _ hfs =>
    refine keep h (.set hg fun k hk => .inl ?_)
    simp only [List.mem_append, hfs] at hk ⊢
    rcases hk with (hk | hk) | hk
    · exact .inl hk
    · simp at hk; subst hk; simp
    · simp [hk]
  | close hg =>
    exact keep h.2 (.set hg fun k hk => .inl (by simp at hk ⊢; exact .inl hk))
  | hctlRaise hE hk hv =>
    subst hk
    obtain ⟨h₁, h₂, h₃, h₄, -, h₆⟩ := h
    refine keep ⟨h₁, h₂, h₃, h₄, caught_plug.mpr ⟨(caught_plug.mp h₆).1, ?_⟩⟩ (.refl _)
    exact ⟨CaughtE.holeC_mono (.inr rfl), h₂, Tm.Caught.noIns fun hi => vn hv ha.2.2.1 hi⟩
  | hctlUnw hE hk =>
    subst hk
    obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆⟩ := h
    refine keep ⟨h₁, h₂, h₃, h₄, caught_plug.mpr ⟨(caught_plug.mp h₆).1, ?_⟩⟩ (.refl _)
    exact ⟨CaughtE.holeC_mono (.inr rfl), h₂, h₁ ▸ h₅⟩
  | hresRaise hA ha' hv =>
    subst ha'
    exact keep (caught_plug.mpr ⟨(caught_plug.mp h.1).1, Tm.Caught.noIns fun hi => vn hv ha.2 hi⟩)
      (.refl _)
  | hresUnw hA ha' =>
    subst ha'
    obtain ⟨h₁, hq, hc, hb⟩ := h
    exact keep (caught_plug.mpr ⟨(caught_plug.mp h₁).1, CaughtE.holeC_mono hq, hc, hb⟩) (.refl _)
  | hwithCatch =>
    obtain ⟨h₁, -, -, -, -, -, h₇⟩ := h
    exact keep (h₁ ▸ h₇) (.refl _)
  | hdl =>
    refine ⟨fun q => if q = Λ.length then H else hs q, fun q => if q = Λ.length then μ.length else cell q,
      fun q hq => by simp [Nat.ne_of_lt hq], ⟨fresh, by simp, by simp,
        Tm.Caught.noIns ha.1, Tm.Caught.noIns ha.2.1, Tm.Caught.noIns ha.2.2⟩, .new _⟩

/-- Every thread of the regions and the main thread: no signal at the top. -/
structure CfgCaught (hs : Nat → Nat → Prop) (cell : Nat → Nat) (Λ : List Rgn) (e : Tm) : Prop where
  main : e.Caught hs cell (fun _ => False)
  rgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.Caught hs cell (fun _ => False)

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

theorem CfgCaught.ghostB {hs' : Nat → Nat → Prop} {cell' : Nat → Nat} {Λ : List Rgn} {μ : List Tm} {e : Tm}
    (h : CfgCaught hs cell Λ e) (hb : CfgBelow Λ μ e)
    (hg : ∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) : CfgCaught hs' cell' Λ e :=
  ⟨h.main.ghostB hb.main hg, fun p g hp k hk => (h.rgs p g hp k hk).ghostB (by
    rcases List.mem_append.mp hk with hk | hk
    · exact (hb.rgs p g hp).1 k hk
    · exact (hb.rgs p g hp).2 k hk) hg⟩

theorem CfgCaught.from {Λ Λ' : List Rgn} {e e' : Tm} (h : CfgCaught hs cell Λ e) (hr : RgsFrom Λ Λ')
    (he : e'.Caught hs cell (fun _ => False)) : CfgCaught hs cell Λ' e' :=
  ⟨he, fun p g' hg k hk => (hr p g' hg k hk).elim (fun ⟨g, hg, hk⟩ => h.rgs p g hg k hk)
    fun hn => Tm.Caught.noIns hn⟩

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
theorem Step.caught {root l Λ μ e Λ' μ' e'} (hroot : RootTy S root)
    (hst : Step root l Λ μ e (.cfg Λ' μ' e')) (hc : CfgCaught hs cell Λ e) (ha : CfgAsc Λ μ e)
    (hb : CfgBelow Λ μ e) :
    ∃ hs' cell', (∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) ∧ CfgCaught hs' cell' Λ' e' := by
  have same : ∀ {E : List Frame} {r x : Tm}, e = plug E r → (∀ H, x.Caught hs cell H) →
      ∃ hs' cell', (∀ p, p < Λ.length → hs' p = hs p ∧ cell' p = cell p) ∧
        CfgCaught hs' cell' Λ (plug E x) := fun he hx => ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩,
      ⟨caught_plug.mpr ⟨(caught_plug.mp (he ▸ hc.main)).1, hx _⟩, hc.rgs⟩⟩
  have hH : ∀ {E : List Frame} {r : Tm} {x : Nat}, e = plug E r → CaughtE.holeC (fun _ => False) E x →
      x < Λ.length := fun {E r x} he h => by
    rcases CaughtE.holeC_has (r := _) h with h | h
    · exact h.elim
    · exact hb.main x ((he ▸ h))
  cases hst with
  | pure hE hh =>
    obtain ⟨hA, hr⟩ := asc_of_plug hh.not_value hE ha.main
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    exact ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, hh.caught hcr hr⟩, hc.rgs⟩⟩
  | rt hE hh =>
    obtain ⟨hA, hr⟩ := asc_of_plug hh.not_value hE ha.main
    obtain ⟨hcE, hcr⟩ := caught_plug.mp hc.main
    obtain ⟨hs', cell', hg, hr', hrg⟩ := hh.caught hcr hr ha.mem fun x h => hH rfl h
    exact ⟨hs', cell', hg, (CfgCaught.ghostB hc hb hg).from hrg
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
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2
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
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2
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
    obtain ⟨hAi, hri⟩ := asc_of_plug perform_not_value hEi hr.2.2
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
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, trivial⟩, fun q g' hq k hk => ?_⟩⟩
    rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
    · simp only [List.mem_append] at hk
      rcases hk with hk | hk | hk
      · exact hc.rgs _ _ hg k (List.mem_append_left _ hk)
      · exact hc.rgs _ _ hg k (List.mem_append_right _ hk)
      · simp at hk; subst hk
        exact Tm.Caught.noIns fun h => hv.ascNoIns hr.2.1 h
    · exact hc.rgs _ _ hq k hk
  | spawn hE hg _ hv =>
    obtain ⟨hA, hr⟩ := asc_of_plug (fin_not_value (.inr rfl)) hE ha.main
    obtain ⟨hcE, -⟩ := caught_plug.mp hc.main
    refine ⟨hs, cell, fun _ _ => ⟨rfl, rfl⟩, ⟨caught_plug.mpr ⟨hcE, trivial⟩, fun q g' hq k hk => ?_⟩⟩
    rcases setRgn_get hg hq with ⟨rfl, rfl⟩ | ⟨_, hq⟩
    · simp only [List.mem_append] at hk
      rcases hk with (hk | hk) | hk
      · exact hc.rgs _ _ hg k (List.mem_append_left _ hk)
      · simp at hk; subst hk
        exact Tm.Caught.noIns fun h => hv.ascNoIns hr.2.1 h
      · exact hc.rgs _ _ hg k (List.mem_append_right _ hk)
    · exact hc.rgs _ _ hq k hk

end Flick
