import Formal.NestHdlRt

/-!
# The steps of handlers in a context keep `⊢ Λ`

An operation runs its arm at the handler, the argument of a type without regions; the
arm resumes the body under the handler, and waits beside it; a new handler is a new
region around its body, with a new cell.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

/-- A signal of a resumed body goes into the arm, past its `inside` forms. -/
theorem Nest.hresSig {c : Cfg} {home i E p ℓ x} {A : List Frame} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hres p ℓ (plug A (.hole p)) x))) (hE : OkE E) (hA : OkE A)
    (hx : ∀ q, ¬ x.Has (.frame q)) (hv : ¬ Value (.hres p ℓ (plug A (.hole p)) x))
    (hmv : ∀ {A₀ : Nat → Prop}, A₀ p → (plug A (.hole p)).Within Ψ B anc A₀ → x.Within Ψ B anc A₀ →
      x.Within Ψ B anc (holeE A₀ A)) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E (.hdone p (plug A x)))) anc B home :=
  hn.redex' hi hE hv (fun hW q => by
      have h₅ := plug_frame_iff (E := A) (x := x) (y := .hole p)
        (fun q => ⟨fun h => (hx q h).elim, fun h => (hole_frames h).elim⟩) q
      have h₄ := hx q
      simp only [Tm.Has] at ⊢; grind)
    fun hW => ⟨hW.1, hW.2.1.replug hole_not_value hA (hmv hW.1 hW.2.1 hW.2.2.1)
      fun x' hx' => (hx x' hx').elim⟩

/-- A signal holds no region the place of the hole does not. -/
theorem unw_inward {q ℓ : Nat} {b : Tm} {A₀ : Nat → Prop} {A : List Frame}
    (h : (Tm.unw q ℓ b).Within Ψ B anc A₀) : (Tm.unw q ℓ b).Within Ψ B anc (holeE A₀ A) :=
  ⟨holeE_iff.mpr (.inl h.1), h.2.1, h.2.2.1, fun x hx => holeE_iff.mpr (.inl (h.2.2.2 x hx))⟩

theorem unw_noFrame {p ℓ : Nat} {a : Tm} {A : Nat → Prop} (h : (Tm.unw p ℓ a).Within Ψ B anc A) :
    ∀ q, ¬ (Tm.unw p ℓ a).Has (.frame q) := fun q hq => by
  simp only [Tm.Has] at hq
  rcases hq with hq | hq | hq
  · cases hq
  · cases hq
  · exact h.2.2.1 q hq

theorem Cfg.at_mem {c : Cfg} {μ' : List Tm} (i : Option (Nat × Nat)) :
    (⟨c.rgs, μ', c.main⟩ : Cfg).at i = c.at i := by
  rcases i with _ | ⟨r, j⟩ <;> rfl

/-- The argument of an operation, at the place of the handler, and its arm there. -/
theorem arm_inst {al b v : Tm} {o : Name} {A : Nat → Prop} {Ei : List Frame} {Θ : Facts}
    {T : Ty} {φ : Stage} {f : Tm} (hal : al.Within Ψ B anc A) (hn : al.NoIns) (ha : al.armOf o = some b)
    (hEi : OkE Ei) (hb : (plug Ei (.app f v)).Within Ψ B anc A) (hfv : ¬ Value (.app f v))
    (hv : Value v) (ht : Typ S Ψ Θ [] v (.ex T φ)) (hT : T.YF 0) (δ : List Tm)
    (hδ : ∀ w ∈ δ, w = v ∨ w.NoIns ∧ w.Within Ψ B anc A) :
    (b.subst (inst δ)).NoIns ∧ (b.subst (inst δ)).Within Ψ B anc A := by
  have hvW : v.Within Ψ B anc A := ((within_of_plug hfv hEi hb).2.2.1).moveC hv ht hT
  have hδ' : ∀ w ∈ δ, w.NoIns ∧ w.Within Ψ B anc A := fun w hw => (hδ w hw).elim
    (fun h => h ▸ ⟨hv.noIns hvW, hvW⟩) id
  have hbn : b.NoIns := fun h => hn (Tm.armOf_has ha h)
  exact ⟨fun hi => by
    rcases subst_inst_has hi with h' | ⟨w, hw, h'⟩
    · exact hbn h'
    · exact (hδ' w hw).1 h', Tm.Within.ofInst hbn (hal.armOf ha) hδ'⟩

theorem Nest.performAbort {c : Cfg} {home i E p ℓ Ef al rf o b v} {Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .abort)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v)
    (ht : Typ S Ψ Θ [] v (.ex T φ)) (hT : T.YF 0) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ (b.subst (inst [v]))))))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      obtain ⟨hn₁, -⟩ := arm_inst hW.2.1 hW.2.2.2.2.1 ha hEi hW.2.2.2.1 perform_not_value hv ht hT [v]
        (by simp)
      have h₅ := plug_frame_iff (E := Ei) (x := .unw p ℓ (b.subst (inst [v])))
        (y := .app (.const (.perform Ef o .abort)) v) (fun q => ⟨fun h => by
          simp only [Tm.Has] at h
          rcases h with h | h | h
          · cases h
          · cases h
          · exact (hn₁.frame _ h).elim, fun h => by
          simp only [Tm.Has] at h
          rcases h with h | h
          · simp at h
          · exact ((hv.noIns ((within_of_plug perform_not_value hEi hW.2.2.2.1).2.2.1)).frame _ h).elim⟩) q
      simp only [Tm.Has] at ⊢; grind)
    fun hW => by
      obtain ⟨hn₁, hW₁⟩ := arm_inst hW.2.1 hW.2.2.2.2.1 ha hEi hW.2.2.2.1 perform_not_value hv ht hT [v]
        (by simp)
      exact ⟨hW.1, hW.2.1, hW.2.2.1, hW.2.2.2.1.replug perform_not_value hEi
        (unw_within hW.1 hW.2.2.2.2.2.2.1 hW₁ hn₁.frame) (fun x hx => by
          simp only [Tm.Has] at hx
          rcases hx with h | h | h
          · cases h
          · cases h
          · exact (hn₁.frame _ h).elim),
        hW.2.2.2.2.1, hW.2.2.2.2.2.1, hW.2.2.2.2.2.2⟩

