import Formal.RT.Pres1

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

theorem step_stJ (I : Inv C) (hs : Step K L C C') :
    ∀ π o, C'.Λ π = some o → (o.st = .open ↔ .close π ∉ C'.J) ∧
      (o.st.live = true ↔ .fins π ∉ C'.J) ∧ (o.st = .closed ↔ .closed π ∈ C'.J) := by
  have hJ := I.stJ
  have hc : Ev.close C.n ∉ C.J := I.fresh_ev (by simp [Ev.ids])
  have hf : Ev.fins C.n ∉ C.J := I.fresh_ev (by simp [Ev.ids])
  have hcl : Ev.closed C.n ∉ C.J := I.fresh_ev (by simp [Ev.ids])
  cases hs
  case finObsLast f φ π s o i m Ly fs mm e oσ hf hc hs ho hph hfs hm he hobs hne hσ hσo hst hb =>
    intro π' o' h'
    simp only [upd] at h'
    simp only [List.mem_cons, reduceCtorEq, false_or, Ev.close.injEq, Ev.fins.injEq,
      Ev.closed.injEq]
    have A := hJ _ _ hσ
    have B := hJ _ _ ho
    by_cases h1 : π' = e.σ
    · subst h1; simp only [if_true, Option.some.injEq] at h'; subst h'
      obtain ⟨_, A2, A3⟩ := A; rw [hσo] at A2 A3
      simp only [St.live, true_iff] at A2
      simp only [reduceCtorEq, false_iff] at A3
      simp [St.live, A2, A3]
    · by_cases h2 : π' = π
      · subst h2; simp only [h1, if_true, if_false, Option.some.injEq] at h'; subst h'
        simp_all [St.live]
      · simp only [h1, h2, if_false] at h'
        have := hJ _ _ h'; simp_all
  all_goals intro π' o' h' <;> (try simp only [upd, updO] at h') <;> (repeat' split at h') <;>
    (try simp only [Option.some.injEq, Option.map_some, Option.map_none, reduceCtorEq,
      Option.map_eq_some_iff] at h') <;>
    (try obtain ⟨o₀, h₀, h'⟩ := h') <;> (try subst h') <;> simp only [List.mem_cons, reduceCtorEq, false_or, Ev.close.injEq,
      Ev.fins.injEq, Ev.closed.injEq] <;>
    first
    | exact hJ _ _ h'
    | (subst_vars; simp [Owner.fresh, St.live, hc, hf, hcl]; done)
    | (obtain ⟨h1, h2, h3⟩ := hJ _ _ ‹_›; simp_all [St.live, Owner.fresh])

theorem step_root (I : Inv C) (hs : Step K L C C') : ∃ o, C'.Λ 0 = some o ∧ o.st = .open := by
  obtain ⟨o, ho, hst⟩ := I.root
  have hz : ∀ f φ π s, C.Φ f = some (.run φ) → φ.stack = .closing π :: s → π ≠ 0 :=
    fun f φ π s hf hs => (I.frames f φ hf (.closing π) (by simp [hs])).closing.1
  have hzi : ∀ f φ π env s, C.Φ f = some (.run φ) → φ.stack = .inside π env :: s → π ≠ 0 :=
    fun f φ π env s hf hs => (I.frames f φ hf (.inside π env) (by simp [hs])).inside.1
  have hzb : ∀ f φ πp m Ly σ Ls b s, C.Φ f = some (.run φ) →
      φ.stack = .build πp m Ly σ Ls b :: s → σ ≠ 0 :=
    fun f φ πp m Ly σ Ls b s hf hs => (I.frames f φ hf (.build πp m Ly σ Ls b) (by simp [hs])).build.1
  have hze : ∀ m mm Ly (e : Ent V), C.M m = some mm → mm.ents Ly = some e → e.σ ≠ 0 :=
    fun m mm Ly e hm he => (I.ents m mm Ly e hm he).1
  cases hs <;> simp only [upd, updO]
  case close => have := hzi _ _ _ _ _ ‹_› ‹_›; simp [Ne.symm this, ho, hst]
  case fins => have := hz _ _ _ _ ‹_› ‹_›; simp [Ne.symm this, ho, hst]
  case closed => have := hz _ _ _ _ ‹_› ‹_›; simp [Ne.symm this, ho, hst]
  case finObsLast =>
    have h1 := hz _ _ _ _ ‹_› ‹_›
    have h2 := hze _ _ _ _ ‹_› ‹_›
    simp [Ne.symm h1, Ne.symm h2, ho, hst]
  case buildFail => have := hzb _ _ _ _ _ _ _ _ _ ‹_› ‹_›; simp [Ne.symm this, ho, hst]
  all_goals first
    | exact ⟨o, ho, hst⟩
    | (split <;> simp_all [Owner.fresh])

theorem step_fsNil (I : Inv C) (hs : Step K L C C') :
    ∀ π o, C'.Λ π = some o → o.st ≠ .finalizing → o.fs = [] := by
  have hJ := I.fsNil
  cases hs <;> intro π' o' h' <;> owner_cases h' <;>
    first
    | exact hJ _ _ h'
    | (simp [Owner.fresh]; done)
    | (simp only; exact hJ _ _ ‹_›)
    | (simp only; intro h''; exact absurd rfl h'')
    | (simp only; intro h''; simp_all; done)
    | (simp only; intro; apply hJ _ _ ‹_›; simp_all)

theorem step_regJ [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ π i, .reg π i ∈ C'.J → .fin π i ∈ C'.J ∨ .now π i ∈ C'.J := by
  have hJ := I.regJ
  cases hs <;> simp only [List.mem_cons, reduceCtorEq, false_or, Ev.reg.injEq, Ev.fin.injEq,
    Ev.now.injEq] <;>
    first
    | exact hJ
    | (intro π i hr; rcases hr with ⟨rfl, rfl⟩ | hr
       · simp
       · rcases hJ π i hr with h1 | h1 <;> simp [h1])
    | (exfalso; simp_all [h.g7a_late, h.g7b]; done)
    | (intro π i hr; rcases hJ π i hr with h1 | h1 <;> simp [h1])

theorem step_defectJ (I : Inv C) (hs : Step K L C C') : ∀ c, .defect c ∈ C'.J → c ∈ C'.D := by
  have hJ := I.defectJ
  cases hs <;> simp only [List.mem_cons, reduceCtorEq, false_or, Ev.defect.injEq] <;>
    first
    | exact hJ
    | (intro c hc; rcases hc with rfl | hc
       · simp
       · simp [hJ c hc])

end Flick.RT