import Formal.RT.Agree

/-! Preservation of the invariant about memo entries. -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- Owners are never removed. -/
theorem step_Λ_mono (hs : Step K L C C') {π : Nat} {o : Owner T V} (h : C.Λ π = some o) :
    ∃ o', C'.Λ π = some o' := by
  cases hs <;> simp only [upd, updO] <;> (repeat' split) <;> simp_all

/-- A frame that holds an owner after a step held it before, or holds the fresh owner. -/
theorem step_held_frames (hs : Step K L C C') : ∀ j (ψ : Fiber T V) fr π,
    C'.Φ j = some (.run ψ) → fr ∈ ψ.stack → fr.held = some π →
    (∃ φ₀, C.Φ j = some (.run φ₀) ∧ fr ∈ φ₀.stack) ∨ π = C.n := by
  cases hs
  case fiber f φ φ' hf hl =>
    intro j ψ fr π hj hfr hh
    fiber_cases hj
    · rcases hl.frames fr hfr with h | ⟨h, -⟩
      · exact .inl ⟨_, hf, h⟩
      · rw [h] at hh; cases hh
    · exact .inl ⟨_, hj, hfr⟩
  all_goals intro j ψ fr π hj hfr hh; fiber_cases hj <;>
    first
    | exact .inl ⟨_, hj, hfr⟩
    | (simp at hfr; done)
    | (obtain ⟨φ₀, hφ₀⟩ : ∃ φ₀, C.Φ j = some (.run φ₀) := ⟨_, ‹_›⟩
       have : fr ∈ φ₀.stack ∨ π = C.n := by grind [Frame.held]
       rcases this with h | h
       · exact .inl ⟨_, hφ₀, h⟩
       · exact .inr h)

end Flick.RT

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- `M` changed at one entry: entries elsewhere are old ones. -/
theorem ents_setEnt {M M' : Nat → Option (Memo V)} {m₀ Ly₀ : Nat} {e₀ : Option (Ent V)}
    {P : Ent V → Prop} (hM : M' = setEnt M m₀ Ly₀ e₀)
    (hold : ∀ m Ly mm e, M m = some mm → mm.ents Ly = some e → P e)
    (hnew : ∀ e, e₀ = some e → P e) :
    ∀ m mm Ly e, M' m = some mm → mm.ents Ly = some e → P e := by
  intro m mm Ly e hm he
  rw [hM] at hm; unfold setEnt at hm
  split at hm
  · next e₁ =>
    subst e₁
    cases hm₀ : M m with
    | none => rw [hm₀] at hm; cases hm
    | some mm₀ =>
      rw [hm₀] at hm; simp only [Option.map_some, Option.some.injEq] at hm; subst hm
      simp only [upd] at he
      split at he
      · exact hnew e he
      · exact hold _ _ _ _ hm₀ he
  · exact hold _ _ _ _ hm he

/-- An entry that was in `C` with the same area keeps its facts. -/
theorem ents_old (I : Inv C) (hs : Step K L C C') {m Ly : Nat} {mm₀ : Memo V} {e₀ e : Ent V}
    (hm : C.M m = some mm₀) (he : mm₀.ents Ly = some e₀) (hσ : e.σ = e₀.σ)
    (hb : e.st = .built → e₀.st = .built) :
    e.σ ≠ 0 ∧ (∃ oσ, C'.Λ e.σ = some oσ) ∧
    (e.st = .built → ∀ j φ, C'.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, fr.held ≠ some e.σ) := by
  obtain ⟨h0, ⟨oσ, hoσ⟩, hn⟩ := I.ents m mm₀ Ly e₀ hm he
  rw [hσ]
  refine ⟨h0, step_Λ_mono hs hoσ, fun hbu j φ hj fr hfr hh => ?_⟩
  rcases step_held_frames hs j φ fr _ hj hfr hh with ⟨φ₀, hφ₀, hfr₀⟩ | hn'
  · exact hn (hb hbu) j φ₀ hφ₀ fr hfr₀ hh
  · exact Nat.lt_irrefl _ (hn' ▸ I.ltΛ hoσ)

set_option maxHeartbeats 1000000 in
theorem step_ents (I : Inv C) (hs : Step K L C C') :
    ∀ m mm Ly e, C'.M m = some mm → mm.ents Ly = some e → e.σ ≠ 0 ∧
      (∃ oσ, C'.Λ e.σ = some oσ) ∧
      (e.st = .built → ∀ j φ, C'.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, fr.held ≠ some e.σ) := by
  have hold : ∀ m Ly (mm₀ : Memo V) (e₀ e : Ent V), C.M m = some mm₀ → mm₀.ents Ly = some e₀ →
      e.σ = e₀.σ → (e.st = .built → e₀.st = .built) → e.σ ≠ 0 ∧ (∃ oσ, C'.Λ e.σ = some oσ) ∧
      (e.st = .built → ∀ j φ, C'.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, fr.held ≠ some e.σ) :=
    fun _ _ _ _ _ hm he hσ hb => ents_old I hs hm he hσ hb
  have hhf := step_held_frames hs
  have hmono : ∀ π (o : Owner T V), C.Λ π = some o → ∃ o', C'.Λ π = some o' :=
    fun _ _ h => step_Λ_mono hs h
  cases hs
  case provide f φ e e' Ls b hf hc hst =>
    intro m mm Ly e hm he
    simp only [upd] at hm
    split at hm
    · cases hm; simp at he
    · exact hold m Ly mm e e hm he rfl id
  case provBuild f φ πp m₀ Ly₀ c Ls b hf hc hlk hsome =>
    intro m mm Ly e hm he
    simp only [setEnt] at hm
    split at hm
    · subst_vars
      obtain ⟨mm₀, hm₀⟩ := Option.isSome_iff_exists.1 hsome
      rw [hm₀] at hm; simp only [Option.map_some, Option.some.injEq] at hm; subst hm
      simp only [upd] at he
      split at he
      · cases he
        obtain ⟨-, hΛn, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
        refine ⟨fun h0 => ?_, ⟨Owner.fresh (.ok K.unit) f, by simp [upd]⟩, fun hb => by cases hb⟩
        obtain ⟨o, ho, _⟩ := I.root; rw [← h0, hΛn] at ho; cases ho
      · exact hold _ Ly mm₀ e e hm₀ he rfl id
    · exact hold m Ly mm e e hm he rfl id
  case provHit f φ πp m₀ Ly₀ c Ls b m' e₀ o mm₀ hf hc hmemo hlk hb ho hst hm₀ =>
    obtain ⟨mm₁, hm₁, he₁⟩ := lookup_lt hlk
    exact ents_setEnt rfl (fun m Ly mm e hm he => hold m Ly mm e e hm he rfl id)
      (fun e he => by cases he; exact hold _ _ _ e₀ _ hm₁ he₁ rfl fun _ => hb)
  case finObs f φ π s o i m₀ Ly₀ fs mm₀ e₀ hf hc hs ho hph hfs hm₀ he₀ hobs hst =>
    exact ents_setEnt rfl (fun m Ly mm e hm he => hold m Ly mm e e hm he rfl id)
      (fun e he => by cases he; exact hold _ _ _ e₀ _ hm₀ he₀ rfl id)
  case finObsLast =>
    exact ents_setEnt rfl (fun m Ly mm e hm he => hold m Ly mm e e hm he rfl id)
      (fun e he => by cases he)
  case buildFail f φ πp m₀ Ly₀ σ Ls b s c oσ hf hc hs hσ hσo =>
    obtain ⟨-, -, mm₀, hm₀, he₀⟩ := (I.frames f φ hf (.build πp m₀ Ly₀ σ Ls b) (by simp [hs])).build
    exact ents_setEnt rfl (fun m Ly mm e hm he => hold m Ly mm e e hm he rfl id)
      (fun e he => by cases he; exact hold _ _ _ _ _ hm₀ he₀ rfl fun h => by cases h)
  case buildOk f φ πp m₀ Ly₀ σ Ls b s v o mm₀ e₀ hf hc hs ho hst hm₀ he₀ =>
    obtain ⟨hσ0, ⟨oσ, hoσ, _, hoh⟩, mm₁, hm₁, he₁⟩ :=
      (I.frames f φ hf (.build πp m₀ Ly₀ σ Ls b) (by simp [hs])).build
    refine ents_setEnt rfl (fun m Ly mm e hm he => hold m Ly mm e e hm he rfl id)
      (fun e he => ?_)
    cases he
    refine ⟨hσ0, hmono _ _ hoσ, fun _ j ψ hj fr hfr hh => ?_⟩
    rcases hhf j ψ fr σ hj hfr hh with ⟨φ₀, hφ₀, hfr₀⟩ | hn
    · simp only [upd, setF] at hj
      split at hj
      · subst_vars
        simp only [Option.some.injEq, FState.run.injEq] at hj; subst hj
        exact held_rest (by rw [← hs]; exact I.held _ φ hf) rfl fr hfr hh
      · obtain ⟨o', ho', _, hoj⟩ := (I.frames j φ₀ hφ₀ fr hfr₀).hold hh
        rw [hoσ] at ho'; cases ho'
        exact ‹¬j = f› (hoj.symm.trans hoh)
    · exact Nat.lt_irrefl _ (hn ▸ I.ltΛ hoσ)
  all_goals intro m mm Ly e hm he; exact hold m Ly mm e e hm he rfl id


theorem step_closeΛ (I : Inv C) (hs : Step K L C C') :
    ∀ π, .close π ∈ C'.J → ∃ o, C'.Λ π = some o := by
  have hJ := I.closeΛ
  have hmono : ∀ π (o : Owner T V), C.Λ π = some o → ∃ o', C'.Λ π = some o' :=
    fun _ _ h => step_Λ_mono hs h
  cases hs <;> simp only [List.mem_cons, reduceCtorEq, false_or, Ev.close.injEq] <;>
    first
    | (intro π hπ; rcases hπ with rfl | hπ
       · exact hmono _ _ ‹_›
       · obtain ⟨o, ho⟩ := hJ π hπ; exact hmono π o ho)
    | (intro π hπ; obtain ⟨o, ho⟩ := hJ π hπ; exact hmono π o ho)

end Flick.RT
