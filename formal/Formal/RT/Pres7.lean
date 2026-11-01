import Formal.RT.Pres6

/-! Preservation of И3, of the watcher's link to R4 and of the Promises of `P` (R2). -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

theorem Local.not_watch {φ φ' : Fiber T V} {π k : Nat} {g : T} (hl : Local K L C φ φ')
    (hc : φ.code = .watch π k g) : False := by
  cases hl <;> simp_all [Code.IntPoint]

set_option maxHeartbeats 4000000 in
theorem step_spawnJ (I : Inv C) (hs : Step K L C C') :
    ∀ π k w, .spawn π k w ∈ C'.J →
      (∃ o, C'.Λ π = some o ∧ (k ∈ o.K ∨ DoneJ k C'.J) ∧ (w ∈ o.W ∨ DoneJ w C'.J)) := by
  have hJ := I.spawnJ
  have hdj := I.doneJ
  have hdj' : ∀ k x, C.Φ k = some (.done x) → Ev.done k x ∈ C.J := fun k x h => (I.doneJ k x).2 h
  have hfr : ∀ π k, Ev.spawn π k C.n ∉ C.J ∧ Ev.spawn π C.n k ∉ C.J :=
    fun π k => ⟨I.fresh_ev (by simp [Ev.ids]), I.fresh_ev (by simp [Ev.ids])⟩
  obtain ⟨-, hΛn, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
  have her : ∀ (a b : Nat) (l : List Nat), a ≠ b → a ∈ l → a ∈ l.erase b :=
    fun _ _ _ h1 h2 => (List.mem_erase_of_ne h1).2 h2
  clear I
  cases hs <;> intro π k w <;> simp only [upd, updO, setF, DoneJ, List.mem_cons, reduceCtorEq,
    false_or, Ev.spawn.injEq, Ev.done.injEq] at hJ ⊢ <;> grind [Owner.fresh]

set_option maxHeartbeats 4000000 in
theorem step_watchJ (I : Inv C) (hs : Step K L C C') :
    ∀ π k w, .spawn π k w ∈ C'.J →
      (∃ φ g, C'.Φ w = some (.run φ) ∧ φ.code = .watch π k g) ∨
      (DoneJ k C'.J ∧ ∀ c, .done k (.fail c) ∈ C'.J → .sup k c ∈ C'.J) := by
  have hJ := I.watchJ
  have hdj := I.doneJ
  have hdj' : ∀ k x, C.Φ k = some (.done x) → Ev.done k x ∈ C.J := fun k x h => (I.doneJ k x).2 h
  have hfr : ∀ π k, Ev.spawn π k C.n ∉ C.J ∧ Ev.spawn π C.n k ∉ C.J ∧
      Ev.spawn π k (C.n + 1) ∉ C.J := fun π k =>
    ⟨I.fresh_ev (by simp [Ev.ids]), I.fresh_ev (by simp [Ev.ids]),
     fun h => by have := I.evIds _ h (C.n + 1) (by simp [Ev.ids]); omega⟩
  obtain ⟨hf1, -, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
  have hlw : ∀ (φ φ' : Fiber T V) π k g, Local K L C φ φ' → φ.code = .watch π k g → False :=
    fun _ _ _ _ _ hl hc => hl.not_watch hc
  have hwlt : ∀ π k w, Ev.spawn π k w ∈ C.J → w < C.n ∧ k < C.n := fun π k w h =>
    ⟨I.evIds _ h w (by simp [Ev.ids]), I.evIds _ h k (by simp [Ev.ids])⟩
  clear I
  cases hs
  case spawn f φ e e' π₁ g t o hf hc hst ho hop =>
    intro π k w hsp
    simp only [List.mem_cons, Ev.spawn.injEq] at hsp
    rcases hsp with ⟨rfl, rfl, rfl⟩ | hsp
    · exact .inl ⟨_, g, (by simp [upd] <;> rfl), rfl⟩
    · obtain ⟨h1, h2⟩ := hwlt π k w hsp
      simp only [upd, setF, List.mem_cons, reduceCtorEq, false_or, Ev.done.injEq, Ev.sup.injEq,
        DoneJ] at hJ ⊢
      grind
  all_goals intro π k w <;> simp only [upd, setF, List.mem_cons, reduceCtorEq,
    false_or, Ev.spawn.injEq, Ev.done.injEq, Ev.sup.injEq, DoneJ] at hJ ⊢ <;> grind

set_option maxHeartbeats 4000000 in
theorem step_extJ [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ π p, .ext π p ∈ C'.J → .settle p ∈ C'.J ∨
      (C'.X p = some π ∧ ∃ o, C'.Λ π = some o ∧ p ∈ o.P) := by
  have hJ := I.extJ
  have hfr : ∀ π, Ev.ext π C.n ∉ C.J := fun π => I.fresh_ev (by simp [Ev.ids])
  obtain ⟨-, hΛn, -, hX, -⟩ := I.fresh C.n (Nat.le_refl _)
  have her : ∀ (a b : Nat) (l : List Nat), a ≠ b → a ∈ l → a ∈ l.erase b :=
    fun _ _ _ h1 h2 => (List.mem_erase_of_ne h1).2 h2
  have g9 := h.g9
  clear I
  cases hs <;> intro π p <;> simp only [upd, updO, List.mem_cons, reduceCtorEq,
    false_or, Ev.ext.injEq, Ev.settle.injEq] at hJ ⊢ <;> grind [Owner.fresh]

end Flick.RT