/-- An arm that runs while the body waits at the hole of its operation. -/
theorem park_within {p ℓ κ : Nat} {tl : Bool} {Ef : Name} {al rf a v f : Tm} {A : Nat → Prop}
    {Ei : List Frame} (hEi : OkE Ei) (hfv : ¬ Value (.app f v))
    (hW : (Tm.hwith p ℓ Ef al rf (plug Ei (.app f v))).Within Ψ B anc A) (han : a.NoIns)
    (haW : a.Within Ψ B anc A) :
    (Tm.hctl p ℓ κ tl Ef al rf a (plug Ei (.hole p))).Within Ψ B anc A :=
  ⟨hW.1, hW.2.1, hW.2.2.1, haW, hW.2.2.2.1.replug hfv hEi (holeE_iff.mpr (.inl hW.1))
    (fun _ hx => (hole_frames hx).elim), hW.2.2.2.2.1, hW.2.2.2.2.2.1,
    fun x hx => (han.frame x hx).elim, hW.2.2.2.2.2.2⟩

theorem park_frames {p ℓ κ ℓ' : Nat} {tl : Bool} {Ef : Name} {al rf a v f : Tm} {Ei : List Frame}
    (han : a.NoIns) (hf : ∀ q, ¬ (Tm.app f v).Has (.frame q)) (q : Nat) :
    (Tm.hctl p ℓ κ tl Ef al rf a (plug Ei (.hole p))).Has (.frame q) ↔
      (Tm.hwith p ℓ' Ef al rf (plug Ei (.app f v))).Has (.frame q) := by
  have h₅ := plug_frame_iff (E := Ei) (x := .hole p) (y := .app f v)
    (fun q => ⟨fun h => (hole_frames h).elim, fun h => (hf q h).elim⟩) q
  have h₄ := han.frame q
  simp only [Tm.Has] at ⊢; grind

theorem perform_frames {Ef o : Name} {c : OpClass} {v : Tm} {A : Nat → Prop} (hv : Value v)
    (h : (Tm.app (.const (.perform Ef o c)) v).Within Ψ B anc A) (q : Nat) :
    ¬ (Tm.app (.const (.perform Ef o c)) v).Has (.frame q) := by
  simp only [Tm.Has, not_or]
  exact ⟨by simp, (hv.noIns h.2.1).frame q⟩

theorem Nest.performTail {c : Cfg} {home i E p ℓ Ef al rf o b v} {Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .tail)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v)
    (ht : Typ S Ψ Θ [] v (.ex T φ)) (hT : T.YF 0) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hctl p ℓ 0 true Ef al rf (b.subst (inst [v])) (plug Ei (.hole p))))) anc B home :=
  hn.redex' hi hE (fun h => by cases h) (fun hW q => by
      obtain ⟨hn₁, -⟩ := arm_inst hW.2.1 hW.2.2.2.2.1 ha hEi hW.2.2.2.1 perform_not_value hv ht hT [v]
        (by simp)
      exact park_frames hn₁ (perform_frames hv (within_of_plug perform_not_value hEi hW.2.2.2.1).2) q)
    fun hW => by
      obtain ⟨hn₁, hW₁⟩ := arm_inst hW.2.1 hW.2.2.2.2.1 ha hEi hW.2.2.2.1 perform_not_value hv ht hT [v]
        (by simp)
      exact park_within hEi perform_not_value hW hn₁ hW₁

