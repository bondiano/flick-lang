import Formal.NestTyping

/-!
# A step keeps `⊢ Λ`
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem PHead.noIns {r r' : Tm} {A : Nat → Prop} (hh : PHead r r') (h : r.Within Ψ B anc A)
    (hw : ∀ q θ v, r ≠ .with_ q θ v) : r.NoIns := by
  have vn : ∀ {x : Tm} {A}, Value x → x.Within Ψ B anc A → ¬ x.Has .ins := fun hv hx => hv.noIns hx
  cases hh <;> simp only [Tm.Within] at h <;> simp only [Tm.NoIns, Tm.Has, not_or] <;>
    first
    | exact (hw _ _ _ rfl).elim
    | exact ⟨h.1.2, vn ‹_› h.2.1⟩
    | exact ⟨vn ‹_› h.1, h.2.2⟩
    | exact ⟨h.2.2.1, h.2.2.2⟩
    | exact vn ‹_› h
    | exact ⟨vn ‹_› h.1, vn ‹_› h.2.1⟩
    | exact ⟨h.2.2, vn ‹_› h.2.1⟩
    | exact ⟨vn (IsEnv.value ‹_›) h.1, vn (IsEnv.value ‹_›) h.2.1⟩
    | (have h3 := h.2.2; simp only [Tm.NoIns, Tm.Has, not_or] at h3; exact ⟨vn ‹_› h.1, h3⟩)

theorem Frame.Within.noFrame_plug {F : Frame} {A : Nat → Prop} {x : Tm} (hF : F.Within Ψ B anc A)
    (hni : ¬ F.isInside) (hx : ∀ q, ¬ x.Has (.frame q)) : ∀ q, ¬ (F.plug x).Has (.frame q) := by
  have vn : ∀ {y : Tm} {A}, Value y → y.Within Ψ B anc A → ¬ y.Has .ins := fun hv hy => hv.noIns hy
  intro q h
  cases F <;> simp only [Frame.Within, Frame.isInside] at hF hni <;>
    simp only [Frame.plug, Tm.Has] at h <;>
    first
    | exact hni trivial
    | exact hx q h
    | (rcases h with h | h
       · exact hx q h
       · first | exact hF.2 (Tm.has_frame_ins h) | exact vn hF.2 hF.1 (Tm.has_frame_ins h))
    | (rcases h with h | h
       · first | exact hF.2 (Tm.has_frame_ins h) | exact vn hF.2 hF.1 (Tm.has_frame_ins h)
       · exact hx q h)
    | (rcases h with h | h | h | h
       · cases h
       · cases h
       · exact hF.1 (Tm.has_frame_ins h)
       · exact hx q h)

/-- Replace the redex by a term that writes only what the redex writes. -/
theorem Nest.sub {c : Cfg} {home i E r r'} (hn : Nest Ψ c anc B home) (hi : c.at i = some (plug E r))
    (hE : OkE E) (hv : ¬ Value r) (hr : r.NoIns) (hs : ∀ s, r'.Has s → r.Has s) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E r')) anc B home :=
  hn.redex hi hE hv hr.frame (fun _ h => hr (Tm.has_frame_ins (hs _ h))) (fun _ _ h => h)
    (fun _ hrW => Tm.Within.of_has (fun h => hr (hs _ h)) (fun _ hp => hrW.rgn0 hr (hs _ hp))
      fun _ hl => hrW.loc0 hr (hs _ hl)) hn.cells

