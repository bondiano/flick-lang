import Formal.Equiv.ThreadG

/-!
# Contextual equivalence: steps of regions

`scope`, a handler (`hdl`), `open`, the operation of a capability, and the end of a region: a
finalizer starts (`finRun`) or the region closes. The regions of the two sides are one by one, so
the right side takes the same step. A region ends when its body and its children are finished,
and a finished child of a live region is related to a finished one (`RgR.fin`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- **The right side finishes** what the left side finished. -/
theorem Sim.fcu {x : Pos} {w b : Tm} (h : Sim W x w b) (hx : x = .any) (hw : Final w)
    (μ : List Tm) (hcl : W.B → w.Closed ∧ b.Closed) :
    ∃ W' μ' b', RJ W μ b W' μ' b' ∧ Final b' ∧ Sim W' .any w b' := by
  rcases hw with hv | ⟨v, hv, rfl⟩ | ⟨p, ℓ, a, rfl⟩
  · obtain ⟨W', μ', b', r, hb, s⟩ := h.vcu hx hv W μ (World.le.refl _) hcl
    exact ⟨W', μ', b', r, .inl hb, s⟩
  · obtain ⟨W', μ', v', r, hv', s⟩ := h.rcu hx rfl hv W μ (World.le.refl _) hcl
    exact ⟨W', μ', _, r, .inr (.inl ⟨v', hv', rfl⟩), .raise s⟩
  · obtain ⟨W', μ', ℓ', k, a', r, hl, s⟩ := h.ucu hx rfl W μ (World.le.refl _) hcl
    exact ⟨W', μ', _, r, .inr (.inr ⟨p, ℓ', a', rfl⟩), .unw hl s⟩

theorem RgsR.append {μ : List Tm} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn}
    (h : RgsR W μ τ Λ Λ₂) : RgsR W μ τ (Λ ++ [Rgn.new]) (Λ₂ ++ [Rgn.new]) := by
  refine ⟨by simp [h.1], fun q g g₂ hg hg₂ => ?_⟩
  by_cases hq : q < Λ.length
  · rw [List.getElem?_append_left hq] at hg
    rw [List.getElem?_append_left (h.1 ▸ hq)] at hg₂
    exact h.2 q g g₂ hg hg₂
  · rw [List.getElem?_append_right (by omega)] at hg
    rw [List.getElem?_append_right (by rw [← h.1]; omega)] at hg₂
    have one : ∀ {k : Nat} {x : Rgn}, [Rgn.new][k]? = some x → x = Rgn.new := by
      intro k x hx; cases k <;> simp at hx; exact hx.symm
    obtain rfl := one hg
    obtain rfl := one hg₂
    exact ⟨rfl, rfl, fun j _ _ _ h => by simp [Rgn.new] at h, fun j _ _ _ _ h => by simp [Rgn.new] at h,
      .nil⟩

/-- A child that a finalizer becomes. -/
theorem RgR.runFin {μ : List Tm} {τ : Option (Nat × Nat)} {p : Nat} {g g₂ : Rgn} {f f₂ : Tm}
    {fs fs₂ : List Tm} (h : RgR W μ τ p g g₂) (hf : Sim W .any f f₂) (hff : Final f → Final f₂)
    (hfs : All2 (fun f f' => Sim W .any f f' ∧ (Final f → Final f')) fs fs₂) :
    RgR W μ τ p ⟨g.live, g.kids ++ [f], fs⟩ ⟨g₂.live, g₂.kids ++ [f₂], fs₂⟩ := by
  refine ⟨h.live, by simp [h.len], fun j a a' hj ha ha' => ?_, fun j a a' hj hl ha ha' hfa => ?_, hfs⟩
  · simp only [List.getElem?_append] at ha ha'
    by_cases hj' : j < g.kids.length
    · rw [if_pos hj'] at ha; rw [if_pos (h.len ▸ hj')] at ha'
      exact h.kids j a a' hj (by simpa using ha) (by simpa using ha')
    · rw [if_neg hj'] at ha; rw [if_neg (h.len ▸ hj')] at ha'
      simp [List.getElem?_cons, h.len] at ha ha'
      obtain ⟨-, rfl⟩ := ha; obtain ⟨-, rfl⟩ := ha'; exact .inl hf
  · simp only [List.getElem?_append] at ha ha'
    by_cases hj' : j < g.kids.length
    · rw [if_pos hj'] at ha; rw [if_pos (h.len ▸ hj')] at ha'
      exact h.fin j a a' hj hl (by simpa using ha) (by simpa using ha') hfa
    · rw [if_neg hj'] at ha; rw [if_neg (h.len ▸ hj')] at ha'
      simp [List.getElem?_cons, h.len] at ha ha'
      obtain ⟨-, rfl⟩ := ha; obtain ⟨-, rfl⟩ := ha'; exact hff hfa

theorem All2.snoc_split {α β : Type} {R : α → β → Prop} {l : List α} {a : α} {m : List β}
    (h : All2 R (l ++ [a]) m) : ∃ m' b, m = m' ++ [b] ∧ All2 R l m' ∧ R a b := by
  obtain ⟨m₁, m₂, rfl, h₁, h₂⟩ := Flick.Elab.All2.split h
  cases h₂ with
  | cons r h₃ => cases h₃; exact ⟨m₁, _, rfl, h₁, r⟩

theorem All2.nil_left {α β : Type} {R : α → β → Prop} {m : List β} (h : All2 R [] m) : m = [] := by
  cases h; rfl

/-- The children of a region that ends are finished on both sides. -/
theorem kids_final {μ : List Tm} {τ : Option (Nat × Nat)} {p : Nat} {g g₂ : Rgn} {t : Tm}
    (hr : RgR W μ τ p g g₂) (hl : g.live = true) (hk : ∀ k ∈ g.kids, Final k)
    (hpos : ∀ j, τ = some (p, j) → g.kids[j]? = some t) (ht : ¬ Final t) :
    ∀ k ∈ g₂.kids, Final k := by
  intro k₂ hk₂
  obtain ⟨j, hj, rfl⟩ := List.getElem_of_mem hk₂
  have hj' : j < g.kids.length := hr.len ▸ hj
  have hkj : g.kids[j]? = some g.kids[j] := List.getElem?_eq_getElem hj'
  by_cases hτ : some (p, j) = τ
  · rw [hpos j hτ.symm] at hkj; cases hkj
    exact (ht (hk _ (List.getElem_mem hj'))).elim
  · exact hr.fin j _ _ hτ hl hkj (List.getElem?_eq_getElem hj) (hk _ (List.getElem_mem hj'))

end Flick.Equiv
