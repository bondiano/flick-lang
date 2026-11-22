import Formal.Equiv.ThreadC

/-!
# Contextual equivalence: registrations, roots, globals, errors and signals

`addFin_sim`, `spawn_sim`, `root_sim`, `glob_sim`, `raise_sim`, `unw_sim`: the right side takes
the same step once it has caught up the focus and left the frames of its own cells.
-/

namespace Flick.Equiv

open Flick.Elab (All2 fin_redex raise_redex unw_redex perform_redex)

variable {W : World}

/-- An answer by catching up, then one step. -/
theorem TRes.last {root : Roots} {τ : Option (Nat × Nat)} {Λ₂ Λ' Λ₂' : List Rgn}
    {μ μ' μ₂ μ₂a μ₂' : List Tm} {t₂ t₂a t' t₂' : Tm} {l : Lbl} {W' W'' : World}
    (hj : RJ W μ₂ t₂ W' μ₂a t₂a) (hs : Step root l Λ₂ μ₂a t₂a (.cfg Λ₂' μ₂' t₂'))
    (hW : W'.le W'') (ht : ThR W'' μ' τ t' t₂') (hst : StR W'' μ' μ₂') (hk : KeepO W τ μ μ')
    (hrg : RgsR W'' μ' τ Λ' Λ₂') : TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' :=
  ⟨W'', Λ₂', μ₂', t₂', hj.le.trans hW, ⟨_, _, l, hj.ksteps, hs⟩, ht, hst, hk, hrg⟩

theorem fin_app {c : Const} {p : Nat} {t : Tm} (hc : c = .addFin ∨ c = .spawn) (ht : Value t) :
    ∀ f a, Tm.app (.app (.const c) (.own p)) t = .app f a → Value f ∧ Value a := by
  intro f a h
  cases h
  exact ⟨.constApp (by rcases hc with rfl | rfl <;> rfl) .own, ht⟩

/-- **A finalizer registered** with a live owner. -/
theorem addFin_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {p : Nat} {g : Rgn} {t t₂ : Tm} (hE : OkE E) (hg : Λ[p]? = some g)
    (hl : g.live = true) (hv : Value t)
    (ht : Sim W .any (plug E (.app (.app (.const .addFin) (.own p)) t)) t₂) (hst : StR W μ μ₂)
    (hR : W.R = root) (hcl₁ : (plug E (.app (.app (.const .addFin) (.own p)) t)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ (setRgn Λ p ⟨g.live, g.kids, g.fins ++ [withs p E (.await t)]⟩) μ
      (plug E (.lit .unit)) := by
  refine focus_cases hE (fin_redex (.inl rfl) hv).1 (fin_app (.inl rfl) hv) (fun _ _ _ h => by cases h)
    id (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg (.addFin hE hg hl hv) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨f₂, t₂', rfl, h₁, h₂⟩ := hs.inv_app
  obtain ⟨c₂, o₂, rfl, h₃, h₄⟩ := h₁.inv_app
  obtain rfl := h₃.inv_const
  obtain rfl := h₄.inv_own
  obtain ⟨g₂, hg₂, hr⟩ := (hrg.keep hj.le keepO_refl).get hg
  have hv₂ := h₂.top_value rfl hv
  exact TRes.last hj (.addFin hE₂ hg₂ (hr.live ▸ hl) hv₂) (World.le.refl _) (.inl (hfr.plugs .lit))
    (hj.st _ hst) keepO_refl ((hrg.keep hj.le keepO_refl).set hg hg₂
      (hr.addFin (hfr.withs p (.await h₂.lower)) (not_final_withs not_final_await)))

/-- **A child spawned** by a live owner. -/
theorem spawn_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {p : Nat} {g : Rgn} {t t₂ : Tm} (hE : OkE E) (hg : Λ[p]? = some g)
    (hl : g.live = true) (hv : Value t)
    (ht : Sim W .any (plug E (.app (.app (.const .spawn) (.own p)) t)) t₂) (hst : StR W μ μ₂)
    (hR : W.R = root) (hcl₁ : (plug E (.app (.app (.const .spawn) (.own p)) t)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ (setRgn Λ p ⟨g.live, g.kids ++ [withs p E (.await t)], g.fins⟩) μ
      (plug E (.lit .unit)) := by
  refine focus_cases hE (fin_redex (.inr rfl) hv).1 (fin_app (.inr rfl) hv) (fun _ _ _ h => by cases h)
    id (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg (.spawn hE hg hl hv) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨f₂, t₂', rfl, h₁, h₂⟩ := hs.inv_app
  obtain ⟨c₂, o₂, rfl, h₃, h₄⟩ := h₁.inv_app
  obtain rfl := h₃.inv_const
  obtain rfl := h₄.inv_own
  obtain ⟨g₂, hg₂, hr⟩ := (hrg.keep hj.le keepO_refl).get hg
  have hv₂ := h₂.top_value rfl hv
  exact TRes.last hj (.spawn hE₂ hg₂ (hr.live ▸ hl) hv₂) (World.le.refl _) (.inl (hfr.plugs .lit))
    (hj.st _ hst) keepO_refl ((hrg.keep hj.le keepO_refl).set hg hg₂
      (hr.addKid (hfr.withs p (.await h₂.lower)) (not_final_withs not_final_await)))

/-- **An operation `Σ₀` answers.** -/
theorem root_sim {root : Roots} (hroot : root.Src) {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E : List Frame} {Ef o : Name} {w f v t₂ : Tm} (hE : OkE E)
    (hh : ∀ G ∈ E, G.handles Ef o = none) (hr : root.svc Ef = some w) (hf : w.field o = some f)
    (hv : Value v) (ht : Sim W .any (plug E (.app (.const (.perform Ef o .tail)) v)) t₂)
    (hst : StR W μ μ₂) (hR : W.R = root)
    (hcl₁ : (plug E (.app (.const (.perform Ef o .tail)) v)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ (plug E (.app f v)) := by
  refine focus_cases hE (perform_redex hv).1 (fun _ _ h => Redex.app (perform_redex hv) h)
    (fun _ _ _ h => by cases h) id (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg
    (.performRoot hE hh hr hf hv) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨c₂, v₂, rfl, h₁, h₂⟩ := hs.inv_app
  obtain rfl := h₁.inv_const
  have hv₂ := h₂.top_value rfl hv
  exact TRes.last hj (.performRoot hE₂ (hfr.handlesE hh) hr hf hv₂) (World.le.refl _)
    (.inl (hfr.plugs (.app (by decide) (Sim.refl ((hroot.1 _ _ hr).field hf) _ (by decide))
      h₂.lower))) (hj.st _ hst) keepO_refl (hrg.keep hj.le keepO_refl)

/-- **A global** steps to its body. -/
theorem glob_sim {root : Roots} (hroot : root.Src) {μ μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E : List Frame} {g : Nat} {v t₂ : Tm} (hE : OkE E)
    (hg : root.glob g = some v) (ht : Sim W .any (plug E (.glob g)) t₂) (hst : StR W μ μ₂)
    (hR : W.R = root) (hcl₁ : (plug E (.glob g)).Closed) (hcl₁' : (plug E v).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) :
    KAns root W τ μ₂ t₂ μ μ (plug E (.glob g)) (plug E v) := by
  rcases ht.dec (glob_redex g).1 (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
      _ _ rfl hE μ₂ (fun _ => ⟨hcl₁, hcl₂ _ _ .refl⟩) with
    hd | ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs⟩ |
    ⟨_, _, _, _, _, _, _, _, _, h', _, _⟩
  · exact (dec_act (τ := τ) (Λ := []) hE hst hR hroot.2 hcl₁ hcl₁' hcl₂ (.glob hE hg) hd).2
  · obtain rfl := hs.inv_glob
    exact ⟨W', μ₂', plug E₂ v, hj.le,
      .inl (hj.ksteps.pos ⟨_, _, fun _ => ⟨_, .glob hE₂ hg⟩, .refl⟩),
      .inl (hfr.plugs (Sim.refl (hroot.2 _ _ hg) _ (by decide))), hj.st _ hst,
      fun _ _ _ _ _ h => h⟩
  · cases h'

end Flick.Equiv
