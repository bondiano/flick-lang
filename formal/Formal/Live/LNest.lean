import Formal.Live.FormsHole
import Formal.NestMain

/-!
# `⊢ Λ` of run-time forms

`LNest` is the half of `Nest` about the run-time forms of a configuration, whatever
its values name: the ancestors of the regions, where the `inside` forms and the
handlers of each thread and finalizer are, and the thread `home π` whose `inside π`
form is that of `π`. The cells hold no run-time form.
-/

namespace Flick

structure LNest (Ψ : List Ty) (c : Cfg) (anc : Nat → Nat → Prop) (home : Nat → Option (Nat × Nat)) : Prop where
  self : ∀ q, anc q q
  le : ∀ q x, anc q x → x ≤ q
  chain : ∀ q, Live c.rgs q → Chain anc (anc q)
  live : ∀ q x, Live c.rgs q → anc q x → Live c.rgs x
  thread : ∀ i e, c.at i = some e → e.Fw Ψ anc (baseOf anc i) ∧ ∀ q, e.Has (.frame q) → home q = i ∧ Live c.rgs q
  home : ∀ q, Live c.rgs q → ∃ e, c.at (home q) = some e ∧ e.Has (.frame q)
  fins : ∀ (r : Nat) (g : Rgn), c.rgs[r]? = some g → g.live = true → ∀ f ∈ g.fins,
    f.Fw Ψ anc (anc r) ∧ ∀ q, ¬ f.Has (.frame q)
  cells : ∀ (l : Nat) (v : Tm), c.mem[l]? = some v → v.NoIns

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

/-- The program at the start. -/
theorem LNest.init {Ψ e₀} (hb : e₀.Below 0) :
    LNest Ψ ⟨[], [], e₀⟩ (fun q x => x = q) (fun _ => none) := by
  have hn : e₀.NoIns := hb.noIns
  have none : ∀ q, ¬ Live [] q := fun q ⟨_, hg, _⟩ => by simp at hg
  refine ⟨fun _ => rfl, fun _ _ h => h ▸ Nat.le_refl _, fun q h => (none q h).elim,
    fun q _ h => (none q h).elim, fun i e hi => ?_, fun q h => (none q h).elim,
    fun _ _ hg => by simp at hg, fun _ _ h => by simp at h⟩
  cases i with
  | none => cases hi; exact ⟨Tm.Fw.of_noIns hn, fun q hq => (hn.frame q hq).elim⟩
  | some rj => obtain ⟨g, hg, -⟩ := Cfg.at_some.mp hi; simp at hg

theorem LNest.base_chain {c home i e} (hn : LNest Ψ c anc home) (h : c.at i = some e) :
    Chain anc (baseOf anc i) := by
  cases i with
  | none => intro x hx; exact hx.elim
  | some rj => obtain ⟨r, j⟩ := rj; exact hn.chain r (Cfg.at_live h r j rfl)

/-- The regions of the hole of a thread are a chain. -/
theorem LNest.hole_chain {c home i E r} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    (hE : OkE E) : Chain anc (holeE (baseOf anc i) E) :=
  (fh_of_plug hE (hn.thread i _ hi).1).1.chain hn.le (hn.base_chain hi)

/-- The regions of the hole of a thread are live. -/
theorem LNest.hole_live {c home i E r} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    {x : Nat} (hx : holeE (baseOf anc i) E x) : Live c.rgs x := by
  rcases holeE_iff.mp hx with hx | hx
  · exact baseOf_live hn.live (Cfg.at_live hi) hx
  · exact ((hn.thread i _ hi).2 x (plug_frame hx)).2

/-- The region of a frame of a thread is a region of its hole. -/
theorem LNest.frame_reg {c home i E r} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    (hE : OkE E) {p : Nat} : ∀ F ∈ E, F.reg = some p → holeE (baseOf anc i) E p :=
  (fh_of_plug hE (hn.thread i _ hi).1).1.reg

