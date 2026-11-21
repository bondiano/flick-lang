import Formal.Equiv.RegionB

/-!
# Contextual equivalence: owners, capabilities and the end of a region

`open_sim`, `op_sim`, `finRun_sim`, `close_sim`.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem unary_not_value {c : Const} {v : Tm} (hc : c.arity = 1) : ¬ Value (.app (.const c) v) := by
  intro h
  cases h with
  | ctorApp _ hK _ => simp [Tm.head] at hK
  | constApp h₂ _ => rw [hc] at h₂; cases h₂

theorem unary_not_final {c : Const} {v : Tm} (hc : c.arity = 1) : ¬ Final (.app (.const c) v) := by
  rintro (h | ⟨_, _, h⟩ | ⟨_, _, _, h⟩)
  · exact unary_not_value hc h
  · cases h
  · cases h

/-- **An owner opens** a capability, registering its finalizer. -/
theorem open_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {p : Nat} {g : Rgn} {t₂ : Tm} (hE : OkE E) (hg : Λ[p]? = some g)
    (hl : g.live = true) (ht : Sim W .any (plug E (.app (.const .open_) (.own p))) t₂)
    (hst : StR W μ μ₂) (hR : W.R = root) (hcl₁ : (plug E (.app (.const .open_) (.own p))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ
      (setRgn Λ p ⟨g.live, g.kids, g.fins ++ [.app (.const .op) (.cap p)]⟩) μ (plug E (.cap p)) := by
  refine focus_cases hE (unary_not_value rfl)
    (fun _ _ h => by cases h; exact ⟨.const, .own⟩) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg (.rt hE (.open_ hg hl)) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨c₂, o₂, rfl, h₁, h₂⟩ := hs.inv_app
  obtain rfl := h₁.inv_const
  obtain rfl := h₂.inv_own
  have hrg' := hrg.keep hj.le keepO_refl
  obtain ⟨g₂, hg₂, hr⟩ := hrg'.get hg
  exact TRes.last hj (.rt hE₂ (.open_ hg₂ (hr.live ▸ hl))) (World.le.refl _) (.inl (hfr.plugs .cap))
    (hj.st _ hst) keepO_refl (hrg'.set hg hg₂ (hr.addFin (.app (by decide) .const .cap)
      (unary_not_final rfl)))

/-- **The operation of a capability** of a live region. -/
theorem op_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {p : Nat} {g : Rgn} {t₂ : Tm} (hE : OkE E) (hg : Λ[p]? = some g)
    (hl : g.live = true) (ht : Sim W .any (plug E (.app (.const .op) (.cap p))) t₂)
    (hst : StR W μ μ₂) (hR : W.R = root) (hcl₁ : (plug E (.app (.const .op) (.cap p))).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ μ (plug E (.lit .unit)) := by
  refine focus_cases hE (unary_not_value rfl)
    (fun _ _ h => by cases h; exact ⟨.const, .cap⟩) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg (.rt hE (.op hg hl)) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨c₂, o₂, rfl, h₁, h₂⟩ := hs.inv_app
  obtain rfl := h₁.inv_const
  obtain rfl := h₂.inv_cap
  have hrg' := hrg.keep hj.le keepO_refl
  obtain ⟨g₂, hg₂, hr⟩ := hrg'.get hg
  exact TRes.last hj (.rt hE₂ (.op hg₂ (hr.live ▸ hl))) (World.le.refl _) (.inl (hfr.plugs .lit))
    (hj.st _ hst) keepO_refl hrg'

/-- The position of the stepping thread. -/
def ThreadAt (τ : Option (Nat × Nat)) (Λ : List Rgn) (t : Tm) : Prop :=
  ∀ p j, τ = some (p, j) → ∃ g, Λ[p]? = some g ∧ g.kids[j]? = some t

/-- **A finalizer starts** once the body and the children of its region are finished. -/
theorem finRun_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {p : Nat} {g : Rgn} {w f t₂ : Tm} {fs : List Tm} (hE : OkE E)
    (hg : Λ[p]? = some g) (hl : g.live = true) (hw : Final w) (hk : ∀ k ∈ g.kids, Final k)
    (hfs : g.fins = fs ++ [f]) (ht : Sim W .any (plug E (.inside p w)) t₂) (hst : StR W μ μ₂)
    (hR : W.R = root) (hcl₁ : (plug E (.inside p w)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hpos : ThreadAt τ Λ (plug E (.inside p w))) :
    TRes root W τ Λ₂ μ₂ t₂ μ (setRgn Λ p ⟨g.live, g.kids ++ [f], fs⟩) μ (plug E (.inside p w)) := by
  refine focus_cases hE (fun h => by cases h) (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg (.rt hE (.finRun hg hl hw hk hfs)) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨e₂, rfl, h₁⟩ := hs.inv_inside
  obtain ⟨W₁, μ₁, w₂, r₁, hw₂, s₁⟩ := h₁.fcu rfl hw μ₂'
    (fun _ => ⟨closed_frame_inner (F := .inside p) (plug_closed hcl₁),
      closed_frame_inner (F := .inside p) (plug_closed (hcl₂ _ _ hj.ksteps))⟩)
  have hrg' := hrg.keep (hj.le.trans r₁.le) keepO_refl
  obtain ⟨g₂, hg₂, hr⟩ := hrg'.get hg
  have hk₂ := kids_final hr hl hk (fun j hτ => by
    obtain ⟨g', hg', hk'⟩ := hpos p j hτ
    rw [hg] at hg'; cases hg'; exact hk') inside_not_final
  obtain ⟨fs₂, f₂, hfs₂, hfa, hff⟩ := All2.snoc_split (hfs ▸ hr.fins)
  exact TRes.last (hj.trans ((r₁.frame (F := .inside p) trivial).plug hE₂))
    (.rt hE₂ (.finRun hg₂ (hr.live ▸ hl) hw₂ hk₂ hfs₂)) (World.le.refl _)
    (.inl ((hfr.mono r₁.le).plugs (.inside s₁))) (r₁.st _ (hj.st _ hst)) keepO_refl
    (hrg'.set hg hg₂ (hr.runFin hff.1 hff.2 hfa))

/-- **A region closes** once its body, its children and its finalizers are finished. -/
theorem close_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {p : Nat} {g : Rgn} {w t₂ : Tm} (hE : OkE E)
    (hg : Λ[p]? = some g) (hl : g.live = true) (hw : Final w) (hk : ∀ k ∈ g.kids, Final k)
    (hfs : g.fins = []) (ht : Sim W .any (plug E (.inside p w)) t₂) (hst : StR W μ μ₂)
    (hR : W.R = root) (hcl₁ : (plug E (.inside p w)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hpos : ThreadAt τ Λ (plug E (.inside p w))) :
    TRes root W τ Λ₂ μ₂ t₂ μ (setRgn Λ p ⟨false, g.kids, []⟩) μ (plug E w) := by
  refine focus_cases hE (fun h => by cases h) (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hR hcl₁ hcl₂ hrg (.rt hE (.close hg hl hw hk hfs)) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨e₂, rfl, h₁⟩ := hs.inv_inside
  obtain ⟨W₁, μ₁, w₂, r₁, hw₂, s₁⟩ := h₁.fcu rfl hw μ₂'
    (fun _ => ⟨closed_frame_inner (F := .inside p) (plug_closed hcl₁),
      closed_frame_inner (F := .inside p) (plug_closed (hcl₂ _ _ hj.ksteps))⟩)
  have hrg' := hrg.keep (hj.le.trans r₁.le) keepO_refl
  obtain ⟨g₂, hg₂, hr⟩ := hrg'.get hg
  have hk₂ := kids_final hr hl hk (fun j hτ => by
    obtain ⟨g', hg', hk'⟩ := hpos p j hτ
    rw [hg] at hg'; cases hg'; exact hk') inside_not_final
  have hfs₂ := All2.nil_left (hfs ▸ hr.fins)
  exact TRes.last (hj.trans ((r₁.frame (F := .inside p) trivial).plug hE₂))
    (.rt hE₂ (.close hg₂ (hr.live ▸ hl) hw₂ hk₂ hfs₂)) (World.le.refl _)
    (.inl ((hfr.mono r₁.le).plugs s₁)) (r₁.st _ (hj.st _ hst)) keepO_refl
    (hrg'.set hg hg₂ ⟨rfl, hr.len, hr.kids, fun _ _ _ _ h => (by cases h), Flick.Elab.All2.nil⟩)

end Flick.Equiv
