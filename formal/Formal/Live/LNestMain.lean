import Formal.Live.LNestStep

/-!
# Every step of a thread keeps `⊢ Λ` of run-time forms

Values hold no run-time form, so they go anywhere. A step needs from the typing only the
services and globals of the root, the length of the store, that a signal does not unwind
to the region it closes, and two facts about names: the owner given work is a region of
the place of the hole (`hown`), and the arms' row of a new handler names the regions of
its place and the new region (`hcell`).
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem LNest.step {root : Roots} {Λ : List Rgn} {μ : List Tm} {m : Tm}
    {i : Option (Nat × Nat)} {e : Tm} {l : Lbl} {Λ' : List Rgn} {μ' : List Tm} {e' : Tm} {X : List Ty}
    {home : Nat → Option (Nat × Nat)} (hroot : RootTyM S .live root)
    (hn : LNest Ψ ⟨Λ, μ, m⟩ anc home) (hi : (⟨Λ, μ, m⟩ : Cfg).at i = some e)
    (hμ : StoreTyM S .live Ψ μ) (hμ' : StoreTyM S .live (Ψ ++ X) μ') {hs₀ : Nat → Nat → Prop}
    {cell : Nat → Nat} {H : Nat → Prop} (hca : e.Caught hs₀ cell H)
    (hown : ∀ E c p t, OkE E → e = plug E (.app (.app (.const c) (.own p)) t) →
      (c = .addFin ∨ c = .spawn) → holeE (baseOf anc i) E p)
    (hcell : ∀ E Ef al rf e₀, OkE E → e = plug E (.hdl Ef al rf e₀) →
      e' = plug E (.inside Λ.length (.hwith Λ.length μ.length Ef al rf e₀)) →
      ∃ r ε t, (Ψ ++ X)[μ.length]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) ∧
        ∀ x, r (.atm (.r x)) → holeE (baseOf anc i) E x ∨ x = Λ.length)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∃ anc' home', LNest (Ψ ++ X) ((⟨Λ', μ', m⟩ : Cfg).put i e') anc' home' ∧
      ∀ q, q ≠ Λ.length → anc' q = anc q := by
  have keep : ∀ {Ψ' c' home'}, LNest Ψ' c' anc home' →
      ∃ anc' home'', LNest Ψ' c' anc' home'' ∧ ∀ q, q ≠ Λ.length → anc' q = anc q :=
    fun h => ⟨anc, _, h, fun _ _ => rfl⟩
  have nil : μ'.length = μ.length → X = [] := fun h => by
    have h₁ := hμ.1; have h₂ := hμ'.1
    simp only [List.length_append] at h₂
    exact List.eq_nil_of_length_eq_zero (by omega)
  have hW := (hn.thread i e hi).1
  have nf : ∀ {x : Tm}, x.NoIns → ∀ q, ¬ x.Has (.frame q) := fun h q => h.frame q
  have app : ∀ {X : List Ty} (l : Nat) (t : Ty), Ψ[l]? = some t → (Ψ ++ X)[l]? = some t :=
    fun _ _ h => getElem?_append_of h
  cases hs with
  | @pure E r₀ r₁ _ _ hE hh =>
    rw [nil rfl, List.append_nil]
    have hrW := (fw_of_plug hh.not_value hE hW).2
    by_cases hd : r₀.IsHdl
    · cases hh with
      | hwithVal hv => exact keep (hn.hwithVal hi hE hv)
      | hctlTail hEi hk hv => subst hk; exact keep (hn.hctlTail hi hE hEi hv)
      | hctlDisc hEi hk hv => subst hk; exact keep (hn.hctlDisc hi hE hEi hv)
      | hkeepVal hv => exact keep (hn.hkeepVal hi hE hv)
      | hkarmVal hEi hk hv => subst hk; exact keep (hn.hkarmVal hi hE hEi hv)
      | hresVal hA ha hv => subst ha; exact keep (hn.hresVal hi hE hA hv)
      | hdoneVal hv => exact keep (hn.hdoneVal hi hE hv)
      | _ => simp [Tm.IsHdl] at hd
    exact keep (hn.sub hi hE hh.not_value (hh.noInsF hrW hd) fun _ h => hh.has h)
  | performRoot hE _ hr₀ hf hv =>
    rw [nil rfl, List.append_nil]
    have hrW := (fw_of_plug perform_not_value hE hW).2
    obtain ⟨hwv, hwt⟩ := hroot.svc _ _ hr₀
    have hfn : Tm.NoIns _ := fun h => hwt.noIns hwv (field_has hf h)
    have hvn := hv.noInsF hrW.2.1
    exact keep (hn.redex hi hE perform_not_value (nf fun h => h.elim (fun h => h.elim) hvn)
      (nf fun h => h.elim hfn hvn) (fun _ _ h => h)
      (fun _ => Tm.Fw.of_noIns fun h => h.elim hfn hvn) hn.cells)
  | glob hE hg =>
    rw [nil rfl, List.append_nil]
    have hno := (hroot.glob _ _ hg).1
    exact keep (hn.redex hi hE (fun h => by cases h) (fun q h => by simp [Tm.Has] at h)
      (nf (hno _)) (fun _ _ h => h) (fun _ => Tm.Fw.of_noIns (hno _)) hn.cells)
  | raise hE hF hnt hni hnc hv =>
    rw [nil rfl, List.append_nil]
    have hrW := (fw_of_plug (not_value_plug raise_not_value _) hE hW).2
    obtain ⟨hFW, hrv⟩ := Frame.fw_of_plug hF raise_not_value hrW
    rw [Frame.hole_eq hni] at hrv
    exact keep (hn.redex hi hE (not_value_plug raise_not_value _)
      (hFW.noFrame_plug hni hnc (nf (hv.noInsF hrv))) (nf (hv.noInsF hrv)) (fun _ _ h => h)
      (fun _ => hrv) hn.cells)
  | @addFin E _ p₀ _ t₀ _ hE hg hl hv =>
    rw [nil rfl, List.append_nil]
    have hnv := fin_not_value (w₁ := .own p₀) (w₂ := t₀) (.inl rfl)
    have hrW := (fw_of_plug hnv hE hW).2
    have htn := hv.noInsF hrW.2.1
    have hp := hown E _ p₀ t₀ hE rfl (.inl rfl)
    refine keep (hn.grow hi hE hnv (fun h => htn (h.elim (fun h => h.elim
      (fun h => h.elim) (fun h => by cases h)) id)) (fun _ h => by cases h) hg hl rfl
      ⟨[], by simp, by simp⟩ fun f hf => ?_)
    rcases List.mem_append.mp hf with hf | hf
    · exact hn.fins _ _ hg hl f hf
    · simp at hf; subst hf
      exact hn.withs hi hE hnv hp (x := .await t₀) htn
  | @spawn E _ p₀ _ t₀ _ hE hg hl hv =>
    rw [nil rfl, List.append_nil]
    have hnv := fin_not_value (w₁ := .own p₀) (w₂ := t₀) (.inr rfl)
    have hrW := (fw_of_plug hnv hE hW).2
    have htn := hv.noInsF hrW.2.1
    have hp := hown E _ p₀ t₀ hE rfl (.inr rfl)
    exact keep (hn.grow hi hE hnv (fun h => htn (h.elim (fun h => h.elim
      (fun h => h.elim) (fun h => by cases h)) id)) (fun _ h => by cases h) hg hl rfl
      ⟨[_], rfl, fun k hk => by
        simp at hk; subst hk
        exact hn.withs hi hE hnv hp (x := .await t₀) htn⟩
      fun f hf => hn.fins _ _ hg hl f hf)
  | @rt E _ _ _ _ _ _ hE hh =>
    have hnv := hh.not_value
    obtain ⟨hWE, hrW⟩ := fw_of_plug hnv hE hW
    cases hh with
    | @refMake v _ _ hv =>
      have hvn := hv.noInsF hrW.2.1
      refine keep (hn.redex hi hE hnv (nf fun h => hvn (h.elim (fun h => h.elim) id))
        (fun q h => by simp [Tm.Has] at h) app (fun _ => trivial) fun l' w hw => ?_)
      by_cases hl' : l' < μ.length
      · rw [List.getElem?_append_left hl'] at hw
        exact hn.cells l' w hw
      · rw [List.getElem?_append_right (by omega)] at hw
        cases hj : l' - μ.length with
        | zero => rw [hj] at hw; simp at hw; subst hw; exact hvn
        | succ => rw [hj] at hw; simp at hw
    | @refGet _ l₀ _ _ hl' =>
      rw [nil rfl, List.append_nil]
      have hcn := hn.cells _ _ hl'
      have hr0 : Tm.NoIns (.app (.const .refGet) (.loc l₀)) := by simp [Tm.NoIns, Tm.Has]
      exact keep (hn.redex hi hE hnv (nf hr0) (nf hcn) (fun _ _ h => h)
        (fun _ => Tm.Fw.of_noIns hcn) hn.cells)
    | @refSet v l₀ _ _ hv hlt =>
      have hX := nil (by simp)
      subst hX
      rw [List.append_nil]
      have hvn := hv.noInsF hrW.2.1
      have hr0 : Tm.NoIns (.app (.app (.const .refSet) (.loc l₀)) v) :=
        fun h => hvn (h.elim (fun h => h.elim (fun h => h.elim) (fun h => by cases h)) id)
      refine keep (hn.redex hi hE hnv (nf hr0) (fun q h => by simp [Tm.Has] at h) (fun _ _ h => h)
        (fun _ => trivial) fun l' w hw => ?_)
      by_cases hl' : l₀ = l'
      · subst hl'; rw [List.getElem?_set_self hlt] at hw; cases hw; exact hvn
      · rw [List.getElem?_set_ne hl'] at hw; exact hn.cells l' w hw
    | await =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv hrW.2 fun _ h => h)
    | scopeRun hv =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim (fun h => by cases h) (hv.noInsF hrW.2.1))
        fun _ h => Tm.has_scopeRun h)
    | tryRaise hv =>
      rw [nil rfl, List.append_nil]
      exact keep (hn.sub hi hE hnv (fun h => h.elim (hv.noInsF hrW.1) hrW.2.2)
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
      exact hn.scope hi hE
    | open_ hg hl =>
      rw [nil rfl, List.append_nil]
      refine keep (hn.grow hi hE hnv (by simp [Tm.NoIns, Tm.Has]) (fun _ h => .inr h) hg hl rfl
        ⟨[], by simp, by simp⟩ fun f hf => ?_)
      rcases List.mem_append.mp hf with hf | hf
      · exact hn.fins _ _ hg hl f hf
      · simp at hf; subst hf
        exact ⟨Tm.Fw.of_noIns (by simp [Tm.NoIns, Tm.Has]), fun q h => by simp [Tm.Has] at h⟩
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
      exact keep (hn.close hi hE hw hg hk fun q ℓ a he' hq => by
        obtain ⟨-, hπ', hwc⟩ := caught_plug.mp hca
        rw [he'] at hwc
        exact hπ' (hq ▸ hwc.1))
    | hctlRaise hEi hk hw =>
      rw [nil rfl, List.append_nil]; subst hk
      exact keep (hn.hctlSig hi hE hEi (fun q h => (hw.noInsF hrW.2.2.2.1).frame q h) hnv)
    | hctlUnw hEi hk =>
      rw [nil rfl, List.append_nil]; subst hk
      exact keep (hn.hctlSig hi hE hEi (fun q h => by
        simp only [Tm.Has] at h
        rcases h with h | h | h
        · cases h
        · cases h
        · exact hrW.2.2.2.1.2.2.1 q h) hnv)
    | hresRaise hA ha hw =>
      rw [nil rfl, List.append_nil]; subst ha
      have hwn := hw.noInsF hrW.2.2.1
      exact keep (hn.hresSig hi hE hA (fun q h => hwn.frame q h) hnv
        fun _ _ => Tm.Fw.of_noIns (e := .raise _) hwn)
    | hresUnw hA ha =>
      rw [nil rfl, List.append_nil]; subst ha
      exact keep (hn.hresSig hi hE hA (fun q h => by
        simp only [Tm.Has] at h
        rcases h with h | h | h
        · cases h
        · cases h
        · exact hrW.2.2.1.2.2.1 q h) hnv fun _ hx => unw_inwardF hx)
    | hwithCatch => rw [nil rfl, List.append_nil]; exact keep (hn.hwithCatch hi hE)
    | hkarmRaise hEi hk hw =>
      rw [nil rfl, List.append_nil]; subst hk
      have hwn := hw.noInsF hrW.2.2.1
      refine keep (hn.hkarmSig hi hE hEi hnv (fun q' h => ?_) hwn.frame fun _ hs hq =>
        ⟨holeE_iff.mpr (.inl hq), Tm.Fw.of_noIns (e := .raise _) hwn, fun q' h => hwn.frame q' h,
          fun x hx' => holeE_iff.mpr (.inl (hs x hx'))⟩)
      simp only [Tm.Has] at h
      rcases h with h | h | h
      · cases h
      · cases h
      · exact hwn.frame q' h
    | hkarmUnw hEi hk =>
      rw [nil rfl, List.append_nil]; subst hk
      have hf := unw_noFrameF hrW.2.2.1
      exact keep (hn.hkarmSig hi hE hEi hnv hf hf fun hx _ _ => unw_inwardF hx)
    | hdl => exact hn.hdl hi hE (hcell E _ _ _ _ hE rfl rfl)
  | unw hE hF hni hnc _ =>
    rw [nil rfl, List.append_nil]
    have hrW := (fw_of_plug (not_value_plug unw_not_value _) hE hW).2
    obtain ⟨hFW, hrv⟩ := Frame.fw_of_plug hF unw_not_value hrW
    rw [Frame.hole_eq hni] at hrv
    exact keep (hn.redex hi hE (not_value_plug unw_not_value _)
      (hFW.noFrame_plug hni hnc (unw_noFrameF hrv)) (unw_noFrameF hrv)
      (fun _ _ h => h) (fun _ => hrv) hn.cells)
  | performAbort hE hEi _ ha hv =>
    rw [nil rfl, List.append_nil]
    exact keep (hn.performAbort hi hE hEi ha hv)
  | performTail hE hEi _ ha hv =>
    rw [nil rfl, List.append_nil]
    exact keep (hn.performTail hi hE hEi ha hv)
  | performKept hE hEi _ ha hv _ =>
    rw [nil rfl, List.append_nil]
    exact keep (hn.performKept hi hE hEi ha hv)
  | performCtl hE hEi _ ha hv => exact keep (hn.performCtl hi hE hEi ha hv)
  | resume hE hA hEi hk _ hu =>
    rw [nil rfl, List.append_nil]; subst hk
    exact keep (hn.resume hi hE hA hEi hu)

end Flick
