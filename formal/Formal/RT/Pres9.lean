import Formal.RT.NoRun

/-! Preservation of the journal invariant `Hist Good` (И1, И2, R1, R2). -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

/-- If no running fiber started in `π` and `P(π)` is empty, all of `π`'s work is done. -/
theorem kidsDone_of (I : Inv C) {π : Nat} (hnr : ∀ j ψ, C.Φ j = some (.run ψ) → ψ.base ≠ π)
    (hP : ∀ o, C.Λ π = some o → o.P = []) : KidsDone π C.J := by
  refine ⟨fun k hk => ?_, fun p hp => ?_⟩
  · obtain ⟨s, hs, hb⟩ := I.startJ π k hk
    cases s with
    | run ψ => exact absurd (hb ψ rfl) (hnr k ψ hs)
    | done x => exact ⟨x, (I.doneJ k x).2 hs⟩
  · rcases I.extJ π p hp with h | ⟨_, o, ho, hp'⟩
    · exact h
    · rw [hP o ho] at hp'; cases hp'

/-- An owner past (C5) has no running fiber and no Promise. -/
theorem kidsDone_dead (I : Inv C) {π : Nat} {o : Owner T V} (ho : C.Λ π = some o)
    (hl : o.st.live = false) : KidsDone π C.J := by
  refine kidsDone_of I (fun j ψ hj hb => ?_) (fun o' ho' => ?_)
  · obtain ⟨o', ho', hl'⟩ := I.live j ψ hj
    rw [hb, ho] at ho'; cases ho'; rw [hl] at hl'; cases hl'
  · rw [ho] at ho'; cases ho'; exact (I.empty π o ho hl).2.2.1

theorem started_cons {i : Nat} {e : Ev V} {J : List (Ev V)}
    (he : ∀ π, e ≠ .run π i ∧ e ≠ .now π i) : Started i (e :: J) ↔ Started i J := by
  simp only [Started, List.mem_cons]
  constructor
  · rintro ⟨π, (h | h) | (h | h)⟩
    · exact absurd h.symm (he π).1
    · exact ⟨π, .inl h⟩
    · exact absurd h.symm (he π).2
    · exact ⟨π, .inr h⟩
  · rintro ⟨π, h | h⟩
    · exact ⟨π, .inl (.inr h)⟩
    · exact ⟨π, .inr (.inr h)⟩

/-- What a (C5) step that starts `t` owes to `Good`. -/
theorem good_run (I : Inv C) {π : Nat} {o : Owner T V} {t : Fin T} {fs : List (Fin T)}
    (ho : C.Λ π = some o) (hfs : o.fs = t :: fs) (hst : o.st = .finalizing) :
    Good (.run π t.id) C.J := by
  obtain ⟨h1, h2, h3⟩ := I.finIn π o t ho (by simp [hfs])
  obtain ⟨-, hl, hc⟩ := I.stJ π o ho
  refine ⟨h1, h2, h3, ?_, ?_, kidsDone_dead I ho (by simp [hst, St.live])⟩
  · exact Classical.byContradiction fun h => by
      have := hl.2 h; rw [hst] at this; cases this
  · intro h; have := hc.2 h; rw [hst] at this; cases this

theorem not_started_fresh (I : Inv C) : ¬ Started C.n C.J := by
  rintro ⟨π, h | h⟩ <;> exact I.fresh_ev (by simp [Ev.ids]) h

set_option maxHeartbeats 1000000 in
theorem step_hist (I : Inv C) (hs : Step K L C C') :
    Hist Good C'.J := by
  have hJ := I.hist
  have hns := not_started_fresh I
  have hfin : ∀ π, Ev.fin π C.n ∉ C.J := fun π => I.fresh_ev (by simp [Ev.ids])
  have hlt : ∀ π i, Ev.fin π i ∈ C.J → i < C.n := fun π i h => I.evIds _ h i (by simp [Ev.ids])
  have hst : ∀ π (o : Owner T V), C.Λ π = some o → (o.st = .open ↔ .close π ∉ C.J) ∧
      (o.st.live = true ↔ .fins π ∉ C.J) ∧ (o.st = .closed ↔ .closed π ∈ C.J) := I.stJ
  cases hs <;> simp only [hist_cons, Good, hJ, and_true, true_and, List.mem_cons, reduceCtorEq,
    false_or, not_or, Ev.fin.injEq, Ev.run.injEq, Ev.now.injEq, Ev.fins.injEq, Ev.close.injEq,
    Ev.closed.injEq]
  case spawn => exact (hst _ _ ‹_›).1.1 ‹_›
  case close => exact (hst _ _ ‹_›).1.1 ‹_›
  case buildFail => exact (hst _ _ ‹_›).1.1 ‹_›
  case addNow => rw [started_cons (fun _ => ⟨by simp, by simp⟩)]; exact hns
  case acqNow => rw [started_cons (fun _ => ⟨by simp, by simp⟩)]; exact hns
  case addFin | acqLive =>
    rw [started_cons (fun _ => ⟨by simp, by simp⟩)]
    exact ⟨(hst _ _ ‹_›).2.1.1 ‹_›, hns, hfin, hlt⟩
  case provHit | buildOk =>
    rw [started_cons (fun _ => ⟨by simp, by simp⟩)]
    exact ⟨(hst _ _ ‹_›).2.1.1 (by simp [‹_ = St.open›, St.live]), hns, hfin, hlt⟩
  case grun f φ e e' γ t g hf hc hstep hg hop =>
    obtain ⟨o, ho, hl, _⟩ := (I.groups γ g hg).1 hop
    exact (hst _ _ ho).2.1.1 hl
  case parStart f φ t ts ks s hf hc hs =>
    obtain ⟨o, ho, hl⟩ := I.cur_live hf
    exact (hst _ _ ho).2.1.1 hl
  case finCode f φ π s o i t fs hf hc hs ho hph hfs hst' => exact good_run I ho hfs hst'
  case finObs f φ π s o i m Ly fs mm e hf hc hs ho hph hfs hm he hobs hst' =>
    exact good_run I ho hfs hst'
  case finObsLast f φ π s o i m Ly fs mm e oσ hf hc hs ho hph hfs hm he hobs hne hσ hσo hst' hb =>
    exact ⟨(hst _ _ hσ).1.1 hσo, good_run I ho hfs hst'⟩
  case fins f φ π s o hf hc hs ho hph hH hP hK hW hst' =>
    have hπ0 := (I.frames f φ hf (.closing π) (by simp [hs])).closing.1
    refine ⟨Classical.byContradiction fun h => ?_, (hst _ _ ho).2.1.1 (by simp [hst', St.live]),
      kidsDone_of I (I.noRun hπ0 ho hst' hph hH hK hW) fun o' ho' => ?_⟩
    · have := (hst _ _ ho).1.2 h; rw [hst'] at this; cases this
    · rw [ho] at ho'; cases ho'; exact hP
  case closed f φ π s o hf hc hs ho hph hfs hst' =>
    obtain ⟨h1, h2, h3⟩ := hst _ _ ho
    refine ⟨Classical.byContradiction fun h => ?_, fun h => ?_, fun i hi => ?_⟩
    · have := h2.2 h; simp [hst', St.live] at this
    · have := h3.2 h; rw [hst'] at this; cases this
    · rcases I.finAll π o i ho hi with ⟨t, ht, _⟩ | hr
      · have hF := (I.empty π o ho (by simp [hst', St.live])).2.2.2.2
        simp [hF, hfs] at ht
      · exact hr

end Flick.RT
