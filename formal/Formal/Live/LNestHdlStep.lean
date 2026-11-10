import Formal.Live.LNestHdl

/-!
# The steps of handlers in a context keep `⊢ Λ` of run-time forms

An operation runs its arm at the handler; the arm resumes the body under the handler,
and waits beside it; a new handler is a new region around its body, with a new cell
whose arms' row names the regions of the new handler's place.
-/

namespace Flick

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

/-- An arm instantiated with values holds no run-time form. -/
theorem arm_noIns {al b : Tm} {o : Name} {δ : List Tm} (hn : al.NoIns) (ha : al.armOf o = some b)
    (hδ : ∀ w ∈ δ, w.NoIns) : (b.subst (inst δ)).NoIns := fun hi => by
  rcases subst_inst_has hi with h' | ⟨w, hw, h'⟩
  · exact hn (Tm.armOf_has ha h')
  · exact hδ w hw h'

theorem perform_framesF {Ef o : Name} {c : OpClass} {v : Tm} {A : Nat → Prop} (hv : Value v)
    (h : (Tm.app (.const (.perform Ef o c)) v).Fw Ψ anc A) (q : Nat) :
    ¬ (Tm.app (.const (.perform Ef o c)) v).Has (.frame q) := by
  simp only [Tm.Has, not_or]
  exact ⟨by simp, (hv.noInsF h.2.1).frame q⟩

/-- The argument of an operation at the hole of a context holds no run-time form. -/
theorem perform_noIns {Ef o : Name} {c : OpClass} {v : Tm} {A : Nat → Prop} {Ei : List Frame}
    (hEi : OkE Ei) (hv : Value v) (h : (plug Ei (.app (.const (.perform Ef o c)) v)).Fw Ψ anc A) :
    v.NoIns :=
  hv.noInsF (fw_of_plug perform_not_value hEi h).2.2.1

theorem LNest.performAbort {c : Cfg} {home i E p ℓ Ef al rf o b v} {Ei : List Frame}
    (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .abort)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ (b.subst (inst [v]))))))) anc home := by
  refine hn.redex' hi hE (fun h => by cases h) (fun hW q => ?_) fun hW => ?_
  · have hn₁ := arm_noIns (δ := [v]) hW.2.2.2.2.1 ha (by simpa using perform_noIns hEi hv hW.2.2.2.1)
    have h₅ := plug_frame_iff (E := Ei) (x := .unw p ℓ (b.subst (inst [v])))
      (y := .app (.const (.perform Ef o .abort)) v) (fun q => ⟨fun h => by
        simp only [Tm.Has] at h
        rcases h with h | h | h
        · cases h
        · cases h
        · exact (hn₁.frame _ h).elim, fun h =>
          (perform_framesF hv (fw_of_plug perform_not_value hEi hW.2.2.2.1).2 _ h).elim⟩) q
    simp only [Tm.Has] at ⊢; grind
  · have hn₁ := arm_noIns (δ := [v]) hW.2.2.2.2.1 ha (by simpa using perform_noIns hEi hv hW.2.2.2.1)
    exact ⟨hW.1, hW.2.1, hW.2.2.1, hW.2.2.2.1.replug perform_not_value hEi
      (unw_fw hW.1 hW.2.2.2.2.2.2.1 (Tm.Fw.of_noIns hn₁) hn₁.frame) (fun x hx => by
        simp only [Tm.Has] at hx
        rcases hx with h | h | h
        · cases h
        · cases h
        · exact (hn₁.frame _ h).elim),
      hW.2.2.2.2.1, hW.2.2.2.2.2.1, hW.2.2.2.2.2.2⟩

/-- An arm that runs while the body waits at the hole of its operation. -/
theorem park_fw {p ℓ κ : Nat} {tl : Bool} {Ef : Name} {al rf a v f : Tm} {A : Nat → Prop}
    {Ei : List Frame} (hEi : OkE Ei) (hfv : ¬ Value (.app f v))
    (hW : (Tm.hwith p ℓ Ef al rf (plug Ei (.app f v))).Fw Ψ anc A) (han : a.NoIns) :
    (Tm.hctl p ℓ κ tl Ef al rf a (plug Ei (.hole p))).Fw Ψ anc A :=
  ⟨hW.1, hW.2.1, hW.2.2.1, Tm.Fw.of_noIns han, hW.2.2.2.1.replug hfv hEi (holeE_iff.mpr (.inl hW.1))
    (fun _ hx => (hole_frames hx).elim), hW.2.2.2.2.1, hW.2.2.2.2.2.1,
    fun x hx => (han.frame x hx).elim, hW.2.2.2.2.2.2⟩

