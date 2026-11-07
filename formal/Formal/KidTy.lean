import Formal.Asc

/-!
# Contexts from the outside in
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty}

theorem list_rev_ind {α : Type} {P : List α → Prop} (h₀ : P []) (h₁ : ∀ E x, P E → P (E ++ [x])) :
    ∀ l, P l := by
  have : ∀ l : List α, P l.reverse := by
    intro l
    induction l with
    | nil => exact h₀
    | cons x l ih => simpa using h₁ _ x ih
  intro l; simpa using this l.reverse

theorem OkE.of_append {E : List Frame} {F : Frame} (h : OkE (E ++ [F])) : OkE E ∧ F.ok :=
  ⟨fun G hG => h G (List.mem_append_left _ hG), h F (by simp)⟩

/-- A handler handles an effect. -/
theorem installs_op {Ψ' Θ x t φ} {F : Frame} {N : Name} (h : Typ S Ψ' Θ [] (F.plug x) (.ex t φ))
    (hi : F.installs N) : ∃ o c a b, S.op N o = some (c, a, b) := by
  cases F <;> simp only [Frame.installs] at hi
  subst hi
  cases h with
  | hwith _ _ _ _ _ _ hc _ _ => exact hc.1
  | errVal hv => cases hv

/-- An operation at the hole of a context that no frame answers has its effect in the stage of
the context: a handler without an arm for it leaves its effect in the row outside. -/
theorem perform_out {Ef o : Name} {c : OpClass} {r : Tm}
    (hask : ∀ {Ψ' Θ' t' φ'}, Typ S Ψ' Θ' [] r (.ex t' φ') → φ'.svc (.nm Ef)) (hv : ¬ Value r)
    (hop : ∃ a b, S.op Ef o = some (c, a, b)) :
    ∀ (E : List Frame) {Ψ : List Ty} {Θ : Facts} {t : Ty} {φ : Stage}, OkE E →
      (∀ F ∈ E, F.handles Ef o c = none) → Typ S Ψ Θ [] (plug E r) (.ex t φ) → φ.svc (.nm Ef) := by
  refine list_rev_ind ?_ ?_
  · intro Ψ Θ t φ _ _ h
    exact hask h
  · intro E₀ G ih Ψ Θ t φ hE hh h
    obtain ⟨hE₀, hG⟩ := OkE.of_append hE
    rw [plug_append] at h
    simp only [plug] at h
    have hnv := not_value_plugs hv E₀
    have hh₀ : ∀ F ∈ E₀, F.handles Ef o c = none := fun F hF => hh F (List.mem_append_left _ hF)
    have hhG : G.handles Ef o c = none := hh G (by simp)
    obtain ⟨k, Θ', t₁, φ₁, h₁, rel, -⟩ := frame_typing hnv _ hG h
    have hn := ih hE₀ hh₀ h₁
    rcases rel.svc _ hn with h₂ | h₂
    · exact h₂
    · cases G <;> simp only [Frame.dom, Frame.installs] at h₂
      subst h₂
      cases h with
      | hwith _ _ _ _ _ _ hc hφ _ =>
        obtain ⟨a, b, hop⟩ := hop
        refine Stage.le_svc hφ _ (hc.2 fun hcomp => ?_)
        have h₁ := hcomp o c a b hop
        simp [Frame.handles, h₁] at hhG
      | errVal hv' => exact (not_value_plug hnv _ hv').elim

end Flick
