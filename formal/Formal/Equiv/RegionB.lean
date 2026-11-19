import Formal.Equiv.Region

/-!
# Contextual equivalence: steps of regions, one by one

`scope_sim`, `hdl_sim`, `open_sim`, `op_sim`, `finRun_sim`, `close_sim`.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem inside_not_final {p : Nat} {E : List Frame} {w : Tm} : ¬ Final (plug E (.inside p w)) := by
  intro h
  have hn₀ : ¬ Value (Tm.inside p w) := by intro h; cases h
  rcases snoc_or E with rfl | ⟨E₀, G, rfl⟩
  · rcases h with h | ⟨_, _, h⟩ | ⟨_, _, _, h⟩ <;> cases h
  · have hn := not_value_plugs hn₀ E₀
    rw [plug_snoc] at h
    rcases h with h | ⟨v, hv, h⟩ | ⟨_, _, _, h⟩
    · exact not_value_plugs hn₀ (E₀ ++ [G]) (by rw [plug_snoc]; exact h)
    · cases G <;> simp only [Frame.plug] at h <;> cases h <;> exact hn hv
    · cases G <;> simp only [Frame.plug] at h <;> cases h

/-- **A scope** opens a new region on both sides. -/
theorem scope_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {e t₂ : Tm} (hE : OkE E) (ht : Sim W .any (plug E (.scope e)) t₂)
    (hst : StR W μ μ₂) (hcl₁ : (plug E (.scope e)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ (Λ ++ [Rgn.new]) μ
      (plug E (.inside Λ.length (e.subst (inst [.own Λ.length])))) := by
  refine focus_cases hE (fun h => by cases h) (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg (.rt hE .scope) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨e₂, rfl, h₁⟩ := hs.inv_scope
  have hl := hrg.1
  exact TRes.last hj (.rt hE₂ .scope) (World.le.refl _)
    (.inl (hfr.plugs (by rw [hl]; exact .inside (h₁.subst _ _ (SubR.inst
      (.cons (Sim.sub (Sim.own (x := .any)) .own .own) .nil))))))
    (hj.st _ hst) keepO_refl ((hrg.keep hj.le keepO_refl).append)

/-- **A handler** opens a new region and a new cell on both sides. -/
theorem hdl_sim {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm} {τ : Option (Nat × Nat)}
    {E : List Frame} {Ef : Name} {al r e t₂ : Tm} (hE : OkE E)
    (ht : Sim W .any (plug E (.hdl Ef al r e)) t₂) (hst : StR W μ μ₂)
    (hcl₁ : (plug E (.hdl Ef al r e)).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂) :
    TRes root W τ Λ₂ μ₂ t₂ μ (Λ ++ [Rgn.new]) (μ ++ [.lam .todo])
      (plug E (.inside Λ.length (.hwith Λ.length μ.length Ef al r e))) := by
  refine focus_cases hE (fun h => by cases h) (fun _ _ h => by cases h) (fun _ _ _ h => by cases h) id
    (fun _ _ h => by cases h) ht hst hcl₁ hcl₂ hrg (.rt hE .hdl) ?_
  intro W' μ₂' E₂ r₂ hj hfr hE₂ hs
  obtain ⟨al₂, r₂', e₂, rfl, hal, hr, he⟩ := hs.inv_hdl
  have hl := hrg.1
  have hst' := hj.st _ hst
  have hW := World.le_addL W' μ.length μ₂'.length .sym
  have hκ : (World.addL W' μ.length μ₂'.length .sym).L μ.length μ₂'.length .sym := .inr ⟨rfl, rfl, rfl⟩
  exact TRes.last hj (.rt hE₂ .hdl) hW
    (.inl ((hfr.mono hW).plugs (by rw [hl]; exact .inside (.hwith hκ (hal.mono hW) (hr.mono hW)
      (he.mono hW)))))
    (hst'.alloc (.lam .todo (by simp [Tm.NoIns, Tm.Has]))) (KeepO.append hst _)
    (((hrg.keep hj.le keepO_refl).keep hW (KeepO.append hst' _)).append)

end Flick.Equiv