theorem LNest.performTail {c : Cfg} {home i E p ℓ Ef al rf o b v} {Ei : List Frame}
    (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .tail)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hctl p ℓ 0 true Ef al rf (b.subst (inst [v])) (plug Ei (.hole p))))) anc home := by
  refine hn.redex' hi hE (fun h => by cases h) (fun hW q => ?_) fun hW => ?_
  · have hn₁ := arm_noIns (δ := [v]) hW.2.2.2.2.1 ha (by simpa using perform_noIns hEi hv hW.2.2.2.1)
    exact park_frames hn₁ (perform_framesF hv (fw_of_plug perform_not_value hEi hW.2.2.2.1).2) q
  · have hn₁ := arm_noIns (δ := [v]) hW.2.2.2.2.1 ha (by simpa using perform_noIns hEi hv hW.2.2.2.1)
    exact park_fw hEi perform_not_value hW hn₁

theorem LNest.performCtl {c : Cfg} {home i E p ℓ Ef al rf o b v} {Ei : List Frame} {X : List Ty}
    (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .control)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v) :
    LNest (Ψ ++ X) ((⟨c.rgs, c.mem ++ [.lam .todo], c.main⟩ : Cfg).put i
      (plug E (.hctl p ℓ c.mem.length false Ef al rf (b.subst (inst [v, .kont p c.mem.length]))
        (plug Ei (.hole p))))) anc home := by
  have hn' := hn.store (X := X) (μ' := c.mem ++ [.lam .todo]) fun l w hw => by
    by_cases hl : l < c.mem.length
    · rw [List.getElem?_append_left hl] at hw; exact hn.cells l w hw
    · rw [List.getElem?_append_right (by omega)] at hw
      cases hj : l - c.mem.length with
      | zero => simp [hj] at hw; subst hw; exact lam_todo_sites.1
      | succ => simp [hj] at hw
  have hδ : ∀ {A : Nat → Prop}, (plug Ei (.app (.const (.perform Ef o .control)) v)).Fw (Ψ ++ X) anc A →
      ∀ w ∈ [v, Tm.kont p c.mem.length], w.NoIns := fun h w hw => by
    simp at hw
    rcases hw with rfl | rfl
    · exact perform_noIns hEi hv h
    · simp [Tm.NoIns, Tm.Has]
  refine hn'.redex' (by rw [Cfg.at_mem]; exact hi) hE (fun h => by cases h) (fun hW q => ?_) fun hW => ?_
  · exact park_frames (arm_noIns hW.2.2.2.2.1 ha (hδ hW.2.2.2.1))
      (perform_framesF hv (fw_of_plug perform_not_value hEi hW.2.2.2.1).2) q
  · exact park_fw hEi perform_not_value hW (arm_noIns hW.2.2.2.2.1 ha (hδ hW.2.2.2.1))

