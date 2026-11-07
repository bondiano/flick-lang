import Formal.NestPut

/-!
# A step at the redex keeps `⊢ Λ`

A value moves from one place to another (from a cell to the redex, from the redex
to a cell, out of a closing region, to a new child) and stays at its place when
the two places agree on what its type shows (`Tm.Within.move`).
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Tm.Within.rgn0 {e : Tm} {A : Nat → Prop} {p : Nat} (h : e.Within Ψ B anc A) (hn : e.NoIns)
    (hp : e.Has (.rgn p)) : A p :=
  (h.rgn hp).resolve_right (hn.frame p)

theorem Tm.Within.loc0' {e : Tm} {A : Nat → Prop} {l : Nat} (h : e.Within Ψ B anc A) (hn : e.NoIns)
    (hl : e.Has (.loc l)) : LocOk Ψ B A l := h.loc0 hn hl

/-- A term without `inside` forms moves to a place that agrees with its own on `C`,
when `C` bounds what it writes and the types of its locations. -/
theorem Tm.Within.move {v : Tm} {A₁ A₂ C : Nat → Prop} (hn : v.NoIns) (h : v.Within Ψ B anc A₁)
    (hr : ∀ p, v.Has (.rgn p) → C p)
    (hl : ∀ l t, v.Has (.loc l) → Ψ[l]? = some t → ∀ x, t.reach (.r x) → C x)
    (hA : ∀ x, C x → (A₁ x ↔ A₂ x)) : v.Within Ψ B anc A₂ := by
  refine Tm.Within.of_has hn (fun p hp => (hA p (hr p hp)).mp (h.rgn0 hn hp)) fun l hl' => ?_
  obtain ⟨t, ht, hB⟩ := h.loc0 hn hl'
  refine ⟨t, ht, hB.trans (funext fun x => propext ?_)⟩
  constructor
  · rintro ⟨h₁, h₂⟩; exact ⟨(hA x (hl l t hl' ht x h₂)).mp h₁, h₂⟩
  · rintro ⟨h₁, h₂⟩; exact ⟨(hA x (hl l t hl' ht x h₂)).mpr h₁, h₂⟩

/-- What a typed value writes is bounded by its type (L4). -/
theorem Typ.sites {Θ : Facts} {v : Tm} {T : Ty} {φ : Stage} (hv : Value v)
    (h : Typ S Ψ Θ [] v (.ex T φ)) :
    (∀ p, v.Has (.rgn p) → T.reach (.r p)) ∧
      ∀ l t, v.Has (.loc l) → Ψ[l]? = some t → ∀ x, t.reach (.r x) → T.reach (.r x) := by
  have hh := (h.holds hv rfl).1
  exact ⟨fun p hp => hh _ (.inr (.inl ⟨p, hp, rfl⟩)),
    fun l t hl ht x hx => hh _ (.inr (.inr ⟨l, t, hl, ht, hx⟩))⟩

/-- A typed value moves to a place that agrees with its own on what its type shows. -/
theorem Tm.Within.moveT {Θ : Facts} {v : Tm} {T : Ty} {φ : Stage} {A₁ A₂ : Nat → Prop}
    (hv : Value v) (ht : Typ S Ψ Θ [] v (.ex T φ)) (h : v.Within Ψ B anc A₁)
    (hA : ∀ x, T.reach (.r x) → (A₁ x ↔ A₂ x)) : v.Within Ψ B anc A₂ :=
  let ⟨hr, hl⟩ := Typ.sites hv ht
  h.move (hv.noIns h) hr hl hA

theorem Frame.Within.mono {Ψ' : List Ty} {B' : Nat → Nat → Prop} {F : Frame} {A : Nat → Prop}
    (hl : ∀ A l, LocOk Ψ B A l → LocOk Ψ' B' A l) (hh : ∀ p ℓ, HCell Ψ p ℓ → HCell Ψ' p ℓ)
    (h : F.Within Ψ B anc A) : F.Within Ψ' B' anc A := by
  have m : ∀ {x : Tm} {A}, x.Within Ψ B anc A → x.Within Ψ' B' anc A := fun hx => hx.mono hl hh fun _ _ => rfl
  cases F <;> simp only [Frame.Within, HLocs] at h ⊢ <;> grind

theorem WithinE.mono {Ψ' : List Ty} {B' : Nat → Nat → Prop} {A : Nat → Prop}
    (hl : ∀ A l, LocOk Ψ B A l → LocOk Ψ' B' A l) (hh : ∀ p ℓ, HCell Ψ p ℓ → HCell Ψ' p ℓ) :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → WithinE Ψ' B' anc A E
  | [], _ => trivial
  | _ :: _, ⟨hE, hF⟩ => ⟨WithinE.mono hl hh hE, hF.mono hl hh⟩

theorem plug_frame {q : Nat} {x : Tm} : ∀ {E : List Frame}, .inside q ∈ E → (plug E x).Has (.frame q)
  | F :: E, h => by
    rcases List.mem_cons.mp h with rfl | h
    · exact plug_has E (by simp [Frame.plug, Tm.Has])
    · exact plug_frame (x := F.plug x) h

theorem plug_frames_iff {E : List Frame} {x : Tm} {q : Nat} (hx : ∀ q, ¬ x.Has (.frame q)) :
    (plug E x).Has (.frame q) ↔ (plug E (.var 0)).Has (.frame q) :=
  ⟨fun h => (plug_has_inv E h).resolve_right (hx q), plug_of_hole⟩

/-- The redex of a thread replaced by a term without `inside` forms at its place. -/
theorem Nest.redex {Ψ' : List Ty} {c : Cfg} {μ' : List Tm} {B' home i E r r'}
    (hn : Nest Ψ c anc B home) (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r)
    (hr : ∀ q, ¬ r.Has (.frame q)) (hr' : ∀ q, ¬ r'.Has (.frame q))
    (hl : ∀ A l, LocOk Ψ B A l → LocOk Ψ' B' A l) (hh : ∀ p ℓ, HCell Ψ p ℓ → HCell Ψ' p ℓ)
    (h' : (plug E r).Within Ψ B anc (baseOf anc i) → r.Within Ψ B anc (holeE (baseOf anc i) E) →
      r'.Within Ψ' B' anc (holeE (baseOf anc i) E))
    (hc : ∀ (l : Nat) (v : Tm), μ'[l]? = some v → v.Within Ψ' B' anc (B' l) ∧ v.NoIns) :
    Nest Ψ' ((⟨c.rgs, μ', c.main⟩ : Cfg).put i (plug E r')) anc B' home := by
  have hw := (hn.thread i _ hi).1
  obtain ⟨-, hrW⟩ := within_of_plug hv hE hw
  refine hn.put_local hi hl hh ((hw.mono hl hh fun _ _ => rfl).replug hv hE (h' hw hrW)
    fun x hx => (hr' x hx).elim) (fun q => ?_) hc
  rw [plug_frames_iff hr', plug_frames_iff hr]

/-- New children and finalizers of a live region, at its place. -/
theorem Nest.regs {c : Cfg} {home p g g'} (hn : Nest Ψ c anc B home) (hg : c.rgs[p]? = some g)
    (hl : g.live = true) (hl' : g'.live = true)
    (hk : ∃ ks, g'.kids = g.kids ++ ks ∧ ∀ k ∈ ks, k.Within Ψ B anc (anc p) ∧ ∀ q, ¬ k.Has (.frame q))
    (hf : ∀ f ∈ g'.fins, f.Within Ψ B anc (anc p) ∧ ∀ q, ¬ f.Has (.frame q)) :
    Nest Ψ ⟨setRgn c.rgs p g', c.mem, c.main⟩ anc B home := by
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

end Flick
