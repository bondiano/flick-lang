import Formal.Progress

/-!
# Typing through evaluation contexts
-/

namespace Flick

variable {S : Sig}

theorem not_value_plug {r : Tm} (hr : ¬ Value r) (F : Frame) : ¬ Value (F.plug r) := by
  intro hv
  cases F <;> simp only [Frame.plug] at hv
  case appL => cases hv with
    | ctorApp hf _ _ => exact hr hf
    | constApp _ _ => exact hr .const
  case appR => cases hv with
    | ctorApp _ _ he => exact hr he
    | constApp _ he => exact hr he
  case rconsL => cases hv with | rcons he _ => exact hr he
  case rconsR => cases hv with | rcons _ he => exact hr he
  all_goals cases hv

theorem not_value_plugs {r : Tm} (hr : ¬ Value r) : ∀ E : List Frame, ¬ Value (plug E r)
  | [] => hr
  | F :: E => not_value_plugs (r := F.plug r) (not_value_plug hr F) E

theorem Head.not_value {μ e μ' e'} (h : Head μ e μ' e') : ¬ Value e := by
  intro hv
  cases h <;> cases hv
  all_goals first
    | (rename_i hf _; simp [Tm.head] at hf)
    | (rename_i hc _; simp [Const.arity] at hc)
    | (rename_i hf; simp [Tm.head] at hf)

/-- The plugged term after a reduction of the hole, in a store grown by `X`. -/
def Refill (S : Sig) (Ψ : List Ty) (t' : Ty) (C : Tm → Tm) (t : Ty) : Prop :=
  ∀ X r', Typ S (Ψ ++ X) [] r' (.ex t') → Typ S (Ψ ++ X) [] (C r') (.ex t)

theorem frame_typing {Ψ r t} (hr : ¬ Value r) :
    ∀ F : Frame, Typ S Ψ [] (F.plug r) (.ex t) →
      ∃ t', Typ S Ψ [] r (.ex t') ∧ Refill S Ψ t' F.plug t
  | .appL _, h => by
    cases h with
    | app h₁ h₂ => exact ⟨_, h₁, fun X _ h' => .app h' (h₂.weakenStore X)⟩
  | .appR _, h => by
    cases h with
    | app h₁ h₂ => exact ⟨_, h₂, fun X _ h' => .app (h₁.weakenStore X) h'⟩
  | .let_ _, h => by
    cases h with
    | let_ h₁ h₂ => exact ⟨_, h₁, fun X _ h' => .let_ h' (h₂.weakenStore X)⟩
  | .match_ _, h => by
    cases h with
    | match_ h₁ h₂ hc =>
      have hx : Exh S _ _ := hc.resolve_right fun ⟨hv, _⟩ => hr hv
      exact ⟨_, h₁, fun X _ h' => .match_ h' (h₂.weakenStore X) (.inl hx)⟩
  | .rconsL _ _, h => by
    cases h with
    | rcons hl h₁ h₂ => exact ⟨_, h₁, fun X _ h' => .rcons hl h' (h₂.weakenStore X)⟩
  | .rconsR _ _, h => by
    cases h with
    | rcons hl h₁ h₂ => exact ⟨_, h₂, fun X _ h' => .rcons hl (h₁.weakenStore X) h'⟩
  | .proj _, h => by
    cases h with
    | proj h₁ hR => exact ⟨_, h₁, fun _ _ h' => .proj h' hR⟩
  | .updL _ _, h => by
    cases h with
    | update h₁ hR h₂ => exact ⟨_, h₁, fun X _ h' => .update h' hR (h₂.weakenStore X)⟩
  | .updR _ _, h => by
    cases h with
    | update h₁ hR h₂ => exact ⟨_, h₂, fun X _ h' => .update (h₁.weakenStore X) hR h'⟩

/-- Л3 without effects: the hole of a well-typed context is well-typed, and any
term of its type can replace it. -/
theorem ctx_typing {Ψ} :
    ∀ (E : List Frame) {r t}, ¬ Value r → Typ S Ψ [] (plug E r) (.ex t) →
      ∃ t', Typ S Ψ [] r (.ex t') ∧ Refill S Ψ t' (plug E) t
  | [], _, _, _, h => ⟨_, h, fun _ _ h' => h'⟩
  | F :: E, r, _, hr, h => by
    obtain ⟨t₁, h₁, back₁⟩ := ctx_typing E (not_value_plug hr F) h
    obtain ⟨t', h', backF⟩ := frame_typing hr F h₁
    exact ⟨t', h', fun X r' hr' => back₁ X (F.plug r') (backF X r' hr')⟩

end Flick
