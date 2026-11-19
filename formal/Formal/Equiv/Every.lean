import Formal.Equiv.CfgStep

/-!
# Equivalence of every run: outcomes, and a simulation that answers each of them

A run of a configuration ends with a literal of its main thread, ends with a defect of its main
thread, never ends, or gets stuck on something else (`Outcome`, `Has`). Two programs are
equivalent for every run when they have the same outcomes (`Formal/Equiv/All.lean`).

`EverySim` packs what a relation of the configurations of two runs must answer, so that each
outcome of the left run is one of the right run (`EverySim.has`): a step of the left side by steps
of the right side, or by none while a potential of the left configuration drops, so an endless left
run has infinitely many right steps; a literal, a defect and a stuck configuration by the same.
These are the run-level theorems; the relation of `Formal/Equiv/Sim.lean` provides them for O1a
and O1c (`everySim`, `Formal/Equiv/All.lean`), and a later rewrite can provide them for its own
relation.
-/

namespace Flick.Equiv

/-- What a run does in the end (`docs/formal/lazy.md` §6). -/
inductive Outcome
  /-- The main thread ends with a literal. -/
  | lit (o : Lit)
  /-- The main thread steps to a defect. -/
  | dies
  /-- The run never ends. -/
  | diverges
  /-- The run gets stuck on something other than a literal. -/
  | stuck

/-- The main thread steps to a defect. -/
def MainDies (root : Roots) (c : Cfg) : Prop := ∃ l, Step root l c.rgs c.mem c.main .die

/-- No thread steps, the main thread does not die and is no literal. -/
def Stuck (root : Roots) (c : Cfg) : Prop :=
  (∀ l c', ¬ CStep root l c c') ∧ ¬ MainDies root c ∧ ∀ o, c.main ≠ .lit o

/-- A run that never ends. -/
def Diverges (root : Roots) (c : Cfg) : Prop :=
  ∃ f : Nat → Cfg, f 0 = c ∧ ∀ n, ∃ l, CStep root l (f n) (f (n + 1))

/-- The outcomes of the runs of a configuration. -/
def Has (root : Roots) (c : Cfg) : Outcome → Prop
  | .lit o => ∃ Λ μ, CSteps root c ⟨Λ, μ, .lit o⟩
  | .dies => ∃ c', CSteps root c c' ∧ MainDies root c'
  | .diverges => Diverges root c
  | .stuck => ∃ c', CSteps root c c' ∧ Stuck root c'