theorem Nest.performCtl {c : Cfg} {home i E p ℓ Ef al rf o b v} {Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} {t₀ : Ty} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .control)) v)))))
    (hE : OkE E) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v) (hμ : c.mem.length = Ψ.length)
    (ht : Typ S Ψ Θ [] v (.ex T φ)) (hT : T.YF 0) :
    Nest (Ψ ++ [t₀]) ((⟨c.rgs, c.mem ++ [.lam .todo], c.main⟩ : Cfg).put i
      (plug E (.hctl p ℓ Ψ.length false Ef al rf (b.subst (inst [v, .kont p Ψ.length]))
        (plug Ei (.hole p))))) anc B home := by
  have hn' := hn.extend_store (t₀ := t₀) hμ lam_todo_sites.1 lam_todo_sites.2.1 lam_todo_sites.2.2
  have ht' := ht.weakenStore [t₀]
  have hd : ∀ (A : Nat → Prop), A p → ∀ w ∈ [v, Tm.kont p Ψ.length],
      w = v ∨ w.NoIns ∧ w.Within (Ψ ++ [t₀]) B anc A := fun A hp w hw => by
    simp at hw
    rcases hw with rfl | rfl
    · exact .inl rfl
    · exact .inr ⟨by simp [Tm.NoIns, Tm.Has], hp⟩
  exact hn'.redex' (by rw [Cfg.at_mem]; exact hi) hE (fun h => by cases h) (fun hW q => by
      obtain ⟨hn₁, -⟩ := arm_inst hW.2.1 hW.2.2.2.2.1 ha hEi hW.2.2.2.1 perform_not_value hv ht' hT _
        (hd _ hW.1)
      exact park_frames hn₁ (perform_frames hv (within_of_plug perform_not_value hEi hW.2.2.2.1).2) q)
    fun hW => by
      obtain ⟨hn₁, hW₁⟩ := arm_inst hW.2.1 hW.2.2.2.2.1 ha hEi hW.2.2.2.1 perform_not_value hv ht' hT _
        (hd _ hW.1)
      exact park_within hEi perform_not_value hW hn₁ hW₁

