import Formal.Regions

/-!
# A use of a service of a `provide` reaches its region

An `ask_S` answered by a `with` frame of the `provide` region `q` records the use
`need S q` in the row outside the frame (`Row.withP`); the use passes every frame
but the `inside` form of `q`, so it reaches the top of the thread or that form.
-/

namespace Flick

variable {S : Sig}

/-- A typed environment provides exactly the services of its row. -/
theorem env_dom {Ψ Θ Γ N} {θ : Tm} (hθ : IsEnv θ) :
    ∀ {ρ φ}, Typ S Ψ Θ Γ θ (.ex (.env ρ) φ) → ((θ.envLookup N).isSome ↔ ρ (.nm N)) := by
  induction hθ with
  | enil => intro _ _ h; cases h with | enil hρ => simp [Tm.envLookup, hρ]
  | @econs v θ N' _ _ ih =>
    intro _ _ h
    cases h with
    | econs _ _ _ hθ' hρ =>
      rw [hρ]
      by_cases he : N' = N
      · subst he; simp [Tm.envLookup]
      · simp only [Tm.envLookup, he, if_false]
        rw [ih hθ']
        constructor
        · exact .inr
        · rintro (h | h)
          · cases h; exact (he rfl).elim
          · exact h

theorem need_out {N : Name} {r : Tm}
    (hask : ∀ {Ψ' Θ' t' φ'}, Typ S Ψ' Θ' [] r (.ex t' φ') → φ'.svc (.nm N)) (hv : ¬ Value r)
    (hsvc : ∀ o, S.op N o = none) :
    ∀ (E : List Frame) {Ψ : List Ty} {Θ : Facts} {t : Ty} {φ : Stage}, OkE E →
      Typ S Ψ Θ [] (plug E r) (.ex t φ) →
      (∀ q v, lookupE E N = some (q, v) → φ.svc (.need N (.r q)) ∨ .inside q ∈ E) ∧
        (lookupE E N = none → φ.svc (.nm N)) := by
  refine list_rev_ind ?_ ?_
  · intro Ψ Θ t φ _ h
    exact ⟨fun q v h' => by simp [lookupE] at h', fun _ => hask h⟩
  · intro E₀ G ih Ψ Θ t φ hE h
    obtain ⟨hE₀, hG⟩ := OkE.of_append hE
    rw [plug_append] at h
    simp only [plug] at h
    have hnv := not_value_plugs hv E₀
    have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
    have look := lookupE_append E₀ G N
    cases G with
    | with_ q' θ =>
      cases h with
      | @with_ _ _ _ _ ρ _ _ φ₀ _ r_out ε _ hθv hθ hb hrow hφ =>
        obtain ⟨h1, h2⟩ := ih hE₀ hb
        have up : ∀ y, r_out y → φ.svc y := fun y h => Stage.le_svc hφ _ h
        have dom := fun N' => env_dom (N := N') (canonical_env hθv hθ) hθ
        rw [look]
        cases hl : lookupE E₀ N with
        | some w =>
          refine ⟨fun q v hqv => ?_, fun h => by simp at h⟩
          simp only [Option.some.injEq] at hqv; subst hqv
          rcases h1 _ _ hl with h' | h'
          · exact .inl (up _ (Stage.le_svc hrow _ h'))
          · exact .inr (mem h')
        | none =>
          have hn := h2 hl
          simp only
          cases hf : (Frame.with_ q' θ).lookup N with
          | some w =>
            refine ⟨fun q v hqv => ?_, fun h => by simp at h⟩
            simp only [Option.some.injEq] at hqv; subst hqv
            simp only [Frame.lookup, Option.map_eq_some_iff] at hf
            obtain ⟨w', hw', hq⟩ := hf
            cases hq
            exact .inl (up _ ((Stage.le_svc hrow _ hn).1 ((dom N).mp (by simp [hw']))))
          | none =>
            refine ⟨fun q v hqv => by simp at hqv, fun _ => ?_⟩
            simp only [Frame.lookup, Option.map_eq_none_iff] at hf
            exact up _ ((Stage.le_svc hrow _ hn).2 fun h => by
              have := (dom N).mpr h; simp [hf] at this)
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | inside p =>
      cases h with
      | inside _ hb hrow _ _ hφ _ =>
        obtain ⟨h1, h2⟩ := ih hE₀ hb
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        rw [look]
        cases hl : lookupE E₀ N with
        | some w =>
          refine ⟨fun q v hqv => ?_, fun h => by simp at h⟩
          simp only [Option.some.injEq] at hqv; subst hqv
          rcases h1 _ _ hl with h' | h'
          · rcases Stage.le_svc hrow _ h' with h₁ | h₁ | h₁ | h₁
            · exact h₁.elim
            · exact .inl (up _ h₁)
            · exact h₁.elim
            · cases h₁; exact .inr (by simp)
          · exact .inr (mem h')
        | none =>
          refine ⟨fun q v hqv => by simp [Frame.lookup] at hqv, fun _ => ?_⟩
          rcases Stage.le_svc hrow _ (h2 hl) with h₁ | h₁ | h₁ | h₁
          · exact h₁.elim
          · exact up _ h₁
          · exact h₁.elim
          · exact h₁.elim
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | _ =>
      obtain ⟨k, Θ', t₁, φ₁, h₁, rel, hk, -, -⟩ := frame_typing hnv _ hG h
      have up : ∀ y, (∀ M, y = .nm M → M = N) → φ₁.svc y → φ.svc y := fun y hy h' =>
        (rel.svc _ h').resolve_right fun h₂ => by
          cases y with
          | nm M =>
            rcases h₂ with h₂ | h₂
            · simp [Frame.lookup] at h₂
            · obtain ⟨o, _, _, _, hop⟩ := installs_op h h₂
              rw [hy M rfl, hsvc o] at hop; cases hop
          | _ => simp [Frame.dom, Frame.lookup] at h₂
      obtain ⟨h1, h2⟩ := ih hE₀ h₁
      rw [look]
      cases hl : lookupE E₀ N with
      | some w =>
        refine ⟨fun q v hqv => ?_, fun h => by simp at h⟩
        simp only [Option.some.injEq] at hqv; subst hqv
        exact (h1 _ _ hl).imp (up _ fun _ h => by cases h) mem
      | none =>
        exact ⟨fun q v hqv => by simp [Frame.lookup] at hqv, fun _ => up _ (fun _ h => by cases h; rfl) (h2 hl)⟩

end Flick
