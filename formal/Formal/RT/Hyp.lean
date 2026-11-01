import Formal.RT.Step

/-!
# λRT: hypotheses about the libraries (runtime.md §5)

Г7(а)–(ж) are about Effect 4.0.5 and Г8, Г9 about Go and foreign code. Each is a field of
`Hyp` about the part of a step that `Lib` leaves open; without it the model has the rule
that breaks the property (`Step.addDrop`, `Step.acqLost`, `Local.parEarly`, …). `T5` takes
`Hyp` as a parameter, nothing here is an axiom.

Not here:

- Г7(в) (leaving a mask delivers a pending interruption at once) is about when an
  interruption arrives. The model has no mask and may deliver at any point of user code,
  so safety does not depend on it.
- the parts of Г8 and Г9 that are liveness (a Promise settles, the process is not killed)
  and Г10: R1–R7 speak about steps that happened.
- Г11 is about the program, not a library: `G11` (`Layers.lean`).
-/

namespace Flick.RT

variable {V : Type} [DecidableEq V]

structure Hyp (L : Lib V) : Prop where
  /-- Г7(а): `Scope.close` runs the stack from the top, each finalizer once -/
  g7a_order : ∀ {α : Type} (l : List α), L.closeOrder l = l
  /-- Г7(а): a finalizer added to a closed area runs at once -/
  g7a_late : L.lateRuns = true
  /-- Г7(б): `acquireRelease` registers under the mask of the acquisition -/
  g7b : L.acqAtomic = true
  /-- Г7(г): `forEachConcurrent` interrupts and waits for its branches -/
  g7g : L.joinWaits = true
  /-- Г7(д): the memo map finds a layer by identity -/
  g7d : L.memo = true
  /-- Г7(е): `causeCombine` and `causeFromReasons` are `⊕` and `⧺` of §2.1 -/
  g7e_combine : ∀ a b, L.combine a b = comb a b
  g7e_concat : ∀ a b, L.concat a b = a ++ b
  /-- Г7(ж): an interruption that skips a handler keeps the head or drops it -/
  g7zh : ∀ c, head (L.skip c) = none ∨ head (L.skip c) = head c
  /-- Г8: the mutex of a cell makes `Ref.modify` one step -/
  g8 : L.refAtomic = true
  /-- Г9: a foreign call is tracked by `P` of its owner -/
  g9 : L.extTracked = true

end Flick.RT
