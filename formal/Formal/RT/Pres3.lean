import Formal.RT.Pres1

/-! Preservation of the invariants about `F` and `fs` (R1, R2). -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

theorem step_empty (I : Inv C) (hs : Step K L C C') :
    ∀ π o, C'.Λ π = some o → o.st.live = false →
      o.K = [] ∧ o.W = [] ∧ o.P = [] ∧ o.H = [] ∧ o.F = [] := by
  have hJ := I.empty
  have hcur : ∀ j φ, C.Φ j = some (.run φ) → ∀ o, C.Λ φ.cur = some o → o.st.live = true :=
    fun j φ h o ho => by obtain ⟨o', h', hl⟩ := I.cur_live h; rw [ho] at h'; cases h'; exact hl
  have hJf : ∀ (o : Owner T V) π, o.st = .finalizing → C.Λ π = some o →
      o.K = [] ∧ o.W = [] ∧ o.P = [] ∧ o.H = [] ∧ o.F = [] :=
    fun o π hst ho => hJ π o ho (by simp [hst, St.live])
  cases hs <;> intro π' o' h' <;> owner_cases h' <;>
    first
    | exact hJ _ _ h'
    | (simp [Owner.fresh, St.live]; done)
    | (intro hl; simp_all [St.live]; done)
    | (intro hl; have := hJf _ _ ‹_› ‹_›; simp_all; done)
    | (intro hl; have := hJ _ _ ‹_› (by simp_all [St.live]); simp_all; done)
    | (intro hl; have := hcur _ _ ‹_› _ ‹_›; simp_all; done)
    | skip

theorem step_finSorted [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ π o, C'.Λ π = some o → (o.F ++ o.fs).Pairwise (fun a b => b.id < a.id) := by
  have hJ := I.finSorted
  have hpop : ∀ π (o : Owner T V) t fs, o.fs = t :: fs → C.Λ π = some o →
      (o.F ++ fs).Pairwise (fun a b => b.id < a.id) := fun π o t fs hfs ho =>
    (hJ π o ho).sublist (by rw [hfs]; exact List.Sublist.append_left (List.sublist_cons_self t fs) _)
  have hpush : ∀ π (o : Owner T V) a, C.Λ π = some o →
      ((⟨C.n, a⟩ :: o.F) ++ o.fs).Pairwise (fun a b => b.id < a.id) := fun π o a ho => by
    rw [List.cons_append, List.pairwise_cons]
    exact ⟨fun b hb => I.fin_lt ho hb, hJ π o ho⟩
  cases hs <;> intro π' o' h' <;> owner_cases h' <;>
    first
    | exact hJ _ _ h'
    | (simp [Owner.fresh]; done)
    | (dsimp only; exact hJ _ _ ‹_›)
    | exact hpush _ _ _ ‹_›
    | exact hpop _ _ _ _ ‹_› ‹_›
    | (dsimp only; exact hpop _ _ _ _ ‹_› ‹_›)
    | skip
  case fins.isTrue =>
    simp only [h.g7a_order, List.nil_append]
    have h1 := hJ _ _ ‹_›
    rw [I.fsNil _ _ ‹_› (by simp_all), List.append_nil] at h1
    exact h1

theorem step_finAll [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ π o i, C'.Λ π = some o → .fin π i ∈ C'.J → (∃ t ∈ o.F ++ o.fs, t.id = i) ∨ .run π i ∈ C'.J := by
  have hJ := I.finAll
  have hfr : ∀ i, Ev.fin C.n i ∉ C.J := fun i => I.fresh_ev (by simp [Ev.ids])
  have hfs := I.fsNil
  cases hs <;> intro π' o' i h' <;> owner_cases h' <;>
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.fin.injEq, Ev.run.injEq] <;>
    first
    | exact hJ _ _ _ h'
    | (dsimp only; exact hJ _ _ _ ‹_›)
    | (clear I; subst_vars; simp only [h.g7a_order, List.cons_append, List.mem_cons, or_and_right,
        exists_or, exists_eq_left] at *; grind)

theorem step_finIn [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ π o t, C'.Λ π = some o → t ∈ o.F ++ o.fs → .fin π t.id ∈ C'.J ∧
      ¬ Started t.id C'.J ∧ ∀ j, .run π j ∈ C'.J → t.id < j := by
  have hJ := I.finIn
  have hfr : ∀ i, Ev.fin C.n i ∉ C.J := fun i => I.fresh_ev (by simp [Ev.ids])
  have hfr' : ∀ π, Ev.run π C.n ∉ C.J ∧ Ev.now π C.n ∉ C.J :=
    fun π => ⟨I.fresh_ev (by simp [Ev.ids]), I.fresh_ev (by simp [Ev.ids])⟩
  have hfs := I.fsNil
  have hrf : ∀ π j, Ev.run π j ∈ C.J → Ev.fins π ∈ C.J := fun _ _ h => I.run_fins h
  have hst := I.stJ
  have hun : ∀ π π' i, Ev.fin π i ∈ C.J → Ev.fin π' i ∈ C.J → π = π' :=
    fun _ _ _ h h' => I.fin_unique h h'
  have hlt : ∀ π (o : Owner T V) t, C.Λ π = some o → t ∈ o.F ++ o.fs → t.id < C.n :=
    fun _ _ _ h h' => I.fin_lt h h'
  have hemp : ∀ π (o : Owner T V), C.Λ π = some o → o.st = .finalizing → o.F = [] :=
    fun π o h h' => (I.empty π o h (by simp [h', St.live])).2.2.2.2
  have hpair : ∀ π (o : Owner T V) t fs, C.Λ π = some o → o.fs = t :: fs →
      ∀ b ∈ fs, b.id < t.id := fun π o t fs h h' b hb => by
    have := I.finSorted π o h
    rw [h', List.pairwise_append, List.pairwise_cons] at this
    exact this.2.1.1 b hb
  have hJh : ∀ π (o : Owner T V) t fs, C.Λ π = some o → o.fs = t :: fs →
      Ev.fin π t.id ∈ C.J := fun π o t fs h h' => (hJ π o t h (by simp [h'])).1
  clear I
  cases hs <;> intro π' o' t' h' <;> owner_cases h' <;>
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.fin.injEq, Ev.run.injEq, Ev.now.injEq,
      Started, h.g7a_order, List.cons_append, List.nil_append, Owner.fresh] at * <;>
    first
    | exact hJ _ _ _ h'
    | (dsimp only; exact hJ _ _ _ ‹_›)
    | (subst_vars; grind [St.live])

end Flick.RT
