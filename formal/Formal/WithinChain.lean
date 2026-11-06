import Formal.Within

/-!
# Places along a thread are chains of ancestors, and what a thread reaches
-/

namespace Flick

variable {Ψ : List Ty} {B : Nat → Nat → Prop} {anc : Nat → Nat → Prop}

/-- Regions whose ancestors are the regions before them. -/
def Chain (anc : Nat → Nat → Prop) (A : Nat → Prop) : Prop :=
  ∀ x, A x → anc x = fun y => A y ∧ y ≤ x

theorem Chain.inside {A : Nat → Prop} {p : Nat} (h : Chain anc A) (hp : ¬ A p)
    (ha : anc p = fun x => A x ∨ x = p) (hle : ∀ x, anc p x → x ≤ p) :
    Chain anc (fun x => A x ∨ x = p) := by
  have lt : ∀ x, A x → x < p := fun x hx =>
    Nat.lt_of_le_of_ne (hle x (by rw [ha]; exact .inl hx)) fun h' => hp (h' ▸ hx)
  rintro x (hx | rfl)
  · rw [h x hx]; funext y
    have := lt x hx
    apply propext; constructor
    · rintro ⟨h₁, h₂⟩; exact ⟨.inl h₁, h₂⟩
    · rintro ⟨h₁ | rfl, h₂⟩; exact ⟨h₁, h₂⟩; omega
  · rw [ha]; funext y
    apply propext; constructor
    · rintro (h₁ | rfl); exact ⟨.inl h₁, Nat.le_of_lt (lt y h₁)⟩; exact ⟨.inr rfl, Nat.le_refl _⟩
    · exact fun h => h.1

theorem WithinE.chain {A : Nat → Prop} (hle : ∀ q x, anc q x → x ≤ q) :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → Chain anc A → Chain anc (holeE A E)
  | [], _, h => h
  | F :: E, ⟨hE, hF⟩, h => by
    have h' := WithinE.chain hle hE h
    cases F <;> simp only [holeE, Frame.hole] <;> try exact h'
    exact h'.inside hF.1 hF.2 (hle _)

/-- What a term reaches through the store is a region of its place or of an
`inside` form in it, when every cell holds only what its locations see. -/
theorem Tm.Within.reaches {μ : List Tm}
    (hμ : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Within Ψ B anc (B l) ∧ v.NoIns) {e : Tm} {p : Nat}
    (hr : Reaches μ e p) : ∀ {A : Nat → Prop}, e.Within Ψ B anc A → A p ∨ e.Has (.frame p) := by
  induction hr with
  | here hp => exact fun h => h.rgn hp
  | cell hl hv _ ih =>
    intro A h
    obtain ⟨hw, hn⟩ := hμ _ _ hv
    obtain ⟨A', h₂, t, _, hB⟩ := h.loc hl
    rcases ih hw with h' | h'
    · rw [hB] at h'; exact h₂ _ h'.1
    · exact absurd h' (hn.frame _)

end Flick
