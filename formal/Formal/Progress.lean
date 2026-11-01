import Formal.StepFrame

/-!
# Progress (core.md §4)
-/

namespace Flick

variable {S : Sig}

theorem match_progress {Ψ v A t t' φ} (hv : Value v) (he : Typ S Ψ [] v (.ex t φ))
    (hA : Typ S Ψ [] A (.alts t t' φ)) (hc : Covers S t A.unguarded v) :
    ∃ e', PHead (.match_ v A) e' := by
  cases hA with
  | anil =>
    exfalso
    rcases hc with hx | ⟨_, _, hp, _⟩
    · obtain ⟨_, hp, _⟩ := hx v (he.shape hv rfl).1; simp [Tm.unguarded] at hp
    · simp [Tm.unguarded] at hp
  | @acons p _ _ _ _ _ _ _ =>
    cases hm : matchP p v with
    | none => exact ⟨_, .matchNo hv hm⟩
    | some δ => exact ⟨_, .matchYes hv hm⟩
  | @aguard p _ _ _ _ _ _ _ _ =>
    cases hm : matchP p v with
    | none => exact ⟨_, .guardNo hv hm⟩
    | some δ => exact ⟨_, .guardYes hv hm⟩

theorem handle_progress {Ψ μ v A ε₀ H t φ} (hv : Value v)
    (hA : Typ S Ψ [] A (.halts ε₀ H t φ)) : ∃ μ' e', RHead μ (.handle v A) μ' e' := by
  cases A with
  | anil => exact ⟨_, _, .handleNil hv⟩
  | acons p b r =>
    cases hm : matchP p v with
    | none => exact ⟨_, _, .handleNo hv hm⟩
    | some δ => exact ⟨_, _, .handleYes hv hm⟩
  | aguard p g b r =>
    cases hm : matchP p v with
    | none => exact ⟨_, _, .hguardNo hv hm⟩
    | some δ => exact ⟨_, _, .hguardYes hv hm⟩
  | _ => cases hA

theorem nt {F : Frame} (h : F.isTry → False) : ¬ F.isTry := h

/-- Progress of an application of two values. -/
theorem app_progress {Ψ μ root f e a ψ t φ} (hμ : StoreTy S Ψ μ) (hvf : Value f) (hve : Value e)
    (hf : Typ S Ψ [] f (.ex (.arrow a ψ t) φ)) (he : Typ S Ψ [] e (.ex a φ)) (hψ : ψ.le φ) :
    Value (.app f e) ∨ NV root μ (.app f e) φ.svc := by
  have step : ∀ {l o}, Step root l μ (.app f e) o → Value (.app f e) ∨ NV root μ (.app f e) φ.svc :=
    fun h => .inr (.inr (.inl ⟨_, _, h⟩))
  rcases canonical_arrow hvf hf with ⟨b, rfl⟩ | ⟨g, rfl⟩ | ⟨K, hK⟩ | ⟨c, rfl⟩ |
      ⟨c, w, rfl, hc, hw⟩
  · exact step (Step.ofP (.beta hve))
  · exact step (Step.ofP (.fix hve))
  · exact .inl (.ctorApp hvf hK hve)
  · cases hf with
    | const _ hT' =>
      cases c with
      | refMake => exact step (Step.ofR (.refMake hve))
      | refGet =>
        obtain ⟨_, _, _, h, la, _, _⟩ := hT'.arrow_left
        cases h; cases la
        obtain ⟨l, rfl, hl⟩ := canonical_ref hve he
        exact step (Step.ofR (.refGet (List.getElem?_eq_getElem (hμ.1 ▸ getElem?_lt hl))))
      | refSet => exact .inl (.constApp (.inl rfl) hve)
      | ask N =>
        obtain ⟨_, _, _, h, la, hψ', _⟩ := hT'.arrow_left
        cases h; cases la
        obtain rfl := canonical_unit hve he
        cases hr : root N with
        | some w => exact step (Step.askRoot (E := []) OkE.nil rfl hr)
        | none =>
          exact .inr (.inr (.inr ⟨[], N, OkE.nil, rfl, rfl, hr,
            Stage.le_svc (Stage.le_trans hψ' hψ) N rfl⟩))
      | succeed => exact .inl (.constApp (.inr trivial) hve)
      | effect => exact .inl (.constApp (.inr trivial) hve)
      | merge => exact .inl (.constApp (.inl rfl) hve)
  · cases c with
    | refSet =>
      cases hf with
      | app hc' hw' _ _ =>
        cases hc' with
        | const _ hT' =>
          obtain ⟨_, _, _, h, la, _, _⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨l, rfl, hl⟩ := canonical_ref hw hw'
          exact step (Step.ofR (.refSet hve (hμ.1 ▸ getElem?_lt hl)))
    | merge => exact .inl (.constApp2 rfl trivial hw hve)
    | _ => simp [Const.arity] at hc

end Flick
