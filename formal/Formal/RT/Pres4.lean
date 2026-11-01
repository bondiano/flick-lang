import Formal.RT.NoRun

/-! Preservation of the invariants about running fibers (R3). -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

set_option maxHeartbeats 4000000 in
theorem step_live (I : Inv C) (hs : Step K L C C') :
    ∀ j φ, C'.Φ j = some (.run φ) → ∃ o, C'.Λ φ.base = some o ∧ o.st.live = true := by
  have hlive := I.live
  have hcur : ∀ j (φ : Fiber T V), C.Φ j = some (.run φ) → ∃ o, C.Λ φ.cur = some o ∧ o.st.live = true :=
    fun _ _ h => I.cur_live h
  have hgr : ∀ γ g, C.G γ = some g → g.isOpen = true →
      ∃ o, C.Λ g.owner = some o ∧ o.st.live = true := fun γ g h ho =>
    let ⟨o, h1, h2, _⟩ := (I.groups γ g h).1 ho; ⟨o, h1, h2⟩
  have hnr : ∀ π (o : Owner T V), π ≠ 0 → C.Λ π = some o → o.st = .closing → o.ph = .hooks true →
      o.H = [] → o.K = [] → o.W = [] → ∀ j ψ, C.Φ j = some (.run ψ) → ψ.base ≠ π :=
    fun _ _ h0 ho h1 h2 h3 h4 h5 => I.noRun h0 ho h1 h2 h3 h4 h5
  have hz : ∀ f (φ : Fiber T V) π s, C.Φ f = some (.run φ) → φ.stack = .closing π :: s → π ≠ 0 :=
    fun f φ π s hf hs => (I.frames f φ hf (.closing π) (by simp [hs])).closing.1
  have hlt : ∀ π (o : Owner T V), C.Λ π = some o → π < C.n := fun _ _ h => I.ltΛ h
  have hloc : ∀ (φ φ' : Fiber T V), Local K L C φ φ' → φ'.base = φ.base :=
    fun _ _ h => h.same.1
  clear I
  cases hs <;> intro j ψ h' <;> fiber_cases h' <;> simp only [upd, updO] <;>
    first
    | (subst_vars; grind [St.live, Owner.fresh])


theorem ParHas.parStart {j π base n : Nat} {ks : List Nat} {s : List (Frame T V)}
    (h : ParHas j π base (.par ks :: s)) : ParHas j π base (.par (n :: ks) :: s) := by
  rcases h with ⟨h1, h2⟩ | h
  · exact .inl ⟨List.mem_cons_of_mem _ h1, h2⟩
  · exact .inr h

set_option maxHeartbeats 4000000 in
theorem step_guard [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ j φ, C'.Φ j = some (.run φ) → Guard C' j φ := by
  have hg := I.guard
  have hlt : ∀ j (s : FState T V), C.Φ j = some s → j < C.n := fun _ _ h => I.ltΦ h
  have hloc : ∀ (φ φ' : Fiber T V), Local K L C φ φ' → φ'.base = φ.base ∧ φ'.role = φ.role :=
    fun _ _ h => ⟨h.same.1, h.same.2.1⟩
  have hlp : ∀ (φ φ' : Fiber T V) j π, Local K L C φ φ' → ¬ Done C.Φ j →
      ParHas j π φ.base φ.stack → ParHas j π φ'.base φ'.stack :=
    fun _ _ _ _ hl hd hp => hl.parHas h.g7g hd hp
  have hcons : ∀ j π base (s : List (Frame T V)) fr, ParHas j π base s →
      ParHas j π base (fr :: s) := fun _ _ _ _ fr h => h.cons fr
  have hpop : ∀ j π base (s : List (Frame T V)) fr, (∀ ks, fr ≠ .par ks) →
      ParHas j π base (fr :: s) → ParHas j π base s := fun _ _ _ _ _ h1 h2 => h2.pop h1
  have hps : ∀ j π base n ks (s : List (Frame T V)), ParHas j π base (.par ks :: s) →
      ParHas j π base (.par (n :: ks) :: s) := fun _ _ _ _ _ _ h => h.parStart
  obtain ⟨hf1, hf2, hf3, -, -⟩ := I.fresh C.n (Nat.le_refl _)
  obtain ⟨hf1', hf2', hf3', -, -⟩ := I.fresh (C.n + 1) (by omega)
  clear I
  cases hs <;> intro j ψ h' <;> fiber_cases h' <;> simp only [Guard, upd, updO] at hg ⊢ <;>
    first
    | (subst_vars; grind [Done, ParHas, Fiber.cur, curOf])

end Flick.RT
