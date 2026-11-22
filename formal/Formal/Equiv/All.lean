import Formal.Equiv.StuckCfg
import Formal.Equiv.Context

/-!
# Equivalence of every run

Two terms are equivalent for every run (`CtxEquivAll`) when every two typed programs one context
builds around them have the same outcomes (`Outcome`): each ends with the same literals, has a
main thread that dies, runs forever or gets stuck exactly when the other does. This is the
statement of `docs/formal/lazy.md` §6 for O1a and O1c (`o1a_all`, `o1c_all`); O1b holds only of
the literals (`o1b`), as two threads that force one `lazy v` at once may die where `born v` does
not.

The relation of the runs of the two programs (`RunR`: related configurations of typed programs,
without the pairs of O1b) answers every outcome (`everySim`): a step by `cstep_every`, whose
answers without a step lower the potential of the left configuration (`cpot`); a literal by
`lit_sim`; a dying main thread by `dies_sim`; and a stuck configuration by `stuck_sim`.
-/

namespace Flick.Equiv

/-- What the runs of a program do. -/
def Runs (root : Roots) (e : Tm) (o : Outcome) : Prop := Has root ⟨[], [], e⟩ o

/-- **Equivalence of every run**: every two typed programs one context builds around `e₁` and
`e₂` have the same outcomes. -/
def CtxEquivAll (S : Sig) (e₁ e₂ : Tm) : Prop :=
  ∀ root C₁ C₂, Ctx e₁ e₂ C₁ C₂ → Prog S root C₁ → Prog S root C₂ →
    ∀ o, Runs root C₁ o ↔ Runs root C₂ o

/-- Equivalence of every run gives the same literals. -/
theorem CtxEquivAll.ctxEquiv {S : Sig} {e₁ e₂ : Tm} (h : CtxEquivAll S e₁ e₂) :
    CtxEquiv S e₁ e₂ :=
  fun root C₁ C₂ hc p₁ p₂ o => h root C₁ C₂ hc p₁ p₂ (.lit o)

/-- Related configurations that two typed programs reach, without the pairs of O1b. -/
def RunR (S : Sig) (root : Roots) (W : World) (c₁ c₂ : Cfg) : Prop :=
  CfgR W c₁ c₂ ∧ W.R = root ∧ ¬ W.B ∧ (∃ e, Prog S root e ∧ CSteps root ⟨[], [], e⟩ c₁) ∧
    ∃ e, Prog S root e ∧ CSteps root ⟨[], [], e⟩ c₂

theorem Prog.typed {S : Sig} {root : Roots} {e : Tm} {c : Cfg} (h : Prog S root e)
    (hs : CSteps root ⟨[], [], e⟩ c) : CfgTyped S c := by
  obtain ⟨hroot, -, hb, t, φ, h₀, ht, hφ⟩ := h
  obtain ⟨X, _, hm, hΛ, -⟩ := steps_typed hroot hs ht hφ (CfgTy.init h₀ hb)
  refine ⟨⟨_, _, _, _, hm⟩, fun p g hg k hk => ?_⟩
  obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
  exact ⟨_, _, _, _, hk'⟩

/-- **The runs of typed programs, related, answer every outcome.** -/
theorem everySim {S : Sig} {root : Roots} (hroot : root.Src) : EverySim root (RunR S root) cpot where
  step := by
    intro W c₁ c₂ l c₁' ⟨hr, hR, hB, ⟨e₁, p₁, hs₁⟩, ⟨e₂, p₂, hs₂⟩⟩ hs
    obtain ⟨W', c₂', hW, hr', h⟩ :=
      cstep_every hroot hr hR (p₁.reach.step hs₁) (p₂.reach.step hs₂) hs
    have hR' : W'.R = root := hW.R.symm.trans hR
    have hB' : ¬ W'.B := fun h => hB (hW.B.2 h)
    rcases h with ⟨l', c, s, hc⟩ | ⟨rfl, hd⟩
    · exact ⟨W', c₂', ⟨hr', hR', hB', ⟨e₁, p₁, hs₁.trans (.step hs .refl)⟩,
        ⟨e₂, p₂, hs₂.trans (.step s hc)⟩⟩, .inl ⟨l', c, s, hc⟩⟩
    · exact ⟨W', _, ⟨hr', hR', hB', ⟨e₁, p₁, hs₁.trans (.step hs .refl)⟩, ⟨e₂, p₂, hs₂⟩⟩,
        .inr ⟨rfl, hd hB⟩⟩
  lit := fun ⟨hr, _, hB, _⟩ => lit_sim hr (fun h => (hB h).elim)
  dies := fun ⟨hr, _, hB, _⟩ => dies_sim hr hB
  stuck := fun ⟨hr, _, hB, ⟨_, p₁, hs₁⟩, _⟩ hs =>
    stuck_sim _ (Nat.lt_succ_self _) hr hB (p₁.typed hs₁) hs

/-- **Terms related both ways without the pairs of O1b are equivalent for every run**, when one
makes or forces a lazy cell at its top. -/
theorem ctxEquivAll_of_sim {S : Sig} {e₁ e₂ : Tm}
    (h₁₂ : ∀ root, Sim (World.empty root False) .any e₁ e₂)
    (h₂₁ : ∀ root, Sim (World.empty root False) .any e₂ e₁)
    (hz : e₁.LzShape ∨ e₂.LzShape) (s₁ : e₁.Src) (s₂ : e₂.Src) : CtxEquivAll S e₁ e₂ := by
  intro root C₁ C₂ hc p₁ p₂ o
  have r₁₂ : RunR S root (World.empty root False) ⟨[], [], C₁⟩ ⟨[], [], C₂⟩ :=
    ⟨⟨.inl (hc.sim (h₁₂ root) hz s₂ (by decide) p₁.noLzTop p₂.noLzTop), StR.empty root False,
      ⟨rfl, fun _ _ _ h => by simp at h⟩⟩, rfl, id, ⟨C₁, p₁, .refl⟩, ⟨C₂, p₂, .refl⟩⟩
  have r₂₁ : RunR S root (World.empty root False) ⟨[], [], C₂⟩ ⟨[], [], C₁⟩ :=
    ⟨⟨.inl (hc.symm.sim (h₂₁ root) hz.symm s₁ (by decide) p₂.noLzTop p₁.noLzTop),
      StR.empty root False, ⟨rfl, fun _ _ _ h => by simp at h⟩⟩, rfl, id, ⟨C₂, p₂, .refl⟩,
      ⟨C₁, p₁, .refl⟩⟩
  exact ⟨(everySim p₁.2.1).has r₁₂ o, (everySim p₁.2.1).has r₂₁ o⟩

/-- **O1a for every run**: a cell nothing names is not made. -/
theorem o1a_all {S : Sig} {e c : Tm} (he : e.Src) (hc : c.Src) :
    CtxEquivAll S (.let_ (.lazy e) (c.rename (· + 1))) c :=
  ctxEquivAll_of_sim (fun _ => .dropA rfl (Sim.refl hc _ (by decide)))
    (fun _ => .dropC rfl hc.noIns (Sim.refl hc _ (by decide))) (.inl trivial) ⟨he, hc.rename _⟩ hc

/-- **O1c for every run**: a cell forced where it is made is not made. -/
theorem o1c_all {S : Sig} {e : Tm} (he : e.Src) : CtxEquivAll S (.force (.lazy e)) e :=
  ctxEquivAll_of_sim (fun _ => .forceA (Sim.refl he _ (by decide)))
    (fun _ => .forceC he.noIns (Sim.refl he _ (by decide))) (.inl trivial) he he

end Flick.Equiv
