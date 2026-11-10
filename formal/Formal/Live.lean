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
have canonical forms, L3 types a frame and a context, progress shows every closed term
to be a value or one of the outcomes `NV` with its effect among the services of its
stage, and preservation holds of a pure step (`PresPure.lean`), of refilling a handler's
hole (`PresHdl.lean`) and of the run-time steps of handlers at the redex
(`PresHdlRt.lean`). Findings from that port:

* A value may be retyped at any stage, so the frame of a finished handler loses nothing
  when it gives its value back (`hdoneVal`).
* The step that closes a handler's region (`hwithCatch`) keeps its answer at a stage
  that holds that region's atom, which only the frame supplies. In the live discipline
  the step therefore carries the atom as a premise (`TypingMode.use_rg_le`), and that
  premise is what the invariant replacing `Nest.reach` must supply: every region atom of
  a running stage is live, including the stages of child work and handler frames.
* A use site needs the qualifier of what it uses in its stage, and a context that fixes
  that stage — `scope`'s rows bound its body, `Scope.run`'s desugaring (`scopeRun_body`,
  `ScopeRun.lean`) creates one — must then hold the qualifier's atoms too. In the
  capture instance the qualifier is erased at exactly those points, so nothing is asked;
  in the live instance the lemma must carry `ofCaps C ⊆ ψ` of the function it runs. For
  `Scope.run` that fact is *not* derivable: the constant's instance `ι` carries a
  qualifier `ι.C` that the subtype of its argument constrains only inside the
  parameter's own rows (`LeLive.arrow_use` gives `ofCaps C_f ⊆ ψ₀ ∪ ofCaps ι.C`, not
  `⊆ ψ₀`), and `Const.ok` said nothing of `.scopeRun` before rule 2 below.

Four rule-level changes close the proofs. They are contract changes to the live instance
rather than proof steps; the capture instance is unchanged by each of them:

1. A run-time form of a handler states that its own region is live. `hwith` and `hctl`
   take `(mode = .live → φ.svc (.atm (.r p)))`. `hdone` already carries the atom in
   `mode.use φ (Ty.rg p).reach`, and a step that builds an `hdone`, `hkeep` or `unw` form
   reads it off the form it replaces. `hwith_catch` then needs no other premise. The
   premise is dischargeable where a handler is installed: `hdl_install` types the new
   `hwith` at the stage of the `inside` body, which holds `Row.scope (.rg n)` and stays
   below that frame's bound. The cell it allocates keeps its old type: the atom lives in
   the *stage* of the form, not in the row the cell's arrow names.
2. A scope declares what its body uses: `Const.ok Θ .scopeRun ι .live` is
   `Row.Sub (Row.ofCaps ι.C) ι.r`. The row bounding the desugared body is
   `Scope s ∪ r ∪ obs r s`; the qualifier of a function outside the binder cannot name
   `s`, and `obs` holds no atom, so this is exactly what that bound asks of the qualifier
   (`scopeRun_body`). `LiveRules.lean` checks that it rejects a qualifier the rows do not
   cover.
3. A resumed body is typed where its handler's region is live. `hres` types its body at
   `mode.use (.rt r ε) (Ty.rg p).reach`, as the hole of its arm already is, and asks
   `C' (.r p)` of the cell of `resume` (which `hctl` supplies). The `hresB` frame provides
   that region's atom like `hctlA` and `hdone` (`Frame.rgn`). Without it `resume` puts an
   `hwith p` under a stage without the atom of `p`, which rule 1 forbids.
4. Work an owner keeps declares what it uses: `Const.ok` of a live `.addFin` or `.spawn`
   is `Row.Sub (Row.ofCaps ι.C) ι.r`, as rule 2 asks of `Scope.run`. The row of a child
   then holds the qualifier of what it runs, and the owner bound of its row
   (`TypingMode.live.keep`) reaches those atoms too: a child of `p` runs at a stage of `p`,
   of regions older than `p` its registration uses, and of the arms of handlers it keeps
   (`withs_typing_atm`, `fin_await_atm`, `kid_stage`). `LiveRules.lean` rejects an
   undeclared qualifier and accepts a declared one.
With them, preservation of every step holds for both instances (`rt_preservation_mode`,
`preservation`), and so does type soundness: `T1_live` is `T1` of `TypM S .live`. That
the roots name no run-time region is part of `RootTyM` (`svcBelow`): typing by use does not
bound what a value holds by its type, so it cannot be read off a root's typing.

`T4'_live` (`Live/T4Live.lean`) is T4′ of `TypLive`. `Nest` bounds what each thread *names*
(`Within`), which the live discipline gives up, so its invariant is split in two:

* `LNest` (`Live/LNest*.lean`) is the half of `Nest` about run-time forms: the ancestors
  of the regions, where the `inside` forms and the handlers of each thread are, and for
  each handler a cell whose arms' row names only ancestors of its region (`LCell`). A
  step keeps it given two facts of typing: the owner given work is a region of the hole,
  and the arms of a new handler name regions of its place or its own.
* `LCfgTy` (`Live/LCfg.lean`) is the typed configuration with the stages of the children
  and finalizers of each region `p` holding only ancestors of `p` (`LRgs`).

Every region atom of the stage of a hole is one of the thread's stage or of a frame
(`hole_atm`), and so a region of the hole, which `LNest` makes live; a step that needs a
region has its atom in the stage of its redex (`open_atm`, `op_atm`, `fin_atm`). That gives
T4.1, and T4.3 follows from the regions of the handler frames.

Signature elision (`Live/Elide.lean`): `Ty.elide` is the model's reading of a signature
with `Live` of what earlier parameters reach. The live type of a constant that the core
types is kept by it (`Const.ty_kept`), so its elided reading is a live supertype
(`Const.ty_le_elide`) and the core types the constant there by (Sub)
(`TypM.const_elided`), given instance types that elision leaves alone (`CArg.Fixed`).
This covers the constants, not user signatures, and the link to the model's `elide` is
by reading, not a proof; the core counts every atom of a reach, the model only regions
and capture rows.

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
