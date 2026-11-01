import Formal.RT.Pres1

/-! How a step changes the owners and layers that frames point to. -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- A step keeps every owner, never reopens one, and keeps the state, holder and layer
being built of every owner outside `bad`. -/
def AgreeBut (C C' : Conf T V) (bad : Nat → Prop) : Prop :=
  (∀ π o, C.Λ π = some o → ∃ o', C'.Λ π = some o' ∧ (o.st ≠ .open → o'.st ≠ .open) ∧
    (¬ bad π → (o'.st = .open ↔ o.st = .open) ∧ o'.hold = o.hold)) ∧
  (∀ m mm Ly σ, ¬ bad σ → C.M m = some mm → mm.ents Ly = some ⟨.building, σ, []⟩ →
    ∃ mm', C'.M m = some mm' ∧ mm'.ents Ly = some ⟨.building, σ, []⟩)

theorem FrOk.agree {j : Nat} {fr : Frame T V} {bad : Nat → Prop} (ha : AgreeBut C C' bad)
    (hb : ∀ π, fr.held = some π → ¬ bad π) (h : FrOk C j fr) : FrOk C' j fr := by
  obtain ⟨hi, hbu, hc⟩ := h
  refine ⟨fun π env he => ?_, fun πp m Ly σ Ls b he => ?_, fun π he => ?_⟩
  · obtain ⟨h0, o, ho, hst, hh⟩ := hi π env he
    obtain ⟨o', ho', -, h2⟩ := ha.1 π o ho
    obtain ⟨hst', hh'⟩ := h2 (hb π (by subst he; rfl))
    exact ⟨h0, o', ho', hst'.2 hst, hh'.trans hh⟩
  · obtain ⟨h0, ⟨o, ho, hst, hh⟩, mm, hm, hent⟩ := hbu πp m Ly σ Ls b he
    have hσ := hb σ (by subst he; rfl)
    obtain ⟨o', ho', -, h2⟩ := ha.1 σ o ho
    obtain ⟨hst', hh'⟩ := h2 hσ
    exact ⟨h0, ⟨o', ho', hst'.2 hst, hh'.trans hh⟩, ha.2 m mm Ly σ hσ hm hent⟩
  · obtain ⟨h0, o, ho, hst⟩ := hc π he
    obtain ⟨o', ho', h1, -⟩ := ha.1 π o ho
    exact ⟨h0, o', ho', h1 hst⟩

theorem FrOk.triv {C : Conf T V} {j : Nat} {fr : Frame T V} (h1 : fr.held = none)
    (h2 : ∀ π, fr ≠ .closing π) : FrOk C j fr :=
  ⟨fun π env he => (by subst he; cases h1), fun _ _ _ _ _ _ he => (by subst he; cases h1),
   fun π he => absurd he (h2 π)⟩

/-- `Λ` changes only at `π`, keeping whether it is open and its holder. -/
theorem agree_upd {π : Nat} {o o' : Owner T V} (ho : C.Λ π = some o)
    (hst : o'.st = .open ↔ o.st = .open) (hh : o'.hold = o.hold)
    (hΛ : C'.Λ = upd C.Λ π (some o'))
    (hM : ∀ m mm Ly σ, C.M m = some mm → mm.ents Ly = some ⟨.building, σ, []⟩ →
      ∃ mm', C'.M m = some mm' ∧ mm'.ents Ly = some ⟨.building, σ, []⟩) :
    AgreeBut C C' fun _ => False := by
  refine ⟨fun π₁ o₁ h₁ => ?_, fun m mm Ly σ _ hm he => hM m mm Ly σ hm he⟩
  rw [hΛ]; by_cases e : π₁ = π
  · subst e; rw [ho] at h₁; cases h₁
    exact ⟨o', by simp, fun h₂ h₃ => h₂ (hst.1 h₃), fun _ => ⟨hst, hh⟩⟩
  · exact ⟨o₁, by simp [upd, e, h₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩

/-- `Λ` and `M` only grow. -/
theorem agree_ext (hΛ : ∀ π o, C.Λ π = some o → C'.Λ π = some o)
    (hM : ∀ m mm Ly σ, C.M m = some mm → mm.ents Ly = some ⟨.building, σ, []⟩ →
      ∃ mm', C'.M m = some mm' ∧ mm'.ents Ly = some ⟨.building, σ, []⟩) :
    AgreeBut C C' fun _ => False :=
  ⟨fun π o h => ⟨o, hΛ π o h, id, fun _ => ⟨Iff.rfl, rfl⟩⟩, fun m mm Ly σ _ => hM m mm Ly σ⟩

theorem M_same (hM : C'.M = C.M) : ∀ m mm Ly σ, C.M m = some mm →
    mm.ents Ly = some ⟨.building, σ, []⟩ →
    ∃ mm', C'.M m = some mm' ∧ mm'.ents Ly = some ⟨.building, σ, []⟩ :=
  fun _ mm _ _ hm he => ⟨mm, hM ▸ hm, he⟩

/-- `M` changes at an entry that is not a layer being built, or at a fresh map. -/
theorem M_setEnt {m₀ Ly₀ : Nat} {e₀ : Option (Ent V)}
    (hM : C'.M = setEnt C.M m₀ Ly₀ e₀)
    (hb : ∀ mm σ, C.M m₀ = some mm → mm.ents Ly₀ = some ⟨.building, σ, []⟩ →
      e₀ = some ⟨.building, σ, []⟩) :
    ∀ m mm Ly σ, C.M m = some mm → mm.ents Ly = some ⟨.building, σ, []⟩ →
    ∃ mm', C'.M m = some mm' ∧ mm'.ents Ly = some ⟨.building, σ, []⟩ := by
  intro m mm Ly σ hm he
  rw [hM]; unfold setEnt
  by_cases e : m = m₀
  · subst e; refine ⟨{ mm with ents := upd mm.ents Ly₀ e₀ }, by simp [hm], ?_⟩
    by_cases e' : Ly = Ly₀
    · subst e'; simpa [upd] using hb mm σ hm he
    · simpa [upd, e'] using he
  · exact ⟨mm, by simp [e, hm], he⟩

theorem agree_updO {π : Nat} {g : Owner T V → Owner T V} (hst : ∀ o, (g o).st = o.st)
    (hh : ∀ o, (g o).hold = o.hold) (hΛ : C'.Λ = updO C.Λ π g) (hM : C'.M = C.M) :
    AgreeBut C C' fun _ => False := by
  refine ⟨fun π₁ o₁ h₁ => ?_, fun m mm Ly σ _ hm he => ⟨mm, hM ▸ hm, he⟩⟩
  rw [hΛ]; by_cases e : π₁ = π
  · subst e; exact ⟨g o₁, by simp [updO, h₁], by rw [hst]; exact id, fun _ => ⟨by rw [hst], hh o₁⟩⟩
  · exact ⟨o₁, by simp [updO, e, h₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩

/-- `M` changes at the entry of a layer whose area is `bad`. -/
theorem M_setEnt_bad {m₀ Ly₀ σ₀ : Nat} {e₀ : Option (Ent V)} {bad : Nat → Prop}
    (hM : C'.M = setEnt C.M m₀ Ly₀ e₀) (hσ : bad σ₀)
    (hb : ∃ mm, C.M m₀ = some mm ∧ mm.ents Ly₀ = some ⟨.building, σ₀, []⟩) :
    ∀ m mm Ly σ, ¬ bad σ → C.M m = some mm → mm.ents Ly = some ⟨.building, σ, []⟩ →
    ∃ mm', C'.M m = some mm' ∧ mm'.ents Ly = some ⟨.building, σ, []⟩ := by
  intro m mm Ly σ hσ' hm he
  obtain ⟨mm₀, hm₀, he₀⟩ := hb
  rw [hM]; unfold setEnt
  by_cases e : m = m₀
  · subst e
    simp only [if_true, hm, Option.map_some, Option.some.injEq, exists_eq_left']
    by_cases e' : Ly = Ly₀
    · subst e'; rw [hm₀] at hm; cases hm; rw [he₀] at he; cases he; exact absurd hσ hσ'
    · simpa [upd, e'] using he
  · exact ⟨mm, by simp [e, hm], he⟩

theorem agree_same (hΛ : C'.Λ = C.Λ) (hM : C'.M = C.M) : AgreeBut C C' fun _ => False :=
  agree_ext (fun _ _ h => hΛ ▸ h) (M_same hM)

theorem frames_of {bad : Nat → Prop} (ha : AgreeBut C C' bad)
    (hsh : ∀ j φ, C'.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, FrOk C' j fr ∨
      ∃ φ₀, C.Φ j = some (.run φ₀) ∧ fr ∈ φ₀.stack ∧ (∀ π, fr.held = some π → ¬ bad π) ∧
        FrOk C j fr) :
    ∀ j φ, C'.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, FrOk C' j fr := by
  intro j φ h fr hfr
  rcases hsh j φ h fr hfr with h1 | ⟨_, _, _, hb, hok⟩
  · exact h1
  · exact hok.agree ha hb

theorem FrOk.hold {j π : Nat} {fr : Frame T V} (h : FrOk C j fr) (hh : fr.held = some π) :
    ∃ o, C.Λ π = some o ∧ o.st = .open ∧ o.hold = j := by
  cases fr with
  | inside π' env =>
    simp only [Frame.held, Option.some.injEq] at hh; subst hh
    obtain ⟨_, o, h1, h2, h3⟩ := h.inside; exact ⟨o, h1, h2, h3⟩
  | build πp m Ly σ Ls b =>
    simp only [Frame.held, Option.some.injEq] at hh; subst hh
    obtain ⟨_, ⟨o, h1, h2, h3⟩, _⟩ := h.build; exact ⟨o, h1, h2, h3⟩
  | _ => simp [Frame.held] at hh

/-- Only the fiber that holds an owner has a frame holding it, and only once. -/
theorem held_only (I : Inv C) {f j π : Nat} {φ ψ : Fiber T V} {fr s} (hf : C.Φ f = some (.run φ))
    (hs : φ.stack = fr :: s) (hh : fr.held = some π) (hj : C.Φ j = some (.run ψ))
    (hj' : j ≠ f ∨ ψ.stack = φ.stack) :
    ∀ fr' ∈ (if j = f then s else ψ.stack), fr'.held ≠ some π := by
  intro fr' hfr' hh'
  obtain ⟨o, ho, _, hof⟩ := (I.frames f φ hf fr (by simp [hs])).hold hh
  by_cases e : j = f
  · subst e; rw [if_pos rfl] at hfr'
    exact held_rest (by rw [← hs]; exact I.held j φ hf) hh fr' hfr' hh'
  · rw [if_neg e] at hfr'
    obtain ⟨o', ho', _, hoj⟩ := (I.frames j ψ hj fr' hfr').hold hh'
    rw [ho] at ho'; cases ho'; exact e (hoj.symm.trans hof)

/-- The old frames of the fiber that holds `π` (but the one popped) and of every other fiber
do not hold `π`. -/
theorem held_only' (I : Inv C) {f j π : Nat} {φ ψ : Fiber T V} {fr : Frame T V}
    {s : List (Frame T V)} (hf : C.Φ f = some (.run φ)) (hs : φ.stack = fr :: s)
    (hh : fr.held = some π) (hj : C.Φ j = some (.run ψ)) (hne : j ≠ f) :
    ∀ fr' ∈ ψ.stack, fr'.held ≠ some π := by
  have := held_only I hf hs hh hj (.inl hne); rwa [if_neg hne] at this

theorem held_only_self (I : Inv C) {f π : Nat} {φ : Fiber T V} {fr : Frame T V}
    {s : List (Frame T V)} (hf : C.Φ f = some (.run φ)) (hs : φ.stack = fr :: s)
    (hh : fr.held = some π) : ∀ fr' ∈ s, fr'.held ≠ some π := by
  have := held_only I hf hs hh hf (.inr rfl); rwa [if_pos rfl] at this

set_option hygiene false in
/-- The shape of the new stacks, for steps that keep every owner. -/
macro "shape_tac" : tactic => `(tactic| (
  intro j ψ h' fr hfr; fiber_cases h' <;>
  first
  | exact .inr ⟨_, h', hfr, fun _ _ h => h, hfr0 _ _ h' _ hfr⟩
  | (simp at hfr; done)
  | (obtain ⟨φ₀, hφ₀⟩ : ∃ φ₀, C.Φ j = some (.run φ₀) := ⟨_, ‹_›⟩
     rcases (show fr ∈ φ₀.stack ∨ (fr.held = none ∧ ∀ π, fr ≠ .closing π) from by
        clear hfr0 I; grind [Frame.held]) with hm | hm
     · exact .inr ⟨_, hφ₀, hm, fun _ _ h => h, hfr0 _ _ hφ₀ _ hm⟩
     · exact .inl (FrOk.triv hm.1 hm.2))))

end Flick.RT
