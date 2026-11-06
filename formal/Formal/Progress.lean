import Formal.StepFrame

/-!
# Progress (core.md §4)
-/

namespace Flick

variable {S : Sig}

theorem acons_progress {v p b r} (hv : Value v) : ∃ e', PHead (.match_ v (.acons p b r)) e' := by
  cases hm : matchP p v with
  | none => exact ⟨_, .matchNo hv hm⟩
  | some δ => exact ⟨_, .matchYes hv hm⟩

theorem match_progress {Ψ Θ v A t t' φ} (hv : Value v) (he : Typ S Ψ Θ [] v (.ex t φ))
    (hA : Typ S Ψ Θ [] A (.alts t t' φ)) (hc : Covers S t A.unguarded v) :
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
  | aref => exact acons_progress hv

theorem handle_progress {Ψ Θ Λ μ v A ε₀ H t φ} (hv : Value v)
    (hA : Typ S Ψ Θ [] A (.halts ε₀ H t φ)) : ∃ Λ' μ' e', RHead Λ μ (.handle v A) Λ' μ' e' := by
  cases A with
  | anil => exact ⟨_, _, _, .handleNil hv⟩
  | acons p b r =>
    cases hm : matchP p v with
    | none => exact ⟨_, _, _, .handleNo hv hm⟩
    | some δ => exact ⟨_, _, _, .handleYes hv hm⟩
  | aguard p g b r =>
    cases hm : matchP p v with
    | none => exact ⟨_, _, _, .hguardNo hv hm⟩
    | some δ => exact ⟨_, _, _, .hguardYes hv hm⟩
  | _ => cases hA

theorem nt {F : Frame} (h : F.isTry → False) : ¬ F.isTry := h
theorem ni {F : Frame} (h : F.isInside → False) : ¬ F.isInside := h

theorem live_or_dead (Λ : List Rgn) (p : Nat) :
    (∃ g, Λ[p]? = some g ∧ g.live = true) ∨ Dead Λ p := by
  cases hg : Λ[p]? with
  | none => exact .inr fun g h => by rw [hg] at h; cases h
  | some g =>
    cases hl : g.live with
    | true => exact .inl ⟨g, rfl, hl⟩
    | false => exact .inr fun g' h => by rw [hg] at h; cases h; exact hl

/-- Progress of an application of two values. -/
theorem app_progress {Ψ Θ Λ μ root f e a ψ t C φ} (hμ : μ.length = Ψ.length) (hvf : Value f)
    (hve : Value e) (hf : Typ S Ψ Θ [] f (.ex (.arrow a ψ t C) φ)) (he : Typ S Ψ Θ [] e (.ex a φ))
    (hψ : ψ.le φ) : Value (.app f e) ∨ NV root Λ μ (.app f e) φ.svc := by
  have step : ∀ {l o}, Step root l Λ μ (.app f e) o → Value (.app f e) ∨ NV root Λ μ (.app f e) φ.svc :=
    fun h => .inr (.inr (.inl ⟨_, _, h⟩))
  have blocked : Blocks Λ (.app f e) → Value (.app f e) ∨ NV root Λ μ (.app f e) φ.svc :=
    fun h => .inr (.inr (.inr (.inr (.inl ⟨[], _, OkE.nil, rfl, h⟩))))
  rcases canonical_arrow hvf hf with ⟨b, rfl⟩ | ⟨g, rfl⟩ | ⟨K, hK⟩ | ⟨c, rfl⟩ |
      ⟨c, w, rfl, hc, hw⟩
  · exact step (Step.ofP (.beta hve))
  · exact step (Step.ofP (.fix hve))
  · exact .inl (.ctorApp hvf hK hve)
  · cases hf with
    | const _ hT' =>
      cases c with
      | perform E o => simp_all [Const.ok]
      | refMake => exact step (Step.ofR (.refMake hve))
      | refGet =>
        obtain ⟨_, _, _, _, h, la, _, _, -⟩ := hT'.arrow_left
        cases h; cases la
        obtain ⟨l, rfl, hl⟩ := canonical_ref hve he
        exact step (Step.ofR (.refGet (List.getElem?_eq_getElem (hμ ▸ getElem?_lt hl))))
      | refSet => exact .inl (.constApp (.inl rfl) hve)
      | ask N =>
        obtain ⟨_, _, _, _, h, la, hψ', _, -⟩ := hT'.arrow_left
        cases h; cases la
        obtain rfl := canonical_unit hve he
        cases hr : root.svc N with
        | some w => exact step (Step.askRoot (E := []) OkE.nil rfl hr)
        | none =>
          exact .inr (.inr (.inr (.inl ⟨[], N, OkE.nil, rfl, rfl, hr,
            Stage.le_svc (Stage.le_trans hψ' hψ) _ rfl⟩)))
      | succeed => exact .inl (.constApp (.inr trivial) hve)
      | effect => exact .inl (.constApp (.inr trivial) hve)
      | merge => exact .inl (.constApp (.inl rfl) hve)
      | addFin => exact .inl (.constApp (.inl rfl) hve)
      | spawn => exact .inl (.constApp (.inl rfl) hve)
      | intEq => exact .inl (.constApp (.inl rfl) hve)
      | open_ =>
        obtain ⟨_, _, _, _, h, la, _, _, -⟩ := hT'.arrow_left
        cases h; cases la
        obtain ⟨p, rfl, -⟩ := canonical_owner hve he
        rcases live_or_dead Λ p with ⟨g, hg, hl⟩ | hd
        · exact step (Step.ofR (.open_ hg hl))
        · exact blocked (.inr ⟨p, hd, .inl rfl⟩)
      | op =>
        obtain ⟨_, _, _, _, h, la, _, _, -⟩ := hT'.arrow_left
        cases h; cases la
        obtain ⟨p, rfl, -⟩ := canonical_hnd hve he
        rcases live_or_dead Λ p with ⟨g, hg, hl⟩ | hd
        · exact step (Step.ofR (.op hg hl))
        · exact blocked (.inr ⟨p, hd, .inr (.inl rfl)⟩)
  · cases c with
    | refSet =>
      cases hf with
      | app hc' hw' _ _ =>
        cases hc' with
        | const _ hT' =>
          obtain ⟨_, _, _, _, h, la, _, _, -⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨l, rfl, hl⟩ := canonical_ref hw hw'
          exact step (Step.ofR (.refSet hve (hμ ▸ getElem?_lt hl)))
    | merge => exact .inl (.constApp2 rfl trivial hw hve)
    | addFin =>
      cases hf with
      | app hc' hw' _ _ =>
        cases hc' with
        | const _ hT' =>
          obtain ⟨_, _, _, _, h, la, _, _, -⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨p, rfl, -⟩ := canonical_owner hw hw'
          rcases live_or_dead Λ p with ⟨g, hg, hl⟩ | hd
          · exact step (Step.addFin (E := []) OkE.nil hg hl hve)
          · exact blocked (.inr ⟨p, hd, .inr (.inr (.inl ⟨_, rfl⟩))⟩)
    | spawn =>
      cases hf with
      | app hc' hw' _ _ =>
        cases hc' with
        | const _ hT' =>
          obtain ⟨_, _, _, _, h, la, _, _, -⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨p, rfl, -⟩ := canonical_owner hw hw'
          rcases live_or_dead Λ p with ⟨g, hg, hl⟩ | hd
          · exact step (Step.spawn (E := []) OkE.nil hg hl hve)
          · exact blocked (.inr ⟨p, hd, .inr (.inr (.inr (.inl ⟨_, rfl⟩)))⟩)
    | intEq =>
      cases hf with
      | app hc' hw' _ hT =>
        cases hc' with
        | const _ hT' =>
          obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
          cases h; cases la
          obtain ⟨_, _, _, _, h', la', _, _, -⟩ := (lt.trans hT).arrow_left
          cases h'; cases la'
          obtain ⟨m, rfl⟩ := canonical_int hw hw'
          obtain ⟨n, rfl⟩ := canonical_int hve he
          exact step (Step.ofP .intEq)
    | _ => simp [Const.arity] at hc

end Flick
