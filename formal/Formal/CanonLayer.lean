import Formal.CanonEnv

/-!
# The lemma about `build` (core.md §3.6)

If `v : Layer rᵢ εₗ ρₒ` is a value, `build(v)` exists and `build(v) : Env ρₒ ! R⟨rᵢ; εₗ⟩`.
-/

namespace Flick

variable {S : Sig}

/-- What `build` gives for a layer value. -/
def Builds (S : Sig) (Ψ : List Ty) (Θ : Facts) (Γ : List Scheme) (v : Tm) : Prop :=
  ∀ {i e o φ}, Typ S Ψ Θ Γ v (.ex (.layer i e o) φ) →
    ∃ b, v.build = some b ∧ Typ S Ψ Θ Γ b (.ex (.env o) (.rt i e))

theorem builds_one {Ψ Θ Γ} (c : Const) {w} (hc : c.arity = 2 ∨ c.layer) (hw : Value w) :
    Builds S Ψ Θ Γ (.app (.const c) w) := by
  intro i e o φ h
  cases h with
  | app hf hw' _ hT =>
    cases hf with
    | const _ hT' =>
      cases c with
      | refMake | refGet | ask | open_ | op | perform _ _ => simp [Const.arity, Const.layer] at hc
      | refSet | merge | addFin | spawn | intEq =>
        obtain ⟨_, _, _, _, h, _, _, lt, -⟩ := hT'.arrow_left
        cases h; have := (lt.trans hT).kind; simp [Ty.kind] at this
      | succeed N =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases lt; cases hT
        exact ⟨_, rfl, .econs hw .enil ((hw'.valueAny hw _).sub ⟨la, Stage.le_refl _⟩)
          (.enil fun _ h => h) fun _ => ⟨.inl, fun h => h.resolve_right id⟩⟩
      | effect N =>
        obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
        cases h; cases lt; cases hT
        exact ⟨_, rfl, .bind (.await ((hw'.valueAny hw _).sub ⟨la, Stage.le_refl _⟩)
          (Stage.le_refl _) .refl) fun _ => Iff.rfl⟩

theorem builds_two {Ψ Θ Γ} (c : Const) {w₁ w₂} (h₁ : c.arity = 2) (h₂ : c.layer)
    (ih₁ : Builds S Ψ Θ Γ w₁) (ih₂ : Builds S Ψ Θ Γ w₂) :
    Builds S Ψ Θ Γ (.app (.app (.const c) w₁) w₂) := by
  intro i e o φ h
  cases c <;> simp [Const.arity, Const.layer] at h₁ h₂
  cases h with
  | app hf hw₂' _ hT =>
    cases hf with
    | app hc hw₁' _ hT' =>
      cases hc with
      | const _ hT'' =>
        obtain ⟨_, _, _, _, h, la₁, _, lt, -⟩ := hT''.arrow_left
        cases h; cases la₁
        obtain ⟨_, _, _, _, h, la₂, _, lt', -⟩ := (lt.trans hT').arrow_left
        cases h; cases la₂; cases lt'; cases hT
        obtain ⟨b₁, hb₁, t₁⟩ := ih₁ hw₁'
        obtain ⟨b₂, hb₂, t₂⟩ := ih₂ hw₂'
        refine ⟨_, by simp [Tm.build, hb₁, hb₂], .join (t₁.sub ⟨.refl, ?_⟩)
          (t₂.sub ⟨.refl, ?_⟩) fun _ => Iff.rfl⟩
        · exact ⟨fun _ h => .inl h, fun _ h => .inl h⟩
        · exact ⟨fun _ h => .inr h, fun _ h => .inr h⟩

theorem canonical_layer {Ψ Θ Γ v} (hv : Value v) : Builds S Ψ Θ Γ v := by
  induction hv with
  | constApp hc hw _ => exact builds_one _ hc hw
  | constApp2 h₁ h₂ _ _ ih₁ ih₂ => exact builds_two _ h₁ h₂ ih₁ ih₂
  | ctorApp _ hf _ =>
    intro _ _ _ _ h; have hk := h.vkind (.ctorApp ‹_› hf ‹_›) rfl
    simp [VKind, Tm.head, hf, Ty.kind] at hk
  | _ =>
    intro _ _ _ _ h; have hk := h.vkind (by constructor <;> assumption) rfl
    simp [VKind, Tm.head, Tm.args, Const.kindAt, Ty.kind] at hk

end Flick