theorem Nest.resume {c : Cfg} {home i E p ℓh κ Ef al rf u} {A Ei : List Frame} {Θ : Facts} {T : Ty}
    {φ : Stage} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hctl p ℓh κ false Ef al rf (plug A (.app (.kont p κ) u))
      (plug Ei (.hole p))))) (hE : OkE E) (hA : OkE A) (hEi : OkE Ei) (hu : Value u)
    (ht : Typ S Ψ Θ [] u (.ex T φ)) (hT : T.YF 0) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i
      (plug E (.hres p ℓh (plug A (.hole p)) (.hwith p ℓh Ef al rf (plug Ei u))))) anc B home := by
  have kf : ∀ {A₀ : Nat → Prop}, (Tm.app (.kont p κ) u).Within Ψ B anc A₀ →
      ∀ q, ¬ (Tm.app (.kont p κ) u).Has (.frame q) := fun h q => by
    simp only [Tm.Has, not_or]; exact ⟨by simp, (hu.noIns h.2.1).frame q⟩
  refine hn.redex' hi hE (fun h => by cases h) (fun hW q => ?_) fun hW => ?_
  · have hk := (within_of_plug kont_not_value hA hW.2.2.2.1).2
    have h₁ := plug_frame_iff (E := A) (x := .hole p) (y := .app (.kont p κ) u)
      (fun q => ⟨fun h => (hole_frames h).elim, fun h => (kf hk q h).elim⟩) q
    have h₂ := plug_frame_iff (E := Ei) (x := u) (y := .hole p)
      (fun q => ⟨fun h => ((hu.noIns hk.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩) q
    have h₃ := hW.2.2.2.2.2.1.frame q
    have h₄ := hW.2.2.2.2.2.2.1.frame q
    simp only [Tm.Has] at h₃ h₄ ⊢; grind
  · have hk := (within_of_plug kont_not_value hA hW.2.2.2.1).2
    have huW := hk.2.1.moveC hu ht hT (A₂ := holeE (holeE (baseOf anc i) E) Ei)
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
      plug_frame_iff fun q => ⟨fun h => ((hu.noIns hk.2.1).frame q h).elim, fun h => (hole_frames h).elim⟩
    refine ⟨hW.1, hW.2.2.2.1.replug kont_not_value hA (holeE_iff.mpr (.inl hW.1))
      (fun x hx => (hole_frames hx).elim), ⟨hW.1, hW.2.1, hW.2.2.1,
        hW.2.2.2.2.1.replug hole_not_value hEi huW fun x hx => ((hu.noIns hk.2.1).frame x hx).elim,
        hW.2.2.2.2.2.1, hW.2.2.2.2.2.2.1, hW.2.2.2.2.2.2.2.2⟩,
      fun x hx hx' => hW.2.2.2.2.2.2.2.1 x ((hfa x).mp hx') ((hfk x).mp (hb x hx)),
      hW.2.2.2.2.2.2.2.2.2⟩

/-- A new handler: a new region around its body, and a new cell that holds its type. -/
theorem Nest.hdl {c : Cfg} {home i E Ef al rf e₀} {t₀ : Ty} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.hdl Ef al rf e₀))) (hE : OkE E) (hμ : c.mem.length = Ψ.length)
    (hΨ : ∀ u ∈ Ψ, u.YF c.rgs.length) (ht₀ : t₀.YF c.rgs.length) :
    ∃ anc' home', Nest (Ψ ++ [t₀]) ((⟨c.rgs ++ [Rgn.new], c.mem ++ [.lam .todo], c.main⟩ : Cfg).put i
      (plug E (.inside c.rgs.length (.hwith c.rgs.length Ψ.length Ef al rf e₀)))) anc' B home' ∧
      ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  have hn' := hn.extend_store (t₀ := t₀) hμ lam_todo_sites.1 lam_todo_sites.2.1 lam_todo_sites.2.2
  have hi' : (⟨c.rgs, c.mem ++ [.lam .todo], c.main⟩ : Cfg).at i = some (plug E (.hdl Ef al rf e₀)) := by
    rw [Cfg.at_mem]; exact hi
  obtain ⟨-, hrW⟩ := within_of_plug (by intro h; cases h) hE (hn'.thread i _ hi').1
  have hnr : (Tm.hdl Ef al rf e₀).NoIns := by
    simp only [Tm.NoIns, Tm.Has, not_or]; exact ⟨hrW.2.2.2.1, hrW.2.2.2.2.1, hrW.2.2.2.2.2⟩
  have hfr : ∀ (l : Nat) (t : Ty), (Ψ ++ [t₀])[l]? = some t → ¬ t.reach (.r c.rgs.length) := by
    intro l t ht h
    by_cases hl : l < Ψ.length
    · rw [List.getElem?_append_left hl] at ht
      exact Nat.lt_irrefl _ ((hΨ t (List.mem_of_getElem? ht)).reach h)
    · rw [List.getElem?_append_right (by omega)] at ht
      cases hj : l - Ψ.length with
      | zero => simp [hj] at ht; subst ht; exact Nat.lt_irrefl _ (ht₀.reach h)
      | succ => simp [hj] at ht
  refine hn'.fresh hi' hE (by intro h; cases h) hnr (fun q h => ?_) fun anc' ha hq => ?_
  · simp only [Tm.Has] at h
    rcases h with h | h | h | h | h
    · cases h
    · cases h
    · exact hrW.2.2.2.1.frame q h
    · exact hrW.2.2.2.2.1.frame q h
    · exact hrW.2.2.2.2.2.frame q h
  · refine ⟨.inr rfl, (hrW.1.extend hrW.2.2.2.1 hfr).anyAnc hrW.2.2.2.1,
      (hrW.2.1.extend hrW.2.2.2.2.1 hfr).anyAnc hrW.2.2.2.2.1,
      (hrW.2.2.1.extend hrW.2.2.2.2.2 hfr).anyAnc hrW.2.2.2.2.2, hrW.2.2.2.1, hrW.2.2.2.2.1,
      fun x => by rw [ha], ⟨t₀, by simp, ht₀⟩⟩

end Flick
