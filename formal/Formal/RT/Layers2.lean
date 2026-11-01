import Formal.RT.Layers

/-! Preservation of `Inv6`. -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

theorem erase_nil_mem {l : List Nat} {i j : Nat} (h : l.erase i = []) (hj : j ∈ l) : j = i :=
  Classical.byContradiction fun hne => by
    have := (List.mem_erase_of_ne hne).2 hj; rw [h] at this; cases this

/-- The observer `i` that (C5) of `π` starts is the one its `obs` event names. -/
theorem obs_owner (I : Inv C) (I6 : Inv6 C) {π π' m Ly σ i : Nat} {o : Owner T V}
    {a : FinAct T} {fs : List (Fin T)} (ho : C.Λ π = some o) (hfs : o.fs = ⟨i, a⟩ :: fs)
    (hob : .obs π' m Ly σ i ∈ C.J) : π' = π :=
  I.fin_unique (I6.obsFin _ _ _ _ _ hob) (I.finIn π o ⟨i, a⟩ ho (by simp [hfs])).1

set_option maxHeartbeats 1000000 in
theorem step_hist6 [DecidableEq V] (h : Hyp L) (I : Inv C) (I6 : Inv6 C) (hs : Step K L C C') :
    Hist Good6 C'.J := by
  have hJ := I6.hist
  cases hs <;> simp only [hist_cons, Good6, hJ, and_true, true_and]
  case provBuild f φ πp m Ly c Ls b hf hc hlk hsome =>
    intro σ' hb
    rcases I6.live m Ly σ' hb with hr | ⟨mm, e, hm, he, -⟩
    · exact hr
    · rw [lookup_none (hlk h.g7d) (Nat.lt_of_le_of_lt (Nat.zero_le _) (I.ltM hm)) hm] at he
      cases he
  case provHit f φ πp m₀ Ly₀ c Ls b m' e₀ o mm₀ hf hc hmemo hlk hb ho hst hm₀ =>
    obtain ⟨mm₁, hm₁, he₁⟩ := lookup_lt hlk
    exact I6.noRel _ _ _ _ hm₁ he₁
  case buildOk f φ πp m₀ Ly₀ σ₀ Ls b s v o mm₀ e₀ hf hc hs ho hst hm₀ he₀ =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    exact I6.noRel _ _ _ _ hm₁ he₁
  case finObsLast f φ π s o i m Ly fs mm e oσ hf hc hs ho hph hfs hm he hobs hne hσ hσo hst hb =>
    intro π' i' hob
    simp only [List.mem_cons, reduceCtorEq, false_or] at hob ⊢
    rcases I6.obsIn m mm Ly e hm he π' i' hob with hi | hr
    · have := erase_nil_mem hobs hi; subst this
      exact .inl (by rw [obs_owner I I6 ho hfs hob])
    · exact .inr hr

set_option maxHeartbeats 1000000 in
theorem step_live6 [DecidableEq V] (h : Hyp L) (I : Inv C) (I6 : Inv6 C) (hs : Step K L C C') :
    ∀ m Ly σ, .build m Ly σ ∈ C'.J → .release m Ly σ ∈ C'.J ∨
      ∃ mm e, C'.M m = some mm ∧ mm.ents Ly = some e ∧ e.σ = σ := by
  have hl := I6.live
  have hsub : ∀ (J' : List (Ev V)), (∀ e ∈ C.J, e ∈ J') → ∀ m Ly σ, .build m Ly σ ∈ C.J →
      .release m Ly σ ∈ J' ∨ ∃ mm e, C.M m = some mm ∧ mm.ents Ly = some e ∧ e.σ = σ :=
    fun J' hJ m Ly σ hb => (hl m Ly σ hb).imp (hJ _) id
  cases hs
  case provide =>
    obtain ⟨-, -, -, -, hMn⟩ := I.fresh (C.n + 1) (by omega)
    intro m Ly σ hb
    rcases hl m Ly σ hb with hr | ⟨mm, e, hm, he, hσ⟩
    · exact .inl hr
    · have : m ≠ C.n + 1 := fun e => by subst e; rw [hMn] at hm; cases hm
      exact .inr ⟨mm, e, by simp [upd, this, hm], he, hσ⟩
  case provHit f φ πp m₀ Ly₀ c Ls b m' e₀ o mm₀ hf hc hmemo hlk hb ho hst hm₀ =>
    obtain ⟨mm₁, hm₁, he₁⟩ := lookup_lt hlk
    exact live_setEnt rfl (fun mm e hm he => by rw [hm₁] at hm; cases hm; rw [he₁] at he; cases he; rfl)
      (fun e he => by simp [he]) (fun _ _ _ h => by simpa using h) hl
  case finObs f φ π s o i m₀ Ly₀ fs mm₀ e₀ hf hc hs ho hph hfs hm₀ he₀ hobs hst =>
    exact live_setEnt rfl (fun mm e hm he => by rw [hm₀] at hm; cases hm; rw [he₀] at he; cases he; rfl)
      (fun e he => by simp [he]) (fun _ _ _ h => by simpa using h) hl
  case buildOk f φ πp m₀ Ly₀ σ₀ Ls b s v o mm₀ e₀ hf hc hs ho hst hm₀ he₀ =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    exact live_setEnt rfl (fun mm e hm he => by rw [hm₁] at hm; cases hm; rw [he₁] at he; cases he; rfl)
      (fun e he => by simp [he]) (fun _ _ _ h => by simpa using h) hl
  case buildFail f φ πp m₀ Ly₀ σ₀ Ls b s c oσ hf hc hs hσ hσo =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    exact live_setEnt rfl (fun mm e hm he => by rw [hm₁] at hm; cases hm; rw [he₁] at he; cases he; rfl)
      (fun e he => by simp [he]) (fun _ _ _ h => by simpa using h) hl
  case provBuild f φ πp m₀ Ly₀ c Ls b hf hc hlk hsome =>
    obtain ⟨mm₀, hm₀⟩ := Option.isSome_iff_exists.1 hsome
    intro m Ly σ hb
    simp only [List.mem_cons, Ev.build.injEq, reduceCtorEq, false_or] at hb ⊢
    rcases hb with ⟨rfl, rfl, rfl⟩ | hb
    · obtain ⟨mm', h1, h2⟩ := setEnt_here (Ly₀ := Ly) (e₀ := ⟨.building, C.n, []⟩) hm₀
      exact .inr ⟨mm', _, h1, h2, rfl⟩
    · rcases hl m Ly σ hb with hr | ⟨mm, e, hm, he, hσ⟩
      · exact .inl hr
      · by_cases hk : m = m₀ ∧ Ly = Ly₀
        · obtain ⟨rfl, rfl⟩ := hk; rw [hm₀] at hm; cases hm
          rw [lookup_none (hlk h.g7d) (Nat.lt_of_le_of_lt (Nat.zero_le _) (I.ltM hm₀)) hm₀] at he
          cases he
        · obtain ⟨mm', h1, h2⟩ := setEnt_keep (e₀ := some ⟨.building, C.n, []⟩) hm he hk
          exact .inr ⟨mm', e, h1, h2, hσ⟩
  case finObsLast f φ π s o i m₀ Ly₀ fs mm₀ e₀ oσ hf hc hs ho hph hfs hm₀ he₀ hobs hne hσ hσo hst hb =>
    intro m Ly σ hbu
    have hbu' : Ev.build m Ly σ ∈ C.J := by simpa using hbu
    rcases hl m Ly σ hbu' with hr | ⟨mm, e, hm, he, rfl⟩
    · exact .inl (by simp [hr])
    · by_cases hk : m = m₀ ∧ Ly = Ly₀
      · obtain ⟨rfl, rfl⟩ := hk; rw [hm₀] at hm; cases hm; rw [he₀] at he; cases he
        exact .inl (by simp)
      · obtain ⟨mm', h1, h2⟩ := setEnt_keep (e₀ := none) hm he hk
        exact .inr ⟨mm', e, h1, h2, rfl⟩
  all_goals
    intro m Ly σ hb
    simp only [List.mem_cons, reduceCtorEq, false_or] at hb ⊢
    rcases hl m Ly σ hb with hr | h'
    · exact .inl (by simp [hr])
    · exact .inr h'

set_option maxHeartbeats 1000000 in
theorem step_obsIn (I : Inv C) (I6 : Inv6 C) (hs : Step K L C C') :
    ∀ m mm Ly e, C'.M m = some mm → mm.ents Ly = some e →
      ∀ π i, .obs π m Ly e.σ i ∈ C'.J → i ∈ e.obs ∨ .run π i ∈ C'.J := by
  have ho := I6.obsIn
  have hbd := I6.building
  cases hs
  case provide =>
    obtain ⟨-, -, -, -, hMn⟩ := I.fresh (C.n + 1) (by omega)
    intro m mm Ly e hm he π i hob
    rcases updM_ent hm he with ⟨rfl, hx⟩ | ⟨mm₀, hm₀, he₀⟩
    · simp only [Option.some.injEq] at hx; subst hx; simp at he
    · exact ho m mm₀ Ly e hm₀ he₀ π i hob
  case provHit f φ πp m₀ Ly₀ c Ls b m' e₀ o mm₀ hf hc hmemo hlk hb hΛ hst hm₀ =>
    obtain ⟨mm₁, hm₁, he₁⟩ := lookup_lt hlk
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.obs.injEq] at hob ⊢
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      rcases hob with ⟨-, -, -, -, rfl⟩ | hob
      · exact .inl List.mem_cons_self
      · rw [hm₁] at hm₀; cases hm₀
        rcases ho _ _ _ _ hm₁ he₁ π i hob with h1 | h1
        · exact .inl (List.mem_cons_of_mem _ h1)
        · exact .inr h1
    · rcases hob with ⟨-, rfl, rfl, -, -⟩ | hob
      · exact absurd ⟨rfl, rfl⟩ hk
      · exact ho _ _ _ _ hm₂ he₂ π i hob
  case buildOk f φ πp m₀ Ly₀ σ₀ Ls b s v o mm₀ e₀ hf hc hs hΛ hst hm₀ he₀ =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.obs.injEq] at hob ⊢
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      rcases hob with ⟨-, -, -, -, rfl⟩ | hob
      · exact .inl List.mem_cons_self
      · exact absurd hob (hbd _ _ _ _ hm₁ he₁ rfl π i)
    · rcases hob with ⟨-, rfl, rfl, -, -⟩ | hob
      · exact absurd ⟨rfl, rfl⟩ hk
      · exact ho _ _ _ _ hm₂ he₂ π i hob
  case buildFail f φ πp m₀ Ly₀ σ₀ Ls b s c oσ hf hc hs hσ hσo =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or] at hob ⊢
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      exact absurd hob (hbd _ _ _ _ hm₁ he₁ rfl π i)
    · exact ho _ _ _ _ hm₂ he₂ π i hob
  case provBuild f φ πp m₀ Ly₀ c Ls b hf hc hlk hsome =>
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or] at hob ⊢
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      exact absurd hob (I.fresh_ev (by simp [Ev.ids]))
    · exact ho _ _ _ _ hm₂ he₂ π i hob
  case finObs f φ π₀ s o i₀ m₀ Ly₀ fs mm₀ e₀ hf hc hs hΛ hph hfs hm₀ he₀ hobs hst =>
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.run.injEq] at hob ⊢
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      rcases ho _ _ _ _ hm₀ he₀ π i hob with h1 | h1
      · by_cases hi : i = i₀
        · subst hi; exact .inr (.inl ⟨obs_owner I I6 hΛ hfs hob, rfl⟩)
        · exact .inl ((List.mem_erase_of_ne hi).2 h1)
      · exact .inr (.inr h1)
    · rcases ho _ _ _ _ hm₂ he₂ π i hob with h1 | h1
      · exact .inl h1
      · exact .inr (.inr h1)
  case finObsLast f φ π₀ s o i₀ m₀ Ly₀ fs mm₀ e₀ oσ hf hc hs hΛ hph hfs hm₀ he₀ hobs hne hσ hσo hst hb =>
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.run.injEq] at hob ⊢
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · cases he'
    · rcases ho _ _ _ _ hm₂ he₂ π i hob with h1 | h1
      · exact .inl h1
      · exact .inr (.inr h1)
  all_goals
    intro m mm Ly e hm he π i hob
    simp only [List.mem_cons, reduceCtorEq, false_or] at hob ⊢
    first
    | exact ho _ _ _ _ hm he π i hob
    | (rcases ho _ _ _ _ hm he π i hob with h1 | h1
       · exact .inl h1
       · exact .inr (by simp [h1]))

