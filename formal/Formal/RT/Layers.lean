import Formal.RT.Reach

/-!
# λRT: layers and memo maps (R6, И6; runtime.md §3.5)

An entry of a memo map keeps the ids of its observers (`Ent.obs`), so `n` of §3.5 is their
number. `Inv6` adds to `Inv` what R6 needs: a build that is not released has its entry
(R6a), an observer of an entry either is in its `obs` or has run (И6), and every observer
is a finalizer of its consumer.
-/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- What a layer event requires of the past. -/
def Good6 : Ev V → List (Ev V) → Prop
  | .build m Ly _, J => ∀ σ', .build m Ly σ' ∈ J → .release m Ly σ' ∈ J
  | .release m Ly σ, J => ∀ π i, .obs π m Ly σ i ∈ J → .run π i ∈ J
  | .obs _ m Ly σ _, J => .release m Ly σ ∉ J
  | _, _ => True

structure Inv6 (C : Conf T V) : Prop where
  hist : Hist Good6 C.J
  live : ∀ m Ly σ, .build m Ly σ ∈ C.J → .release m Ly σ ∈ C.J ∨
    ∃ mm e, C.M m = some mm ∧ mm.ents Ly = some e ∧ e.σ = σ
  /-- И6: an observer of an entry is in its `obs` until its consumer's (C5) runs it -/
  obsIn : ∀ m mm Ly e, C.M m = some mm → mm.ents Ly = some e →
    ∀ π i, .obs π m Ly e.σ i ∈ C.J → i ∈ e.obs ∨ .run π i ∈ C.J
  obsFin : ∀ π m Ly σ i, .obs π m Ly σ i ∈ C.J → .fin π i ∈ C.J
  building : ∀ m mm Ly e, C.M m = some mm → mm.ents Ly = some e → e.st = .building →
    ∀ π i, .obs π m Ly e.σ i ∉ C.J
  noRel : ∀ m mm Ly e, C.M m = some mm → mm.ents Ly = some e → .release m Ly e.σ ∉ C.J

theorem Inv6.initial (e₀ : T) (unit : V) : Inv6 (init e₀ unit) :=
  ⟨fun J₁ e J₂ h => by cases J₁ <;> simp [RT.init] at h, by simp [RT.init],
   by simp [RT.init], by simp [RT.init], by simp [RT.init], by simp [RT.init]⟩

/-- An entry after `setEnt` is the new one or an old one. -/
theorem setEnt_ent {M : Nat → Option (Memo V)} {m₀ Ly₀ m Ly : Nat} {e₀ : Option (Ent V)}
    {mm : Memo V} {e : Ent V} (hm : setEnt M m₀ Ly₀ e₀ m = some mm) (he : mm.ents Ly = some e) :
    (m = m₀ ∧ Ly = Ly₀ ∧ e₀ = some e) ∨
      (¬(m = m₀ ∧ Ly = Ly₀) ∧ ∃ mm₀, M m = some mm₀ ∧ mm₀.ents Ly = some e) := by
  unfold setEnt at hm
  split at hm
  · next h =>
    subst h
    cases h₀ : M m with
    | none => rw [h₀] at hm; cases hm
    | some mm₀ =>
      rw [h₀] at hm; simp only [Option.map_some, Option.some.injEq] at hm; subst hm
      simp only [upd] at he
      split at he
      · exact .inl ⟨rfl, ‹_›, he⟩
      · exact .inr ⟨fun h => ‹¬_› h.2, mm₀, rfl, he⟩
  · exact .inr ⟨fun h => ‹¬_› h.1, mm, hm, he⟩

/-- An entry after `upd` of a map is in the new map or an old one. -/
theorem updM_ent {M : Nat → Option (Memo V)} {m₀ m Ly : Nat} {x : Option (Memo V)}
    {mm : Memo V} {e : Ent V} (hm : upd M m₀ x m = some mm) (he : mm.ents Ly = some e) :
    (m = m₀ ∧ x = some mm) ∨ ∃ mm₀, M m = some mm₀ ∧ mm₀.ents Ly = some e := by
  simp only [upd] at hm
  split at hm
  · exact .inl ⟨‹_›, hm⟩
  · exact .inr ⟨mm, hm, he⟩

