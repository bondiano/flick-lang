import Formal.Equiv.Cfg

/-!
# Contextual equivalence: typing keeps lazy cells away from patterns

Where a pattern reads a term without running it, typing gives the term the pure stage (a
field of a dictionary, the value of a handler), or no term judgment at all (a list of
alternatives or of arms): `PureJ`. A term that makes or forces a lazy cell has the stage `rt` at
least (`Typ.lzShape`). So a typed term has none at such a place (`Typ.noLzTop`), but in the body
of an arm, which typing does not reach when the arm is a dead one of a GADT.
-/

namespace Flick.Equiv

variable {S : Sig}

/-- A judgment of a place a pattern reads. -/
def PureJ : J → Prop
  | .ex _ φ => φ = .pure
  | _ => True

theorem not_value_lazy {e : Tm} : ¬ Value (.lazy e) := fun h => by cases h

/-- A judgment of a term with the stage `rt` at least. -/
def RtJ (J : J) : Prop := ∃ t φ, J = .ex t φ ∧ (Stage.rt Row.none Row.none).le φ

theorem rt_not_pure : ¬ (Stage.rt Row.none Row.none).le .pure := fun h => h

theorem RtJ.not_pure {J : J} (h : RtJ J) : ¬ PureJ J := by
  obtain ⟨_, _, rfl, hφ⟩ := h
  intro hp; cases hp; exact rt_not_pure hφ

theorem _root_.Flick.Typ.lazy_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {a}, e = .lazy a → RtJ J := by
  induction h with
  | lazy _ _ _ hφ => intro _ _; exact ⟨_, _, rfl, hφ⟩
  | allI hv => intro _ he; subst he; exact (not_value_lazy hv).elim
  | errVal _ _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | conv _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | _ => intro _ he; cases he

theorem _root_.Flick.Typ.let_lazy_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {a b}, e = .let_ (.lazy a) b → RtJ J := by
  induction h with
  | let_ h₁ _ _ _ =>
    intro _ _ he; cases he
    obtain ⟨_, _, he', hφ⟩ := h₁.lazy_stage rfl
    cases he'; exact ⟨_, _, rfl, hφ⟩
  | letGen h₁ _ _ _ =>
    intro _ _ he; cases he
    obtain ⟨_, _, he', hφ⟩ := h₁.lazy_stage rfl
    cases he'; exact (rt_not_pure hφ).elim
  | allI hv => intro _ _ he; subst he; cases hv
  | errVal _ _ _ ih =>
    intro _ _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | conv _ _ ih =>
    intro _ _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | _ => intro _ _ he; cases he

theorem _root_.Flick.Typ.force_lazy_stage {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {a}, e = .force (.lazy a) → RtJ J := by
  induction h with
  | force h₁ _ _ _ =>
    intro _ he; cases he
    obtain ⟨_, _, he', hφ⟩ := h₁.lazy_stage rfl
    cases he'; exact ⟨_, _, rfl, hφ⟩
  | allI hv => intro _ he; subst he; cases hv
  | errVal _ _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | conv _ _ ih =>
    intro _ he
    obtain ⟨_, _, he', hφ⟩ := ih he
    cases he'
    exact ⟨_, _, rfl, hφ⟩
  | _ => intro _ he; cases he

/-- **A term that makes or forces a lazy cell has the stage `rt` at least.** -/
theorem _root_.Flick.Typ.lzShape {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) (hs : e.LzShape) :
    ¬ PureJ J := by
  match e, hs with
  | .lazy _, _ => exact (h.lazy_stage rfl).not_pure
  | .let_ (.lazy _) _, _ => exact (h.let_lazy_stage rfl).not_pure
  | .force (.lazy _), _ => exact (h.force_lazy_stage rfl).not_pure

theorem noLzTop_aux {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Typ S Ψ Θ Γ e J → ∀ {x : Pos}, (x = .top → PureJ J) → ¬ e.LzTop x := by
  induction h with
  | allI _ h₁ ih => exact fun _ {_} _ => ih h₁ (fun _ => rfl)
  | conv h₁ _ ih => exact fun _ {_} hx => ih h₁ hx
  | errVal h₁ _ _ ih => exact fun _ {_} hx => ih h₁ hx
  | _ =>
    intro hty x hx hz
    cases hz
    all_goals first
      | (rename_i hs; exact Typ.lzShape hty hs (hx rfl))
      | (apply absurd ‹Tm.LzTop _ _›; apply_assumption <;>
         first | assumption | (intro h; cases h; done) | (intro _; trivial) | (intro _; rfl) | exact hx)

/-- **A typed term has no lazy cell where a pattern reads**, outside the bodies of arms. -/
theorem _root_.Flick.Typ.noLzTop {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) {x : Pos}
    (hx : x = .top → PureJ J) : ¬ e.LzTop x :=
  noLzTop_aux h h hx

end Flick.Equiv
