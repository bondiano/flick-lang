import Formal.Regions
import Formal.T43Main
import Formal.LiveRules

/-!
# T4′: the safety a discipline of use promises (spike)

`T4` promises two things of every configuration a program reaches: no thread is at a step
that needs a region that is not live (T4.1, with T4.3 for handlers), and nothing that can
still step *reaches* a region that is not live (T4.2). The discipline where a region is
needed alive only where it is used (`flick-model/src/check/regions/live.rs`) keeps the first
and gives up the second on purpose: a closure or a task that holds a capability and never
uses it may outlive the region, and the machine runs it without a use after close.

`SafeUse Ok` is the first half as a property of a judgment `Ok` of closed programs. It is
what T4′ states of the live discipline. `T4'_captures` proves it of the current judgment
`Typ` from T4 and T4.3: T4′ is weaker than T4, so a program the current rules accept keeps
every guarantee T4′ promises.

`TypLive` is now defined in `Rules.lean` as `TypM S .live`; `Typ` is the unchanged
captures instance. `Live C` is `Row.ofCaps C`, using the existing atoms of `Scope s`,
so (Scope) discharges it without a new row element. Live drops the held-value bound
from (Lam), (Fix), (Letrec), (Task) and (Drec), while a runtime call or await needs the
qualifier in its stage (`TypingMode.use`). A qualified body is checked at that same
stage. `op` needs `Scope s`, and work an owner keeps needs every region atom of its row
to outlive the owner. `LeLive` compares use stages, so subtyping cannot erase Live by
hiding a qualifier in an argument or a result type. `LiveRules.lean` checks these rules,
including a source scope that returns a closure holding its capability without using it.
Control arms and their runtime frames add the handler's own region locally, so `resume`
can use it without leaking that region into the handler's outer row.

The lemmas type soundness rests on now hold for both instances: values are pure and
have canonical forms, L3 types a frame and a context, and progress shows every closed term
to be a value or one of the outcomes `NV` with its effect among the services of its
stage. What remains is preservation for `TypLive` and the invariant replacing
`Nest.reach`: every region atom of a running stage is live, including the stages of child
work and handler frames. Preservation of that invariant is where `T4'_live` must be
proved. Neither the examples nor the existing captures proofs establish it. Translation of
live signature elision into the core judgment is a separate remaining obligation.

T4.2 matters beyond safety: the runtime contract (`Formal/RT/`) takes only T4.3 (as `G11`),
so giving T4.2 up does not touch T5; what it costs is that a value may keep a closed
capability reachable (memory, not use).
-/

namespace Flick

variable {S : Sig}

/-- T4′ of a judgment `Ok` of closed programs: in every configuration a program it accepts
reaches, no thread (the main thread or a child of a live region) is at a step that needs a
region that is not live (T4.1), and no operation is answered by a handler of a region that
ended (T4.3). Unlike T4, nothing is said of what threads *reach*. -/
def SafeUse (root : Roots) (Ok : Tm → Ty → Stage → Prop) : Prop :=
  ∀ e₀ t φ₀ c, Ok e₀ t φ₀ → e₀.Below 0 → t.YF 0 → φ₀.substT (kill 0) = φ₀ →
    CSteps root ⟨[], [], e₀⟩ c →
    (∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids, ∀ E r,
      OkE E → k = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ i e E Ef o k v q, c.at i = some e → OkE E → e = plug E (.app (.const (.perform Ef o k)) v) →
      handlerOf E Ef o k = some q → Live c.rgs q)

/-- The current rules satisfy T4′: it follows from T4 and T4.3. -/
theorem T4'_captures {root : Roots} (hroot : RootTy S root) :
    SafeUse root (fun e t φ => Typ S [] Facts.none [] e (.ex t φ)) := by
  intro e₀ t φ₀ c h₀ hp ht hφ hs
  obtain ⟨h1, h2, -, -⟩ := T4 hroot h₀ hp ht hφ hs
  exact ⟨h1, h2, T4_3 hroot h₀ hp ht hφ hs⟩

/-- T4′ is closed under a smaller judgment: a discipline that accepts less keeps it. -/
theorem SafeUse.mono {root : Roots} {Ok Ok' : Tm → Ty → Stage → Prop}
    (h : SafeUse root Ok) (hle : ∀ e t φ, Ok' e t φ → Ok e t φ) : SafeUse root Ok' :=
  fun e₀ t φ₀ c h₀ => h e₀ t φ₀ c (hle _ _ _ h₀)

end Flick

/-- info: 'Flick.T4'_captures' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms Flick.T4'_captures