/-- The handlers of a thread have their cells. -/
theorem LNest.frame_cell {c home i E r} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    (hE : OkE E) {q ℓ : Nat} : ∀ F ∈ E, F.cell = some (q, ℓ) → LCell Ψ anc q ℓ :=
  (fh_of_plug hE (hn.thread i _ hi).1).1.cell

/-- A finalizer of a live region `ϱ` in a context: the regions of its hole are a chain of
live regions, the ancestors of `ϱ`, the regions of its frames among them, and its handlers
have their cells. -/
theorem LNest.fin_hole {c home} {ρ : Nat} {g : Rgn} {f : Tm} {E : List Frame} {r : Tm}
    (hn : LNest Ψ c anc home) (hg : c.rgs[ρ]? = some g) (hl : g.live = true) (hf : f ∈ g.fins)
    (he : f = plug E r) (hE : OkE E) :
    holeE (anc ρ) E = anc ρ ∧ Chain anc (anc ρ) ∧ (∀ x, anc ρ x → Live c.rgs x) ∧
      (∀ F ∈ E, ∀ p, F.reg = some p → anc ρ p) ∧
      ∀ F ∈ E, ∀ q ℓ, F.cell = some (q, ℓ) → LCell Ψ anc q ℓ := by
  obtain ⟨hw, hnf⟩ := hn.fins ρ g hg hl f hf
  subst he
  have hWE := (fh_of_plug hE hw).1
  have hH : holeE (anc ρ) E = anc ρ := funext fun x => propext ⟨fun h =>
    (holeE_iff.mp h).resolve_right fun h' => hnf x (plug_frame h'), fun h => holeE_iff.mpr (.inl h)⟩
  refine ⟨hH, hn.chain ρ ⟨g, hg, hl⟩, fun x hx => hn.live ρ x ⟨g, hg, hl⟩ hx,
    fun F hF p hp => hH ▸ hWE.reg F hF hp, fun F hF q ℓ hc => hWE.cell F hF hc⟩

