import Formal.Equiv.CatchUp

/-!
# Contextual equivalence: the right side reaches a value

When the left side holds a value, a raised error or a signal, the right side reaches the same
form by steps that touch only its own cells (`Sim.vcu`, `Sim.rcu`, `Sim.ucu`): it does the work a
rewrite the other way round put there, and leaves the frames of its own cells.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem Sim.top_const {x : Pos} {c : Const} {b : Tm} (h : Sim W x (.const c) b) (hx : x = .top) :
    b = .const c := by
  cases h with
  | const => rfl
  | jR hx' => exact (hx' hx).elim
  | _ => cases hx

/-- **The right side reaches a value** related to a value of the left side. -/
theorem Sim.vcu {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) (ha : Value a) :
    ∀ (W' : World) (μ : List Tm), W.le W' →
      ∃ W'' μ' b', RJ W' μ b W'' μ' b' ∧ Value b' ∧ Sim W'' .any a b' := by
  induction h with
  | @app x f f' a₀ a₀' hx' h₁ h₂ ih₁ ih₂ =>
    intro W' μ hW
    subst hx
    have hfa : Value f ∧ Value a₀ := by cases ha with
      | ctorApp hf _ he => exact ⟨hf, he⟩
      | constApp _ he => exact ⟨.const, he⟩
    obtain ⟨W₁, μ₁, f₁, r₁, v₁, s₁⟩ := ih₁ rfl hfa.1 W' μ hW
    obtain ⟨W₂, μ₂, a₁, r₂, v₂, s₂⟩ := ih₂ rfl hfa.2 W₁ μ₁ (hW.trans r₁.le)
    refine ⟨W₂, μ₂, .app f₁ a₁, (r₁.frame (F := .appL a₀') trivial).trans
      (r₂.frame (F := .appR f₁) v₁), ?_, .app (by decide) (s₁.mono r₂.le) s₂⟩
    cases ha with
    | ctorApp hf hK he => exact .ctorApp v₁ ((s₁.value_any hf v₁ .top).top_head rfl hf hK) v₂
    | constApp hc he =>
      rw [(s₁.value_any .const v₁ .top).top_const rfl]
      exact .constApp hc v₂
  | @rcons x l e e' r r' hx' h₁ h₂ ih₁ ih₂ =>
    intro W' μ hW
    subst hx
    cases ha with
    | rcons he hr =>
      obtain ⟨W₁, μ₁, e₁, r₁, v₁, s₁⟩ := ih₁ rfl he W' μ hW
      obtain ⟨W₂, μ₂, r₂', r₂, v₂, s₂⟩ := ih₂ rfl hr W₁ μ₁ (hW.trans r₁.le)
      exact ⟨W₂, μ₂, .rcons l e₁ r₂', (r₁.frame (F := .rconsL l r') trivial).trans
        (r₂.frame (F := .rconsR l e₁) v₁), .rcons v₁ v₂, .rcons (by decide) (s₁.mono r₂.le) s₂⟩
  | @dropC e c c' b' hb hn h' ih =>
    intro W' μ hW
    subst hb
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ :=
      ih rfl ha (W'.addJ₂ μ.length) (μ ++ [.lazy e]) (hW.trans (World.le_addJ₂ _ _))
    exact ⟨W₁, μ₁, b₁, (RJ.drop μ e c').trans r₁, v₁, s₁⟩
  | @forceC e e' hn h' ih =>
    intro W' μ hW
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ :=
      ih rfl ha (W'.addJ₂ μ.length) _ (hW.trans (World.le_addJ₂ _ _))
    exact ⟨W₁, _, b₁, (RJ.start μ e').trans ((r₁.frame (F := .settle μ.length) trivial).trans
      (RJ.settleV (r₁.le.J₂ _ (.inr rfl)) v₁)), v₁, s₁⟩
  | @jR x n a a' hx' hn h' ih =>
    intro W' μ hW
    subst hx
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ := ih rfl ha W' μ hW
    exact ⟨W₁, _, b₁, (r₁.frame (F := .settle n) trivial).trans
      (RJ.settleV (r₁.le.J₂ _ (hW.J₂ _ hn)) v₁), v₁, s₁⟩
  | dropA | dropB | forceA | jL | bornV | dead | bornA | bornB => cases ha
  | bornS _ _ _ hs => rcases hs with rfl | rfl | rfl | rfl | rfl <;> cases ha
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | _ =>
    intro W' μ hW
    first
      | (cases ha; done)
      | exact ⟨W', μ, _, RJ.refl _ _, by constructor, Sim.mono hW (by constructor <;> assumption)⟩

/-- **The right side reaches a raised error** related to one of the left side. -/
theorem Sim.rcu {x : Pos} {r b : Tm} (h : Sim W x r b) (hx : x = .any) {v : Tm} (hr : r = .raise v)
    (hv : Value v) : ∀ (W' : World) (μ : List Tm), W.le W' →
      ∃ W'' μ' v', RJ W' μ b W'' μ' (.raise v') ∧ Value v' ∧ Sim W'' .any v v' := by
  induction h with
  | raise h' =>
    cases hr; subst hx
    intro W' μ hW
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ := h'.vcu rfl hv W' μ hW
    exact ⟨W₁, μ₁, b₁, r₁.frame (F := .raise) trivial, v₁, s₁⟩
  | @dropC e c c' b' hb hn h' ih =>
    intro W' μ hW
    subst hb
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ :=
      ih rfl hr (W'.addJ₂ μ.length) (μ ++ [.lazy e]) (hW.trans (World.le_addJ₂ _ _))
    exact ⟨W₁, μ₁, b₁, (RJ.drop μ e c').trans r₁, v₁, s₁⟩
  | @forceC e e' hn h' ih =>
    intro W' μ hW
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ :=
      ih rfl hr (W'.addJ₂ μ.length) _ (hW.trans (World.le_addJ₂ _ _))
    exact ⟨W₁, _, b₁, (RJ.start μ e').trans ((r₁.frame (F := .settle μ.length) trivial).trans
      (RJ.settleR (r₁.le.J₂ _ (.inr rfl)) v₁)), v₁, s₁⟩
  | @jR x n a a' hx' hn h' ih =>
    intro W' μ hW
    subst hx
    obtain ⟨W₁, μ₁, b₁, r₁, v₁, s₁⟩ := ih rfl hr W' μ hW
    exact ⟨W₁, _, b₁, (r₁.frame (F := .settle n) trivial).trans
      (RJ.settleR (r₁.le.J₂ _ (hW.J₂ _ hn)) v₁), v₁, s₁⟩
  | bornS _ _ _ hs => subst hr; rcases hs with h | h | h | h | h <;> cases h
  | _ => first | cases hr | cases hx | (rename_i h; exact (h rfl).elim)

/-- **The right side reaches a signal** related to one of the left side. -/
theorem Sim.ucu {x : Pos} {r b : Tm} (h : Sim W x r b) (hx : x = .any) {p ℓ : Nat} {a : Tm}
    (hr : r = .unw p ℓ a) : ∀ (W' : World) (μ : List Tm), W.le W' →
      ∃ W'' μ' ℓ' k a', RJ W' μ b W'' μ' (.unw p ℓ' a') ∧ W''.L ℓ ℓ' k ∧ Sim W'' .any a a' := by
  induction h with
  | unw hl h' =>
    cases hr
    intro W' μ hW
    exact ⟨W', μ, _, _, _, RJ.refl _ _, hW.L _ _ _ hl, h'.mono hW⟩
  | @dropC e c c' b' hb hn h' ih =>
    intro W' μ hW
    subst hb
    obtain ⟨W₁, μ₁, ℓ', k, a', r₁, hl, s₁⟩ :=
      ih rfl hr (W'.addJ₂ μ.length) (μ ++ [.lazy e]) (hW.trans (World.le_addJ₂ _ _))
    exact ⟨W₁, μ₁, ℓ', k, a', (RJ.drop μ e c').trans r₁, hl, s₁⟩
  | @forceC e e' hn h' ih =>
    intro W' μ hW
    obtain ⟨W₁, μ₁, ℓ', k, a', r₁, hl, s₁⟩ :=
      ih rfl hr (W'.addJ₂ μ.length) _ (hW.trans (World.le_addJ₂ _ _))
    exact ⟨W₁, _, ℓ', k, a', (RJ.start μ e').trans ((r₁.frame (F := .settle μ.length) trivial).trans
      (RJ.settleU (r₁.le.J₂ _ (.inr rfl)))), hl, s₁⟩
  | @jR x n a₀ a' hx' hn h' ih =>
    intro W' μ hW
    subst hx
    obtain ⟨W₁, μ₁, ℓ', k, a'', r₁, hl, s₁⟩ := ih rfl hr W' μ hW
    exact ⟨W₁, _, ℓ', k, a'', (r₁.frame (F := .settle n) trivial).trans
      (RJ.settleU (r₁.le.J₂ _ (hW.J₂ _ hn))), hl, s₁⟩
  | bornS _ _ _ hs => subst hr; rcases hs with h | h | h | h | h <;> cases h
  | _ => first | cases hr | cases hx | (rename_i h; exact (h rfl).elim)

end Flick.Equiv