set_option maxHeartbeats 1000000 in
theorem step_noRel (I : Inv C) (I6 : Inv6 C) (hs : Step K L C C') :
    ∀ m mm Ly e, C'.M m = some mm → mm.ents Ly = some e → .release m Ly e.σ ∉ C'.J := by
  have hn := I6.noRel
  cases hs
  case provide =>
    intro m mm Ly e hm he
    rcases updM_ent hm he with ⟨rfl, hx⟩ | ⟨mm₀, hm₀, he₀⟩
    · simp only [Option.some.injEq] at hx; subst hx; simp at he
    · exact hn m mm₀ Ly e hm₀ he₀
  case provHit f φ πp m₀ Ly₀ c Ls b m' e₀ o mm₀ hf hc hmemo hlk hb ho hst hm₀ =>
    obtain ⟨mm₁, hm₁, he₁⟩ := lookup_lt hlk
    intro m mm Ly e hm he
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₁ he₁
    · simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₂ he₂
  case finObs f φ π s o i m₀ Ly₀ fs mm₀ e₀ hf hc hs ho hph hfs hm₀ he₀ hobs hst =>
    intro m mm Ly e hm he
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₀ he₀
    · simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₂ he₂
  case buildOk f φ πp m₀ Ly₀ σ₀ Ls b s v o mm₀ e₀ hf hc hs ho hst hm₀ he₀ =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    intro m mm Ly e hm he
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₁ he₁
    · simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₂ he₂
  case buildFail f φ πp m₀ Ly₀ σ₀ Ls b s c oσ hf hc hs hσ hσo =>
    obtain ⟨-, -, mm₁, hm₁, he₁⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ₀ Ls b) (by simp [hs])).build
    intro m mm Ly e hm he
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₁ he₁
    · simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₂ he₂
  case provBuild f φ πp m₀ Ly₀ c Ls b hf hc hlk hsome =>
    intro m mm Ly e hm he
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · simp only [Option.some.injEq] at he'; subst he'
      simp only [List.mem_cons, reduceCtorEq, false_or]; exact I.fresh_ev (by simp [Ev.ids])
    · simp only [List.mem_cons, reduceCtorEq, false_or]; exact hn _ _ _ _ hm₂ he₂
  case finObsLast f φ π s o i m₀ Ly₀ fs mm₀ e₀ oσ hf hc hs ho hph hfs hm₀ he₀ hobs hne hσ hσo hst hb =>
    intro m mm Ly e hm he
    rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hk, mm₂, hm₂, he₂⟩
    · cases he'
    · simp only [List.mem_cons, reduceCtorEq, false_or, Ev.release.injEq, not_or]
      exact ⟨fun h => hk ⟨h.1, h.2.1⟩, hn _ _ _ _ hm₂ he₂⟩
  all_goals
    intro m mm Ly e hm he
    simp only [List.mem_cons, reduceCtorEq, false_or]
    exact hn _ _ _ _ hm he

end Flick.RT
