import Formal.Equiv.StuckCfg
import Formal.Equiv.Context

/-!
# Equivalence of every run

Two terms are equivalent for every run (`CtxEquivAll`) when every two typed programs one context
builds around them have the same outcomes (`Outcome`): each ends with the same literals, has a
main thread that dies, runs forever or gets stuck exactly when the other does. This is the
statement of `docs/formal/lazy.md` §6 for O1a, O1c and O2a (`o1a_all`, `o1c_all`, `o2a_all`);
O1b holds only of the literals (`o1b`), as two threads that force one `lazy v` at once may die
where `born v` does not.

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
  CfgR W c₁ c₂ ∧ W.R = root ∧ ¬ W.B ∧
    (∃ e, Prog S root e ∧ e.NoSettle ∧ CSteps root ⟨[], [], e⟩ c₁) ∧
    ∃ e, Prog S root e ∧ e.NoSettle ∧ CSteps root ⟨[], [], e⟩ c₂

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
    intro W c₁ c₂ l c₁' ⟨hr, hR, hB, ⟨e₁, p₁, hns₁, hs₁⟩, ⟨e₂, p₂, hns₂, hs₂⟩⟩ hs
    obtain ⟨W', c₂', hW, hr', h⟩ :=
      cstep_every hroot hr hR ((p₁.reach hns₁).step hs₁) ((p₂.reach hns₂).step hs₂) hs
    have hR' : W'.R = root := hW.R.symm.trans hR
    have hB' : ¬ W'.B := fun h => hB (hW.B.2 h)
    rcases h with ⟨l', c, s, hc⟩ | ⟨rfl, hd⟩
    · exact ⟨W', c₂', ⟨hr', hR', hB', ⟨e₁, p₁, hns₁, hs₁.trans (.step hs .refl)⟩,
        ⟨e₂, p₂, hns₂, hs₂.trans (.step s hc)⟩⟩, .inl ⟨l', c, s, hc⟩⟩
    · exact ⟨W', _, ⟨hr', hR', hB', ⟨e₁, p₁, hns₁, hs₁.trans (.step hs .refl)⟩,
        ⟨e₂, p₂, hns₂, hs₂⟩⟩, .inr ⟨rfl, hd hB⟩⟩
  lit := fun ⟨hr, _, hB, _⟩ => lit_sim hr (fun h => (hB h).elim)
  dies := fun ⟨hr, _, hB, _⟩ => dies_sim hr hB
  stuck := fun ⟨hr, _, hB, ⟨_, p₁, _, hs₁⟩, _⟩ hs =>
    stuck_sim _ (Nat.lt_succ_self _) hr hB (p₁.typed hs₁) hs

/-- **Terms related both ways without the pairs of O1b are equivalent for every run**, when one
makes or forces a lazy cell at its top. -/
theorem ctxEquivAll_of_sim {S : Sig} {e₁ e₂ : Tm}
    (h₁₂ : ∀ root, Sim (World.empty root False) .any e₁ e₂)
    (h₂₁ : ∀ root, Sim (World.empty root False) .any e₂ e₁)
    (hz : e₁.LzShape ∨ e₂.LzShape) (s₁ : e₁.Src) (s₂ : e₂.Src) : CtxEquivAll S e₁ e₂ := by
  intro root C₁ C₂ hc p₁ p₂ o
  have hns₁ : C₁.NoSettle := (hc.symm.src s₁).noSettle
  have hns₂ : C₂.NoSettle := (hc.src s₂).noSettle
  have r₁₂ : RunR S root (World.empty root False) ⟨[], [], C₁⟩ ⟨[], [], C₂⟩ :=
    ⟨⟨.inl (hc.sim (h₁₂ root) hz s₂ (by decide) p₁.noLzTop p₂.noLzTop), StR.empty root False,
      ⟨rfl, fun _ _ _ h => by simp at h⟩⟩, rfl, id, ⟨C₁, p₁, hns₁, .refl⟩, ⟨C₂, p₂, hns₂, .refl⟩⟩
  have r₂₁ : RunR S root (World.empty root False) ⟨[], [], C₂⟩ ⟨[], [], C₁⟩ :=
    ⟨⟨.inl (hc.symm.sim (h₂₁ root) hz.symm s₁ (by decide) p₂.noLzTop p₁.noLzTop),
      StR.empty root False, ⟨rfl, fun _ _ _ h => by simp at h⟩⟩, rfl, id,
      ⟨C₂, p₂, hns₂, .refl⟩, ⟨C₁, p₁, hns₁, .refl⟩⟩
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

theorem _root_.Flick.Tm.Src.subst : ∀ {e : Tm}, e.Src → ∀ {σ : Nat → Tm}, (∀ i, (σ i).Src) →
    (e.subst σ).Src := by
  have lift : ∀ {σ : Nat → Tm}, (∀ i, (σ i).Src) → ∀ n i, (liftS n σ i).Src := fun hσ n i => by
    unfold liftS; split
    · trivial
    · exact (hσ _).rename _
  intro e
  induction e <;> intro h σ hσ <;> simp_all [Tm.Src, Tm.subst]

theorem subForce_var (i : Nat) : subForce (.var i) Tm.var = inst [.force (.var (i + 1))] := by
  funext j
  match j with
  | 0 => rfl
  | 1 => rfl
  | _ + 2 => rfl

theorem subVar_var : subVar Tm.var = inst [.var 0] := by
  funext j
  match j with
  | 0 => rfl
  | 1 => rfl
  | _ + 2 => rfl

/-- **O2a for every run**: forcing a cell already forced reuses its value. -/
theorem o2a_all {S : Sig} {i : Nat} {c : Tm} (hc : c.Src) :
    CtxEquivAll S
      (.let_ (.force (.var i)) (c.subst (inst [.force (.var (i + 1))])))
      (.let_ (.force (.var i)) (c.subst (inst [.var 0]))) := by
  rw [← subForce_var i, ← subVar_var]
  refine ctxEquivAll_of_sim
    (fun _ => .againF1 id hc (fun _ hy => .var hy) (.inl ⟨i, rfl⟩) (.inl ⟨i, rfl⟩)
      (fun _ _ hy => .var hy) (fun _ => by simp [Tm.NoIns, Tm.Has])
      (fun j => .inl ⟨j, rfl⟩) (fun j => .inl ⟨j, rfl⟩))
    (fun _ => .againR1 id hc (fun _ hy => .var hy) (.inl ⟨i, rfl⟩) (.inl ⟨i, rfl⟩)
      (fun _ _ hy => .var hy) (fun _ => by simp [Tm.NoIns, Tm.Has])
      (fun j => .inl ⟨j, rfl⟩) (fun j => .inl ⟨j, rfl⟩))
    (.inl trivial) ?_ ?_
  · exact ⟨trivial, hc.subst fun j => by match j with | 0 => trivial | 1 => trivial | _ + 2 => trivial⟩
  · exact ⟨trivial, hc.subst fun j => by match j with | 0 => trivial | 1 => trivial | _ + 2 => trivial⟩

end Flick.Equiv