/-- A set of configurations each of which steps into it has a run that never ends. -/
theorem Diverges.of_set {root : Roots} (S : Cfg → Prop) {c : Cfg} (h₀ : S c)
    (h : ∀ x, S x → ∃ l y, CStep root l x y ∧ S y) : Diverges root c := by
  let next : {x // S x} → {x // S x} := fun x =>
    ⟨Classical.choose (Classical.choose_spec (h x.1 x.2)),
      (Classical.choose_spec (Classical.choose_spec (h x.1 x.2))).2⟩
  let g : Nat → {x // S x} := fun n => Nat.rec ⟨c, h₀⟩ (fun _ x => next x) n
  exact ⟨fun n => (g n).1, rfl, fun n => ⟨Classical.choose (h (g n).1 (g n).2),
    (Classical.choose_spec (Classical.choose_spec (h (g n).1 (g n).2))).1⟩⟩

/-- What a relation of the configurations of two runs answers, with worlds `ω` and a potential
`P` of the left configuration. -/
structure EverySim (root : Roots) {ω : Type} (R : ω → Cfg → Cfg → Prop) (P : ω → Cfg → Nat) :
    Prop where
  /-- A step of the left side: steps of the right side, or none, and the potential drops. -/
  step : ∀ {w c₁ c₂ l c₁'}, R w c₁ c₂ → CStep root l c₁ c₁' →
    ∃ w' c₂', R w' c₁' c₂' ∧
      ((∃ l' c, CStep root l' c₂ c ∧ CSteps root c c₂') ∨ (c₂' = c₂ ∧ P w' c₁' < P w c₁))
  lit : ∀ {w Λ μ o c₂}, R w ⟨Λ, μ, .lit o⟩ c₂ → ∃ Λ₂ μ₂, CSteps root c₂ ⟨Λ₂, μ₂, .lit o⟩
  dies : ∀ {w c₁ c₂}, R w c₁ c₂ → MainDies root c₁ → ∃ c₂', CSteps root c₂ c₂' ∧ MainDies root c₂'
  stuck : ∀ {w c₁ c₂}, R w c₁ c₂ → Stuck root c₁ → ∃ c₂', CSteps root c₂ c₂' ∧ Stuck root c₂'

namespace EverySim

variable {root : Roots} {ω : Type} {R : ω → Cfg → Cfg → Prop} {P : ω → Cfg → Nat}

theorem csteps (h : EverySim root R P) {c₁ c₁' : Cfg} (hs : CSteps root c₁ c₁') :
    ∀ {w c₂}, R w c₁ c₂ → ∃ w' c₂', R w' c₁' c₂' ∧ CSteps root c₂ c₂' := by
  induction hs with
  | refl => exact fun hr => ⟨_, _, hr, .refl⟩
  | step s _ ih =>
    intro w c₂ hr
    obtain ⟨w₁, c₂₁, hr₁, hs₁⟩ := h.step hr s
    obtain ⟨w', c₂', hr', hs'⟩ := ih hr₁
    refine ⟨w', c₂', hr', ?_⟩
    rcases hs₁ with ⟨l', c, s', hs₁⟩ | ⟨rfl, -⟩
    · exact .step s' (hs₁.trans hs')
    · exact hs'

/-- Along a left run that never ends, the right side steps: the potential cannot drop forever. -/
theorem progress (h : EverySim root R P) {f : Nat → Cfg} (hf : ∀ n, ∃ l, CStep root l (f n) (f (n + 1))) :
    ∀ (k n : Nat) {w x}, P w (f n) < k → R w (f n) x →
      ∃ m w' x', R w' (f m) x' ∧ ∃ l' c, CStep root l' x c ∧ CSteps root c x' := by
  intro k
  induction k with
  | zero => intro _ _ _ hk; exact absurd hk (Nat.not_lt_zero _)
  | succ k ih =>
    intro n w x hk hr
    obtain ⟨l, s⟩ := hf n
    obtain ⟨w', x', hr', hs | ⟨rfl, hlt⟩⟩ := h.step hr s
    · exact ⟨n + 1, w', x', hr', hs⟩
    · exact ih (n + 1) (by omega) hr'

/-- **Every outcome of the left run is one of the right run.** -/
theorem has (h : EverySim root R P) {w : ω} {c₁ c₂ : Cfg} (hr : R w c₁ c₂) :
    ∀ o, Has root c₁ o → Has root c₂ o
  | .lit o, ⟨Λ, μ, hs⟩ => by
    obtain ⟨w', c₂', hr', hs'⟩ := h.csteps hs hr
    obtain ⟨Λ₂, μ₂, hs''⟩ := h.lit hr'
    exact ⟨Λ₂, μ₂, hs'.trans hs''⟩
  | .dies, ⟨c₁', hs, hd⟩ => by
    obtain ⟨w', c₂', hr', hs'⟩ := h.csteps hs hr
    obtain ⟨c₂'', hs'', hd'⟩ := h.dies hr' hd
    exact ⟨c₂'', hs'.trans hs'', hd'⟩
  | .stuck, ⟨c₁', hs, hd⟩ => by
    obtain ⟨w', c₂', hr', hs'⟩ := h.csteps hs hr
    obtain ⟨c₂'', hs'', hd'⟩ := h.stuck hr' hd
    exact ⟨c₂'', hs'.trans hs'', hd'⟩
  | .diverges, ⟨f, hf₀, hf⟩ => by
    subst hf₀
    refine Diverges.of_set (fun y => ∃ w n x, R w (f n) x ∧ CSteps root y x) ⟨w, 0, c₂, hr, .refl⟩ ?_
    rintro y ⟨w, n, x, hr, hy⟩
    cases hy with
    | step s hy => exact ⟨_, _, s, w, n, x, hr, hy⟩
    | refl =>
      obtain ⟨m, w', x', hr', l', c, s, hc⟩ := h.progress hf _ n (Nat.lt_succ_self _) hr
      exact ⟨l', c, s, w', m, x', hr', hc⟩

end EverySim

end Flick.Equiv
