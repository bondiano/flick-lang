import Formal.RT.Step

/-!
# λRT: hypotheses about the libraries (runtime.md §5)

G7(a)–(g) are about Effect 4.1.2 and G8, G9 about Go and foreign code. Each is a field of
`Hyp` about the part of a step that `Lib` leaves open; without it the model has the rule
that breaks the property (`Step.addDrop`, `Step.acqLost`, `Local.parEarly`, …). `T5` takes
`Hyp` as a parameter, nothing here is an axiom.

Not here:

- G7(c) (leaving a mask delivers a pending interruption at once) is about when an
  interruption arrives. The model has no mask and may deliver at any point of user code,
  so safety does not depend on it.
- the parts of G8 and G9 that are liveness (a Promise settles, the process is not killed)
  and G10: R1–R7 speak about steps that happened.
- G11 is about the program, not a library: `G11` below, over the reachable configurations.
-/

namespace Flick.RT

variable {V : Type} [DecidableEq V]

structure Hyp (L : Lib V) : Prop where
  /-- G7(a): `Scope.close` runs the stack from the top, each finalizer once -/
  g7a_order : ∀ {α : Type} (l : List α), L.closeOrder l = l
  /-- G7(a): a finalizer added to a closed area runs at once -/
  g7a_late : L.lateRuns = true
  /-- G7(b): `acquireRelease` registers under the mask of the acquisition -/
  g7b : L.acqAtomic = true
  /-- G7(d): `forEachConcurrent` interrupts and waits for its branches -/
  g7g : L.joinWaits = true
  /-- G7(e): the memo map finds a layer by identity -/
  g7d : L.memo = true
  /-- G7(f): `causeCombine` and `causeFromReasons` are `⊕` and `⧺` of §2.1 -/
  g7e_combine : ∀ a b, L.combine a b = comb a b
  g7e_concat : ∀ a b, L.concat a b = a ++ b
  /-- G7(d), G7(f): `causeElectDefects` is `elect` of §2.3 -/
  g7e_elect : ∀ c, L.elect c = elect c
  /-- G7(g): an interruption that skips a handler keeps the head or drops it -/
  g7zh : ∀ c, head (L.skip c) = none ∨ head (L.skip c) = head c
  /-- G8: the mutex of a cell makes `Ref.modify` one step -/
  g8 : L.refAtomic = true
  /-- G9: a foreign call is tracked by `P` of its owner -/
  g9 : L.extTracked = true

section
variable {T : Type}

/-- `→*` -/
inductive Steps (K : Core T V) (L : Lib V) : Conf T V → Conf T V → Prop
  | refl {C} : Steps K L C C
  | step {C C' C''} : Steps K L C C' → Step K L C' C'' → Steps K L C C''

/-- A configuration reachable from the program `e₀`. -/
def Reach (K : Core T V) (L : Lib V) (e₀ : T) (C : Conf T V) : Prop :=
  Steps K L (init e₀ K.unit) C

/-- **G11**, the liveness premise of `provide` (`runtime.md`): at an `ask_S`, the environment entry that answers it — the
`provide` owner `πₚ` — is live (before its (C5)). Only the answering entry counts: a child
may carry an entry it never asks (`docs/formal/runtime.md#the-provide-liveness-premise-is-stronger-than-typing-gives`). It is the typing rule of `regions.md` §2.7, point 3
of T4, proved as `T4_3`; `Live` of the region of the `with` frame `lookupE` finds. -/
def G11 (K : Core T V) (L : Lib V) (e₀ : T) : Prop :=
  ∀ C, Reach K L e₀ C → ∀ f φ e e' π, C.Φ f = some (.run φ) → φ.code = .tm e →
    K.step e (.use π) e' → π ∈ φ.env → ∃ o, C.Λ π = some o ∧ o.st.live = true

end

end Flick.RT
