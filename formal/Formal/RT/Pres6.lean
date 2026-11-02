import Formal.RT.Agree

/-! Preservation of the invariants that tie the journal to fibers and groups (R3, R4, I3). -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- A step never changes a finished fiber. -/
theorem step_done (I : Inv C) (hs : Step K L C C') {k : Nat} {x : Out V}
    (hk : C.Φ k = some (.done x)) : C'.Φ k = some (.done x) := by
  have h1 := I.ltΦ hk
  cases hs <;> simp only [upd, setF] <;> (repeat' split) <;> first | omega | simp_all

set_option maxHeartbeats 4000000 in
theorem step_groups (I : Inv C) (hs : Step K L C C') :
    ∀ γ g, C'.G γ = some g → GroupOk C' γ g := by
  have hg := I.groups
  have hd : ∀ k, Done C.Φ k → Done C'.Φ k := fun k ⟨x, hx⟩ => ⟨x, step_done I hs hx⟩
  obtain ⟨-, hΛn, hGn, -, -⟩ := I.fresh C.n (Nat.le_refl _)
  clear I
  cases hs <;> intro γ g h' <;> (try simp only [upd, updO, GroupOk] at hg h' hd ⊢) <;>
    (repeat' split at h') <;> (try simp only [Option.some.injEq, reduceCtorEq] at h') <;>
    (try subst h') <;> (try subst_vars) <;> grind [St.live, Done]

set_option maxHeartbeats 4000000 in
theorem step_doneJ (I : Inv C) (hs : Step K L C C') :
    ∀ k x, .done k x ∈ C'.J ↔ C'.Φ k = some (.done x) := by
  have hJ := I.doneJ
  obtain ⟨hf1, -, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
  obtain ⟨hf1', -, -, -, -⟩ := I.fresh (C.n + 1) (by omega)
  have hfr : ∀ x, Ev.done C.n x ∉ C.J := fun x => I.fresh_ev (by simp [Ev.ids])
  clear I
  cases hs <;> intro k x <;> simp only [upd, setF, List.mem_cons, reduceCtorEq, false_or,
    Ev.done.injEq] <;> grind

set_option maxHeartbeats 4000000 in
theorem step_startJ (I : Inv C) (hs : Step K L C C') :
    ∀ π k, StartedIn π k C'.J → ∃ s, C'.Φ k = some s ∧ ∀ φ, s = .run φ → φ.base = π := by
  have hJ := I.startJ
  obtain ⟨hf1, -, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
  obtain ⟨hf1', -, -, -, -⟩ := I.fresh (C.n + 1) (by omega)
  have hfr : ∀ π k, Ev.spawn π C.n k ∉ C.J ∧ Ev.spawn π k C.n ∉ C.J ∧
      Ev.spawn π k (C.n + 1) ∉ C.J ∧ Ev.spawn π (C.n + 1) k ∉ C.J ∧ Ev.gkid π C.n ∉ C.J ∧
      Ev.branch π C.n ∉ C.J := fun π k =>
    ⟨I.fresh_ev (by simp [Ev.ids]), I.fresh_ev (by simp [Ev.ids]),
     fun h => by have := I.evIds _ h (C.n + 1) (by simp [Ev.ids]); omega,
     fun h => by have := I.evIds _ h (C.n + 1) (by simp [Ev.ids]); omega,
     I.fresh_ev (by simp [Ev.ids]), I.fresh_ev (by simp [Ev.ids])⟩
  have hloc : ∀ (φ φ' : Fiber T V), Local K L C φ φ' → φ'.base = φ.base :=
    fun _ _ h => h.same.1
  clear I
  cases hs <;> intro π k <;> simp only [StartedIn, upd, setF, List.mem_cons, reduceCtorEq,
    false_or, Ev.spawn.injEq, Ev.gkid.injEq, Ev.branch.injEq] at hJ ⊢ <;> grind

theorem held_pop {φ : Fiber T V} {fr : Frame T V} {s : List (Frame T V)} (hs : φ.stack = fr :: s)
    (h : (φ.stack.filterMap Frame.held).Nodup) : (s.filterMap Frame.held).Nodup := by
  rw [hs, List.filterMap_cons] at h
  split at h
  · exact h
  · exact h.of_cons

theorem Local.held {φ φ' : Fiber T V} (hl : Local K L C φ φ')
    (h : (φ.stack.filterMap Frame.held).Nodup) : (φ'.stack.filterMap Frame.held).Nodup := by
  cases hl with
  | resume _ hs | then_ _ hs | acqFail _ hs | parDone _ hs | parEarly _ _ hs => exact held_pop hs h
  | acquire | join | par | modRead => simpa [Frame.held] using h
  | _ => exact h

set_option maxHeartbeats 1000000 in
theorem step_held (I : Inv C) (hs : Step K L C C') :
    ∀ j φ, C'.Φ j = some (.run φ) → (φ.stack.filterMap Frame.held).Nodup := by
  have hh0 := I.held
  have hfresh : ∀ j (φ : Fiber T V), C.Φ j = some (.run φ) →
      C.n ∉ φ.stack.filterMap Frame.held := fun j φ hj hm => by
    obtain ⟨fr, hfr, hh⟩ := List.mem_filterMap.1 hm
    obtain ⟨o, ho, -, -⟩ := (I.frames j φ hj fr hfr).hold hh
    exact Nat.lt_irrefl _ (I.ltΛ ho)
  cases hs <;> intro j ψ h' <;> fiber_cases h' <;>
    first
    | exact hh0 _ _ h'
    | (simp; done)
    | (simp only [List.filterMap_cons, Frame.held]; exact hh0 _ _ ‹_›)
    | exact (‹Local K L C _ _›).held (hh0 _ _ ‹_›)
    | (dsimp only; exact held_pop ‹_› (hh0 _ _ ‹_›))
    | (dsimp only; simp only [List.filterMap_cons, Frame.held]; exact held_pop ‹_› (hh0 _ _ ‹_›))
    | (simp only [List.filterMap_cons, Frame.held, List.nodup_cons]
       exact ⟨hfresh _ _ ‹_›, hh0 _ _ ‹_›⟩)
    | skip

end Flick.RT