theorem Cfg.at_grow {Λ : List Rgn} {μ : List Tm} {m : Tm} {p : Nat} {g g' : Rgn} {i e}
    (hg : Λ[p]? = some g) (hl : g'.live = g.live) (hk : ∃ ks, g'.kids = g.kids ++ ks)
    (h : (⟨Λ, μ, m⟩ : Cfg).at i = some e) : (⟨setRgn Λ p g', μ, m⟩ : Cfg).at i = some e := by
  rcases i with _ | ⟨r, j⟩
  · exact h
  · obtain ⟨g'', h₁, h₂, h₃⟩ := Cfg.at_some.mp h
    apply Cfg.at_some.mpr
    by_cases hr : r = p
    · subst hr; rw [hg] at h₁; cases h₁
      obtain ⟨ks, hks⟩ := hk
      exact ⟨g', by simp [setRgn, List.getElem?_set_self (getElem?_lt hg)], hl.trans h₂, by
        rw [hks, List.getElem?_append_left (getElem?_lt h₃)]; exact h₃⟩
    · exact ⟨g'', by rw [setRgn, List.getElem?_set_ne (Ne.symm hr)]; exact h₁, h₂, h₃⟩

/-- The regions of the hole of a thread are a chain. -/
theorem Nest.hole_chain {c : Cfg} {home i E} (hn : Nest Ψ c anc B home) {e : Tm} (hi : c.at i = some e)
    (hE : WithinE Ψ B anc (baseOf anc i) E) : Chain anc (holeE (baseOf anc i) E) :=
  hE.chain hn.le (hn.base_chain hi)

/-- The work given to an owner `p` at the hole moves to the place of `p`. -/
theorem Nest.to_owner {c : Cfg} {home i E} (hn : Nest Ψ c anc B home) {e : Tm} (hi : c.at i = some e)
    (hE : WithinE Ψ B anc (baseOf anc i) E) {p : Nat} (hp : holeE (baseOf anc i) E p)
    {Θ : Facts} {w : Tm} {U : Ty} {φ : Stage} (hv : Value w) (ht : Typ S Ψ Θ [] w (.ex U φ))
    (hU : ∀ x, U.reach (.r x) → x ≤ p) (hw : w.Within Ψ B anc (holeE (baseOf anc i) E)) :
    w.Within Ψ B anc (anc p) := by
  have hc := hn.hole_chain hi hE p hp
  refine hw.moveT hv ht fun x hx => ?_
  rw [hc]
  exact ⟨fun h => ⟨h, hU x hx⟩, fun h => h.1⟩

/-- New children and finalizers of a live region, and the redex replaced. -/
theorem Nest.grow {c : Cfg} {home i E r r' p g g'} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r) (hr : r.NoIns)
    (hs : ∀ s, r'.Has s → r.Has s) (hg : c.rgs[p]? = some g) (hl : g.live = true) (hl' : g'.live = g.live)
    (hk : ∃ ks, g'.kids = g.kids ++ ks ∧ ∀ k ∈ ks, k.Within Ψ B anc (anc p) ∧ ∀ q, ¬ k.Has (.frame q))
    (hf : ∀ f ∈ g'.fins, f.Within Ψ B anc (anc p) ∧ ∀ q, ¬ f.Has (.frame q)) :
    Nest Ψ ((⟨setRgn c.rgs p g', c.mem, c.main⟩ : Cfg).put i (plug E r')) anc B home :=
  (hn.regs hg hl (hl'.trans hl) hk hf).sub
    (Cfg.at_grow hg hl' (let ⟨ks, h, _⟩ := hk; ⟨ks, h⟩) hi) hE hv hr hs

/-- New children and finalizers of a live region, the thread kept. -/
theorem Nest.grow0 {c : Cfg} {home i e p g g'} (hn : Nest Ψ c anc B home) (hi : c.at i = some e)
    (hg : c.rgs[p]? = some g) (hl : g.live = true) (hl' : g'.live = g.live)
    (hk : ∃ ks, g'.kids = g.kids ++ ks ∧ ∀ k ∈ ks, k.Within Ψ B anc (anc p) ∧ ∀ q, ¬ k.Has (.frame q))
    (hf : ∀ f ∈ g'.fins, f.Within Ψ B anc (anc p) ∧ ∀ q, ¬ f.Has (.frame q)) :
    Nest Ψ ((⟨setRgn c.rgs p g', c.mem, c.main⟩ : Cfg).put i e) anc B home :=
  have hn₁ := hn.regs hg hl (hl'.trans hl) hk hf
  have hi₁ := Cfg.at_grow hg hl' (let ⟨ks, h, _⟩ := hk; ⟨ks, h⟩) hi
  hn₁.put_local hi₁ (fun _ _ h => h) (hn₁.thread i e hi₁).1 (fun _ => Iff.rfl) hn₁.cells

theorem WithinE.with_frame {A : Nat → Prop} {q : Nat} {θ : Tm} :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → .with_ q θ ∈ E → θ.NoIns ∧ θ.Within Ψ B anc (fun _ => False)
  | F :: E, ⟨hE, hF⟩, h => by
    rcases List.mem_cons.mp h with rfl | h
    · exact hF
    · exact WithinE.with_frame hE h

/-- A child or a finalizer in the `with` frames of its context, at the place of its owner. -/
theorem withs_within {A : Nat → Prop} {x : Tm} :
    ∀ {E : List Frame}, (∀ q θ, .with_ q θ ∈ E → θ.NoIns ∧ θ.Within Ψ B anc (fun _ => False)) →
      x.Within Ψ B anc A → (∀ q, ¬ x.Has (.frame q)) →
      (withs E x).Within Ψ B anc A ∧ ∀ q, ¬ (withs E x).Has (.frame q)
  | [], _, hx, hf => ⟨hx, hf⟩
  | F :: E, hE, hx, hf => by
    simp only [withs]
    refine withs_within (fun q θ h => hE q θ (List.mem_cons_of_mem _ h)) ?_ ?_
    · cases F with
      | with_ q θ => exact ⟨(hE q θ List.mem_cons_self).1, (hE q θ List.mem_cons_self).2, hx⟩
      | _ => exact hx
    · cases F with
      | with_ q θ =>
        rintro q' (h | h | h | h)
        · cases h
        · cases h
        · exact (hE q θ List.mem_cons_self).1.frame q' h
        · exact hf q' h
      | _ => exact hf

theorem Frame.hole_eq {F : Frame} {A : Nat → Prop} (h : ¬ F.isInside) : F.hole A = A := by
  cases F <;> simp_all [Frame.hole, Frame.isInside]

end Flick