theorem step_obsFin (I6 : Inv6 C) (hs : Step K L C C') :
    ∀ π m Ly σ i, .obs π m Ly σ i ∈ C'.J → .fin π i ∈ C'.J := by
  have hJ := I6.obsFin
  cases hs <;> simp only [List.mem_cons, reduceCtorEq, false_or, Ev.obs.injEq, Ev.fin.injEq] <;>
    grind

set_option maxHeartbeats 1000000 in
theorem step_building (I : Inv C) (I6 : Inv6 C) (hs : Step K L C C') :
    ∀ m mm Ly e, C'.M m = some mm → mm.ents Ly = some e → e.st = .building →
      ∀ π i, .obs π m Ly e.σ i ∉ C'.J := by
  have hJ := I6.building
  have hfr : ∀ π m Ly i, Ev.obs π m Ly C.n i ∉ C.J := fun _ _ _ _ => I.fresh_ev (by simp [Ev.ids])
  obtain ⟨-, -, -, -, hMn⟩ := I.fresh (C.n + 1) (by omega)
  have hlk : ∀ Ly m (mm : Memo V) m' e, lookup C.M Ly C.n m = some (m', e) →
      ∃ mm, C.M m' = some mm ∧ mm.ents Ly = some e := fun _ _ _ _ _ h => lookup_lt h
  clear I
  cases hs <;> intro m mm Ly e hm he hb π i <;>
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.obs.injEq] <;>
    first
    | exact hJ m mm Ly e hm he hb π i
    | (rcases setEnt_ent hm he with ⟨rfl, rfl, he'⟩ | ⟨hne, mm₀, hm₀, he₀⟩
       · first
         | (simp only [reduceCtorEq] at he')
         | (simp only [Option.some.injEq] at he'; subst he'; grind)
       · grind)
    | (simp only [upd] at hm; split at hm
       · simp only [Option.some.injEq] at hm; subst hm; simp at he
       · grind)

theorem setEnt_keep {M : Nat → Option (Memo V)} {m₀ Ly₀ m Ly : Nat} {e₀ : Option (Ent V)}
    {mm : Memo V} {e : Ent V} (hm : M m = some mm) (he : mm.ents Ly = some e)
    (hk : ¬(m = m₀ ∧ Ly = Ly₀)) : ∃ mm', setEnt M m₀ Ly₀ e₀ m = some mm' ∧ mm'.ents Ly = some e := by
  unfold setEnt
  by_cases h : m = m₀
  · subst h
    refine ⟨{ mm with ents := upd mm.ents Ly₀ e₀ }, by simp [hm], ?_⟩
    have : Ly ≠ Ly₀ := fun h' => hk ⟨rfl, h'⟩
    simp [upd, this, he]
  · exact ⟨mm, by simp [h, hm], he⟩

theorem setEnt_here {M : Nat → Option (Memo V)} {m₀ Ly₀ : Nat} {e₀ : Ent V} {mm : Memo V}
    (hm : M m₀ = some mm) : ∃ mm', setEnt M m₀ Ly₀ (some e₀) m₀ = some mm' ∧
      mm'.ents Ly₀ = some e₀ :=
  ⟨{ mm with ents := upd mm.ents Ly₀ (some e₀) }, by simp [setEnt, hm], by simp [upd]⟩

/-- `M` changed at one entry that keeps its area, or not at all: entries keep their areas. -/
theorem live_setEnt {J J' : List (Ev V)} {M M' : Nat → Option (Memo V)} {m₀ Ly₀ : Nat}
    {e₀ : Ent V} (hM : M' = setEnt M m₀ Ly₀ (some e₀))
    (hσ : ∀ mm e, M m₀ = some mm → mm.ents Ly₀ = some e → e₀.σ = e.σ)
    (hJ : ∀ e ∈ J, e ∈ J') (hb' : ∀ m Ly σ, Ev.build m Ly σ ∈ J' → Ev.build m Ly σ ∈ J)
    (h : ∀ m Ly σ, .build m Ly σ ∈ J → .release m Ly σ ∈ J ∨
      ∃ mm e, M m = some mm ∧ mm.ents Ly = some e ∧ e.σ = σ) :
    ∀ m Ly σ, .build m Ly σ ∈ J' → .release m Ly σ ∈ J' ∨
      ∃ mm e, M' m = some mm ∧ mm.ents Ly = some e ∧ e.σ = σ := by
  intro m Ly σ hb
  rcases h m Ly σ (hb' m Ly σ hb) with hr | ⟨mm, e, hm, he, rfl⟩
  · exact .inl (hJ _ hr)
  · right; subst hM
    by_cases hk : m = m₀ ∧ Ly = Ly₀
    · obtain ⟨rfl, rfl⟩ := hk
      obtain ⟨mm', h1, h2⟩ := setEnt_here (Ly₀ := Ly) (e₀ := e₀) hm
      exact ⟨mm', e₀, h1, h2, hσ mm e hm he⟩
    · obtain ⟨mm', h1, h2⟩ := setEnt_keep (e₀ := some e₀) hm he hk
      exact ⟨mm', e, h1, h2, rfl⟩

end Flick.RT