/-- A thread replaced, the regions and the ancestors kept: the cells may change. -/
theorem LNest.put_local {Ψ' : List Ty} {c : Cfg} {μ' : List Tm} {home i e e'}
    (hn : LNest Ψ c anc home) (hi : c.at i = some e)
    (hΨ : ∀ (l : Nat) (t : Ty), Ψ[l]? = some t → Ψ'[l]? = some t)
    (h' : e'.Fw Ψ' anc (baseOf anc i)) (hf : ∀ q, e'.Has (.frame q) ↔ e.Has (.frame q))
    (hc : ∀ (l : Nat) (v : Tm), μ'[l]? = some v → v.NoIns) :
    LNest Ψ' ((⟨c.rgs, μ', c.main⟩ : Cfg).put i e') anc home := by
  have hi₁ : (⟨c.rgs, μ', c.main⟩ : Cfg).at i = some e := by
    rcases i with _ | ⟨r, j⟩
    · exact hi
    · exact Cfg.at_some.mpr (Cfg.at_some.mp hi)
  have hat : ∀ i', (((⟨c.rgs, μ', c.main⟩ : Cfg).put i e').at i' = if i' = i then some e' else c.at i') := by
    intro i'; rw [Cfg.at_put hi₁]
    rcases i' with _ | ⟨r, j⟩
    · rfl
    · split
      · rfl
      · apply Option.ext; intro k; rw [Cfg.at_some, Cfg.at_some]
  have hlive : ∀ q, Live ((⟨c.rgs, μ', c.main⟩ : Cfg).put i e').rgs q ↔ Live c.rgs q :=
    fun q => Cfg.put_live
  refine ⟨hn.self, hn.le, fun q hq => hn.chain q ((hlive q).mp hq),
    fun q x hq hx => (hlive x).mpr (hn.live q x ((hlive q).mp hq) hx), fun i' k hk => ?_,
    fun q hq => ?_, fun r g hg hgl f hf' => ?_, by rw [Cfg.put_mem]; exact hc⟩
  · rw [hat] at hk
    split at hk
    · subst i'; cases hk
      refine ⟨h', fun q hq => ?_⟩
      obtain ⟨h₁, h₂⟩ := (hn.thread i e hi).2 q ((hf q).mp hq)
      exact ⟨h₁, (hlive q).mpr h₂⟩
    · obtain ⟨h₁, h₂⟩ := hn.thread i' k hk
      exact ⟨h₁.mono hΨ fun _ _ => rfl, fun q hq => let ⟨a, b⟩ := h₂ q hq; ⟨a, (hlive q).mpr b⟩⟩
  · obtain ⟨k, hk, hkq⟩ := hn.home q ((hlive q).mp hq)
    rw [hat]
    by_cases hh : home q = i
    · rw [if_pos hh]; rw [hh] at hk; rw [hi] at hk; cases hk
      exact ⟨e', rfl, (hf q).mpr hkq⟩
    · rw [if_neg hh]; exact ⟨k, hk, hkq⟩
  · obtain ⟨g₀, h₀, h₁, h₂⟩ := Cfg.put_get hg
    rw [h₂] at hf'
    obtain ⟨h₃, h₄⟩ := hn.fins r g₀ h₀ (h₁ ▸ hgl) f hf'
    exact ⟨h₃.mono hΨ fun _ _ => rfl, h₄⟩

/-- The redex of a thread replaced by a term without `inside` forms at its place. -/
theorem LNest.redex {Ψ' : List Ty} {c : Cfg} {μ' : List Tm} {home i E r r'}
    (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r)
    (hr : ∀ q, ¬ r.Has (.frame q)) (hr' : ∀ q, ¬ r'.Has (.frame q))
    (hΨ : ∀ (l : Nat) (t : Ty), Ψ[l]? = some t → Ψ'[l]? = some t)
    (h' : r.Fw Ψ anc (holeE (baseOf anc i) E) → r'.Fw Ψ' anc (holeE (baseOf anc i) E))
    (hc : ∀ (l : Nat) (v : Tm), μ'[l]? = some v → v.NoIns) :
    LNest Ψ' ((⟨c.rgs, μ', c.main⟩ : Cfg).put i (plug E r')) anc home := by
  have hw := (hn.thread i _ hi).1
  obtain ⟨-, hrW⟩ := fw_of_plug hv hE hw
  refine hn.put_local hi hΨ ((hw.mono hΨ fun _ _ => rfl).replug hv hE (h' hrW)
    fun x hx => (hr' x hx).elim) (fun q => ?_) hc
  rw [plug_frames_iff hr', plug_frames_iff hr]

/-- Replace the redex by a term that writes only what the redex writes. -/
theorem LNest.sub {c : Cfg} {home i E r r'} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    (hE : OkE E) (hv : ¬ Value r) (hr : r.NoIns) (hs : ∀ s, r'.Has s → r.Has s) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E r')) anc home :=
  hn.redex hi hE hv hr.frame (fun _ h => hr (Tm.has_frame_ins (hs _ h))) (fun _ _ h => h)
    (fun _ => Tm.Fw.of_noIns fun h => hr (hs _ h)) hn.cells

/-- A redex replaced by one with the same `inside` forms, at the place of its hole. -/
theorem LNest.redex' {c : Cfg} {home i E r r'} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    (hE : OkE E) (hv : ¬ Value r)
    (hfr : r.Fw Ψ anc (holeE (baseOf anc i) E) → ∀ q, r'.Has (.frame q) ↔ r.Has (.frame q))
    (h' : r.Fw Ψ anc (holeE (baseOf anc i) E) → r'.Fw Ψ anc (holeE (baseOf anc i) E)) :
    LNest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E r')) anc home := by
  have hw := (hn.thread i _ hi).1
  obtain ⟨-, hrW⟩ := fw_of_plug hv hE hw
  refine hn.put_local hi (fun _ _ h => h)
    (hw.replug hv hE (h' hrW) fun x hx => .inl ((hfr hrW x).mp hx)) (fun q => ?_) hn.cells
  rw [plug_has_iff, plug_has_iff (x := r), hfr hrW]

end Flick
