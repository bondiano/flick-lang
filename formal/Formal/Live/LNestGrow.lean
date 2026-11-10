import Formal.Live.LNest

/-!
# New children, finalizers and cells keep `⊢ Λ` of run-time forms
-/

namespace Flick

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem getElem?_append_of {Ψ X : List Ty} {l : Nat} {t : Ty} (h : Ψ[l]? = some t) :
    (Ψ ++ X)[l]? = some t := by
  rw [List.getElem?_append_left (getElem?_lt h)]; exact h

/-- New children and finalizers of a live region, at its place. -/
theorem LNest.regs {c : Cfg} {home p g g'} (hn : LNest Ψ c anc home) (hg : c.rgs[p]? = some g)
    (hl : g.live = true) (hl' : g'.live = true)
    (hk : ∃ ks, g'.kids = g.kids ++ ks ∧ ∀ k ∈ ks, k.Fw Ψ anc (anc p) ∧ ∀ q, ¬ k.Has (.frame q))
    (hf : ∀ f ∈ g'.fins, f.Fw Ψ anc (anc p) ∧ ∀ q, ¬ f.Has (.frame q)) :
    LNest Ψ ⟨setRgn c.rgs p g', c.mem, c.main⟩ anc home := by
  obtain ⟨ks, hks, hk⟩ := hk
  have get : ∀ {q g''}, (setRgn c.rgs p g')[q]? = some g'' ↔
      (q = p ∧ g'' = g') ∨ (q ≠ p ∧ c.rgs[q]? = some g'') := by
    intro q g''
    constructor
    · exact setRgn_get hg
    · rintro (⟨rfl, rfl⟩ | ⟨hq, h⟩)
      · simp [setRgn, List.getElem?_set_self (getElem?_lt hg)]
      · rw [setRgn, List.getElem?_set_ne (Ne.symm hq)]; exact h
  have hlive : ∀ q, Live (setRgn c.rgs p g') q ↔ Live c.rgs q := by
    intro q; constructor
    · rintro ⟨g'', h₁, h₂⟩
      rcases get.mp h₁ with ⟨rfl, rfl⟩ | ⟨_, h₁⟩
      · exact ⟨g, hg, hl⟩
      · exact ⟨g'', h₁, h₂⟩
    · rintro ⟨g'', h₁, h₂⟩
      by_cases hq : q = p
      · subst hq; exact ⟨g', get.mpr (.inl ⟨rfl, rfl⟩), hl'⟩
      · exact ⟨g'', get.mpr (.inr ⟨hq, h₁⟩), h₂⟩
  have old : ∀ i e, c.at i = some e → (⟨setRgn c.rgs p g', c.mem, c.main⟩ : Cfg).at i = some e := by
    rintro (_ | ⟨r, j⟩) e h
    · exact h
    · obtain ⟨g'', h₁, h₂, h₃⟩ := Cfg.at_some.mp h
      apply Cfg.at_some.mpr
      by_cases hr : r = p
      · subst hr; rw [hg] at h₁; cases h₁
        exact ⟨g', get.mpr (.inl ⟨rfl, rfl⟩), hl', by
          rw [hks, List.getElem?_append_left (getElem?_lt h₃)]; exact h₃⟩
      · exact ⟨g'', get.mpr (.inr ⟨hr, h₁⟩), h₂, h₃⟩
  refine ⟨hn.self, hn.le, fun q hq => hn.chain q ((hlive q).mp hq),
    fun q x hq hx => (hlive x).mpr (hn.live q x ((hlive q).mp hq) hx), fun i e hi => ?_,
    fun q hq => ?_, fun r g'' hr hgl f hf' => ?_, hn.cells⟩
  · rcases i with _ | ⟨r, j⟩
    · obtain ⟨h₁, h₂⟩ := hn.thread none e hi
      exact ⟨h₁, fun q hq => let ⟨a, b⟩ := h₂ q hq; ⟨a, (hlive q).mpr b⟩⟩
    · obtain ⟨g'', h₁, h₂, h₃⟩ := Cfg.at_some.mp hi
      rcases get.mp h₁ with ⟨rfl, rfl⟩ | ⟨hr, h₁⟩
      · rw [hks] at h₃
        by_cases hj : j < g.kids.length
        · rw [List.getElem?_append_left hj] at h₃
          obtain ⟨h₄, h₅⟩ := hn.thread (some (r, j)) e (Cfg.at_some.mpr ⟨g, hg, hl, h₃⟩)
          exact ⟨h₄, fun q hq => let ⟨a, b⟩ := h₅ q hq; ⟨a, (hlive q).mpr b⟩⟩
        · rw [List.getElem?_append_right (by omega)] at h₃
          obtain ⟨h₄, h₅⟩ := hk e (List.mem_of_getElem? h₃)
          exact ⟨h₄, fun q hq => (h₅ q hq).elim⟩
      · obtain ⟨h₄, h₅⟩ := hn.thread (some (r, j)) e (Cfg.at_some.mpr ⟨g'', h₁, h₂, h₃⟩)
        exact ⟨h₄, fun q hq => let ⟨a, b⟩ := h₅ q hq; ⟨a, (hlive q).mpr b⟩⟩
  · obtain ⟨e, he, hq'⟩ := hn.home q ((hlive q).mp hq)
    exact ⟨e, old _ _ he, hq'⟩
  · rcases get.mp hr with ⟨rfl, rfl⟩ | ⟨hr', hr⟩
    · exact hf f hf'
    · exact hn.fins r g'' hr hgl f hf'

/-- New children and finalizers of a live region, and the redex replaced. -/
theorem LNest.grow {c : Cfg} {home i E r r' p g g'} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r) (hr : r.NoIns)
    (hs : ∀ s, r'.Has s → r.Has s) (hg : c.rgs[p]? = some g) (hl : g.live = true) (hl' : g'.live = g.live)
    (hk : ∃ ks, g'.kids = g.kids ++ ks ∧ ∀ k ∈ ks, k.Fw Ψ anc (anc p) ∧ ∀ q, ¬ k.Has (.frame q))
    (hf : ∀ f ∈ g'.fins, f.Fw Ψ anc (anc p) ∧ ∀ q, ¬ f.Has (.frame q)) :
    LNest Ψ ((⟨setRgn c.rgs p g', c.mem, c.main⟩ : Cfg).put i (plug E r')) anc home :=
  (hn.regs hg hl (hl'.trans hl) hk hf).sub
    (Cfg.at_grow hg hl' (let ⟨ks, h, _⟩ := hk; ⟨ks, h⟩) hi) hE hv hr hs

/-- New children and finalizers of a live region, the thread kept. -/
theorem LNest.grow0 {c : Cfg} {home i e p g g'} (hn : LNest Ψ c anc home) (hi : c.at i = some e)
    (hg : c.rgs[p]? = some g) (hl : g.live = true) (hl' : g'.live = g.live)
    (hk : ∃ ks, g'.kids = g.kids ++ ks ∧ ∀ k ∈ ks, k.Fw Ψ anc (anc p) ∧ ∀ q, ¬ k.Has (.frame q))
    (hf : ∀ f ∈ g'.fins, f.Fw Ψ anc (anc p) ∧ ∀ q, ¬ f.Has (.frame q)) :
    LNest Ψ ((⟨setRgn c.rgs p g', c.mem, c.main⟩ : Cfg).put i e) anc home :=
  have hn₁ := hn.regs hg hl (hl'.trans hl) hk hf
  have hi₁ := Cfg.at_grow hg hl' (let ⟨ks, h, _⟩ := hk; ⟨ks, h⟩) hi
  hn₁.put_local hi₁ (fun _ _ h => h) (hn₁.thread i e hi₁).1 (fun _ => Iff.rfl) hn₁.cells

/-- A larger store whose cells hold no run-time form. -/
theorem LNest.store {X : List Ty} {c : Cfg} {μ' : List Tm} {home} (hn : LNest Ψ c anc home)
    (hc : ∀ (l : Nat) (v : Tm), μ'[l]? = some v → v.NoIns) :
    LNest (Ψ ++ X) ⟨c.rgs, μ', c.main⟩ anc home :=
  hn.put_local (i := none) (e := c.main) (e' := c.main) rfl (fun _ _ h => getElem?_append_of h)
    (((hn.thread none c.main rfl).1).mono (fun _ _ h => getElem?_append_of h) fun _ _ => rfl)
    (fun _ => Iff.rfl) hc

/-- A child or a finalizer in the handlers of its context older than its owner, at the place of
the owner: every region of the context as old as the owner is an ancestor of it. -/
theorem withs_fw {base A : Nat → Prop} {p : Nat} {x : Tm} (hle : ∀ q y, anc q y → y ≤ q) :
    ∀ {E : List Frame}, FwE Ψ anc base E → (∀ y, holeE base E y → y ≤ p → A y) →
      x.Fw Ψ anc A → (∀ q, ¬ x.Has (.frame q)) →
      (withs p E x).Fw Ψ anc A ∧ ∀ q, ¬ (withs p E x).Has (.frame q)
  | [], _, _, hx, hf => ⟨hx, hf⟩
  | F :: E, ⟨hE, hF⟩, hup, hx, hf => by
    have up : ∀ y, holeE base E y → holeE base (F :: E) y := fun y hy => holeE_cons hy
    simp only [withs]
    refine withs_fw hle hE (fun y hy => hup y (up y hy)) ?_ ?_
    · cases F with
      | hwith q ℓ E' al rf =>
        simp only [Frame.wrap]
        split
        · rename_i hqp
          obtain ⟨hq, hal, -, hn, -, hA', hC⟩ := hF
          exact ⟨hup q (up q hq) hqp, hal.congrA hA', hx, hn,
            fun y hy => hup y (up y ((hA' y).mpr hy)) (Nat.le_trans (hle q y hy) hqp), hC⟩
        · exact hx
      | hkeep q ℓ E' al =>
        simp only [Frame.wrap]
        split
        · rename_i hqp
          obtain ⟨hq, hal, hn, hs, hC⟩ := hF
          exact ⟨hup q (up q hq) hqp, hal, hx, hn,
            fun y hy => hup y (up y (hs y hy)) (Nat.le_trans (hle q y hy) hqp), hC⟩
        · exact hx
      | _ => exact hx
    · intro q'
      cases F with
      | hwith q ℓ E' al rf =>
        simp only [Frame.wrap]
        split
        · rintro (h | h | h | h)
          · cases h
          · cases h
          · exact absurd (Tm.has_frame_ins h) hF.2.2.2.1
          · exact hf q' h
        · exact hf q'
      | hkeep q ℓ E' al =>
        simp only [Frame.wrap]
        split
        · rintro (h | h | h | h)
          · cases h
          · cases h
          · exact absurd (Tm.has_frame_ins h) hF.2.2.1
          · exact hf q' h
        · exact hf q'
      | _ => exact hf q'

/-- Work registered with an owner of the place of the hole, in the handlers of the context older
than the owner, at the place of the owner. -/
theorem LNest.withs {c : Cfg} {home i E r p} {x : Tm} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r) (hp : holeE (baseOf anc i) E p)
    (hx : x.NoIns) :
    (withs p E x).Fw Ψ anc (anc p) ∧ ∀ q, ¬ (withs p E x).Has (.frame q) := by
  have hc := hn.hole_chain hi hE p hp
  exact withs_fw hn.le (fw_of_plug hv hE (hn.thread i _ hi).1).1
    (fun y hy hyp => by rw [hc]; exact ⟨hy, hyp⟩) (Tm.Fw.of_noIns hx) hx.frame

end Flick