theorem LNest.resume {c : Cfg} {home i E p ℓh κ Ef al rf u} {A Ei : List Frame}
    (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hctl p ℓh κ false Ef al rf (plug A (.app (.kont p κ) u))
      (plug Ei (.hole p))))) (hE : OkE E) (hA : OkE A) (hEi : OkE Ei) (hu : Value u) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hres p ℓh (plug A (.hole p)) (.hwith p ℓh Ef al rf (plug Ei u))))) anc home := by
  have kf : ∀ {A₀ : Nat → Prop}, (Tm.app (.kont p κ) u).Fw Ψ anc A₀ →
      ∀ q, ¬ (Tm.app (.kont p κ) u).Has (.frame q) := fun h q => by
    simp only [Tm.Has, not_or]; exact ⟨by simp, (hu.noInsF h.2.1).frame q⟩
  refine hn.redex' hi hE (fun h => by cases h) (fun hW q => ?_) fun hW => ?_
  · have hk := (fw_of_plug kont_not_value hA hW.2.2.2.1).2
    have h₁ := plug_frame_iff (E := A) (x := .hole p) (y := .app (.kont p κ) u)
      (fun q => ⟨fun h => (hole_frames h).elim, fun h => (kf hk q h).elim⟩) q
    have h₂ := plug_frame_iff (E := Ei) (x := u) (y := .hole p)
      (fun q => ⟨fun h => ((hu.noInsF hk.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q
    have h₃ := hW.2.2.2.2.2.1.frame q
    have h₄ := hW.2.2.2.2.2.2.1.frame q
    simp only [Tm.Has] at h₃ h₄ ⊢; grind
  · have hk := (fw_of_plug kont_not_value hA hW.2.2.2.1).2
    have hb : ∀ x, (Tm.hwith p ℓh Ef al rf (plug Ei u)).Has (.frame x) → (plug Ei u).Has (.frame x) :=
      fun x hx => by
        simp only [Tm.Has] at hx
        rcases hx with h | h | h | h | h
        · cases h
        · cases h
        · exact (hW.2.2.2.2.2.1.frame _ h).elim
        · exact (hW.2.2.2.2.2.2.1.frame _ h).elim
        · exact h
    have hfa : ∀ x, (plug A (.hole p)).Has (.frame x) ↔ (plug A (.app (.kont p κ) u)).Has (.frame x) :=
      plug_frame_iff fun q => ⟨fun h => (hole_frames h).elim, fun h => (kf hk q h).elim⟩
    have hfk : ∀ x, (plug Ei u).Has (.frame x) ↔ (plug Ei (.hole p)).Has (.frame x) :=
      plug_frame_iff fun q => ⟨fun h => ((hu.noInsF hk.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩
    refine ⟨hW.1, hW.2.2.2.1.replug kont_not_value hA (holeE_iff.mpr (.inl hW.1))
      (fun x hx => (hole_frames hx).elim), ⟨hW.1, hW.2.1, hW.2.2.1,
        hW.2.2.2.2.1.replug hole_not_value hEi (Tm.Fw.of_noIns (hu.noInsF hk.2.1))
          fun x hx => ((hu.noInsF hk.2.1).frame x hx).elim,
        hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩,
      fun x hx hx' => hW.2.2.2.2.2.2.2.1 x ((hfa x).mp hx') ((hfk x).mp (hb x hx)),
      hW.2.2.2.2.2.2.2.2.2⟩

/-- A new handler: a new region around its body, and a new cell whose arms' row names the
regions of the place of the handler and the new region. -/
theorem LNest.hdl {c : Cfg} {home i E Ef al rf e₀} {X : List Ty} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hdl Ef al rf e₀))) (hE : OkE E)
    (hcell : ∃ r ε t, (Ψ ++ X)[c.mem.length]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) ∧
      ∀ x, r (.atm (.r x)) → holeE (baseOf anc i) E x ∨ x = c.rgs.length) :
    ∃ anc' home', LNest (Ψ ++ X) ((⟨c.rgs ++ [Rgn.new], c.mem ++ [.lam .todo], c.main⟩ : Cfg).put i
      (plug E (.inside c.rgs.length (.hwith c.rgs.length c.mem.length Ef al rf e₀)))) anc' home' ∧
      ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  have hn' := hn.store (X := X) (μ' := c.mem ++ [.lam .todo]) fun l w hw => by
    by_cases hl : l < c.mem.length
    · rw [List.getElem?_append_left hl] at hw; exact hn.cells l w hw
    · rw [List.getElem?_append_right (by omega)] at hw
      cases hj : l - c.mem.length with
      | zero => simp [hj] at hw; subst hw; exact lam_todo_sites.1
      | succ => simp [hj] at hw
  have hi' : (⟨c.rgs, c.mem ++ [.lam .todo], c.main⟩ : Cfg).at i = some (plug E (.hdl Ef al rf e₀)) := by
    rw [Cfg.at_mem]; exact hi
  obtain ⟨-, hrW⟩ := fw_of_plug (by intro h; cases h) hE (hn'.thread i _ hi').1
  have hnr : (Tm.hdl Ef al rf e₀).NoIns := by
    simp only [Tm.NoIns, Tm.Has, not_or]; exact ⟨hrW.2.2.2.1, hrW.2.2.2.2.1, hrW.2.2.2.2.2⟩
  obtain ⟨r, ε, t, hℓ, hr⟩ := hcell
  refine hn'.fresh hi' hE (by intro h; cases h) hnr (fun q h => ?_) fun anc' ha _ => ?_
  · simp only [Tm.Has] at h
    rcases h with h | h | h | h | h
    · cases h
    · cases h
    · exact hrW.2.2.2.1.frame q h
    · exact hrW.2.2.2.2.1.frame q h
    · exact hrW.2.2.2.2.2.frame q h
  · exact ⟨.inr rfl, Tm.Fw.of_noIns hrW.2.2.2.1, Tm.Fw.of_noIns hrW.2.2.2.2.1,
      Tm.Fw.of_noIns hrW.2.2.2.2.2, hrW.2.2.2.1, hrW.2.2.2.2.1, fun x => by rw [ha],
      r, ε, t, hℓ, fun x hx => by rw [ha]; exact hr x hx⟩

end Flick
