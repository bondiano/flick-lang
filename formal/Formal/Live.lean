import Formal.Regions
import Formal.Nest
import Formal.T43Main
import Formal.LiveRules

/-!
# T4′: the safety a discipline of use promises

The discipline promises of every configuration a program reaches that no thread is at a
step that needs a region that is not live (T4.1), and that no operation is answered by a
handler of a region that ended (T4.3). It does not promise that nothing that can still step
*reaches* a dead region (the former T4.2): a region is needed alive only where it is used
(`flick-model/src/check/regions/live.rs`), so a closure or a task that holds a capability
and never uses it may outlive the region, and the machine runs it without a use after
close.

`SafeUse Ok` is that promise as a property of a judgment `Ok` of closed programs; `T4'`
(`Live/T4Live.lean`) proves it of `Typ`. `Live C` is `Row.ofCaps C`, using the existing
atoms of `Scope s`, so (Scope) discharges it without a new row element. No rule bounds
what a value holds by its type: a runtime call or await needs the qualifier in its stage
(`Stage.use`), and a qualified body is checked at that same stage. `op` needs `Scope s`,
and work an owner keeps needs every region atom of its row to outlive the owner
(`Row.Outlives`). `LeLive` compares use stages, so subtyping cannot erase a use
obligation by hiding a qualifier in an argument or a result type. `LiveRules.lean` checks
these rules, including a source scope that returns a closure holding its capability
without using it. Control arms and their runtime frames add the handler's own region
locally (`Frame.provides`), so `resume` can use it without leaking that region into the
handler's outer row.

Findings that shaped the rules:

* A value may be retyped at any stage, so the frame of a finished handler loses nothing
  when it gives its value back (`hdoneVal`).
* The step that closes a handler's region (`hwithCatch`) keeps its answer at a stage
  that holds that region's atom, which only the frame supplies, so the step carries the
  atom as a premise (`Stage.use_rg_le`). Every region atom of a running stage is live,
  including the stages of child work and handler frames.
* A use site needs the qualifier of what it uses in its stage, and a context that fixes
  that stage (`scope`'s rows bound its body; `Scope.run`'s desugaring, `scopeRun_body` in
  `ScopeRun.lean`, creates one) must then hold the qualifier's atoms too. For `Scope.run`
  that is not derivable from subtyping alone: `LeLive.arrow_use` gives
  `ofCaps C_f ⊆ ψ₀ ∪ ofCaps ι.C`, not `⊆ ψ₀`, so `Const.ok` asks it (rule 2 below).

Four rules close the proofs:

1. A run-time form of a handler states that its own region is live. `hwith` and `hctl`
   take `(φ.svc (.atm (.r p)))`. `hdone` already carries the atom in
   `Stage.use φ (Ty.rg p).reach`, and a step that builds an `hdone`, `hkeep` or `unw` form
   reads it off the form it replaces. `hwith_catch` then needs no other premise. The
   premise is dischargeable where a handler is installed: `hdl_install` types the new
   `hwith` at the stage of the `inside` body, which holds `Row.scope (.rg n)` and stays
   below that frame's bound. The cell it allocates keeps its old type: the atom lives in
   the *stage* of the form, not in the row the cell's arrow names.
2. A scope declares what its body uses: `Const.ok Θ .scopeRun ι` is
   `Row.Sub (Row.ofCaps ι.C) ι.r`. The row bounding the desugared body is
   `Scope s ∪ r ∪ obs r s`; the qualifier of a function outside the binder cannot name
   `s`, and `obs` holds no atom, so this is exactly what that bound asks of the qualifier
   (`scopeRun_body`). `LiveRules.lean` checks that it rejects a qualifier the rows do not
   cover.
3. A resumed body is typed where its handler's region is live. `hres` types its body at
   `Stage.use (.rt r ε) (Ty.rg p).reach`, as the hole of its arm already is, and asks
   `C' (.r p)` of the cell of `resume` (which `hctl` supplies). The `hresB` frame provides
   that region's atom like `hctlA` and `hdone` (`Frame.rgn`). Without it `resume` puts an
   `hwith p` under a stage without the atom of `p`, which rule 1 forbids.
4. Work an owner keeps declares what it uses: `Const.ok` of `.addFin` or `.spawn`
   is `Row.Sub (Row.ofCaps ι.C) ι.r`, as rule 2 asks of `Scope.run`. The row of a child
   then holds the qualifier of what it runs, and the owner bound of its row
   (`Row.Outlives`) reaches those atoms too: a child of `p` runs at a stage of `p`,
   of regions older than `p` its registration uses, and of the arms of handlers it keeps
   (`withs_typing_atm`, `fin_await_atm`, `kid_stage`). `LiveRules.lean` rejects an
   undeclared qualifier and accepts a declared one.

With them preservation (`rt_preservation`, `preservation`) and type soundness (`T1`) hold.
That the roots name no run-time region is part of `RootTy` (`svcBelow`): typing by use
does not bound what a value holds by its type, so it cannot be read off a root's typing.

The proof of `T4'` keeps only the half of the old nesting invariant about run-time forms,
since what each thread *names* is no longer bounded:

* `LNest` (`Live/LNest*.lean`): the ancestors of the regions, where the `inside` forms
  and the handlers of each thread are, and for each handler a cell whose arms' row names
  only ancestors of its region (`LCell`). A step keeps it given two facts of typing: the
  owner given work is a region of the hole, and the arms of a new handler name regions of
  its place or its own.
* `LCfgTy` (`Live/LCfg.lean`): the typed configuration with the stages of the children
  and finalizers of each region `p` naming only ancestors of `p` (`LRgs`).

Every region atom of the stage of a hole is one of the thread's stage or of a frame
(`hole_atm`), and so a region of the hole, which `LNest` makes live; a step that needs a
region has its atom in the stage of its redex (`open_atm`, `op_atm`, `fin_atm`). That gives
T4.1, and T4.3 follows from the regions of the handler frames.

Signature elision (`Live/Elide.lean`): `Ty.elide` is the model's reading of a signature
with `Live` of what earlier parameters reach. The live type of a constant that the core
types is kept by it (`Const.ty_kept`), so its elided reading is a live supertype
(`Const.ty_le_elide`) and the core types the constant there by (Sub)
(`Typ.const_elided`), given instance types that elision leaves alone (`CArg.Fixed`).
This covers the constants, not user signatures, and the link to the model's `elide` is
by reading, not a proof; the core counts every atom of a reach, the model only regions
and capture rows.

The runtime contract (`Formal/RT/`) takes only T4.3 (as `G11`), so giving T4.2 up does not
touch T5; what it costs is that a value may keep a closed capability reachable (memory,
not use).
-/

namespace Flick

variable {S : Sig}

/-- T4′ of a judgment `Ok` of closed programs: in every configuration a program it accepts
reaches, no thread (the main thread or a child of a live region) is at a step that needs a
region that is not live (T4.1), and no operation is answered by a handler of a region that
ended (T4.3). Nothing is said of what threads *reach*. -/
def SafeUse (root : Roots) (Ok : Tm → Ty → Stage → Prop) : Prop :=
  ∀ e₀ t φ₀ c, Ok e₀ t φ₀ → e₀.Below 0 → t.YF 0 → φ₀.substT (kill 0) = φ₀ →
    CSteps root ⟨[], [], e₀⟩ c →
    (∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids, ∀ E r,
      OkE E → k = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ i e E Ef o k v q, c.at i = some e → OkE E → e = plug E (.app (.const (.perform Ef o k)) v) →
      handlerOf E Ef o k = some q → Live c.rgs q)

/-- T4′ is closed under a smaller judgment: a discipline that accepts less keeps it. -/
theorem SafeUse.mono {root : Roots} {Ok Ok' : Tm → Ty → Stage → Prop}
    (h : SafeUse root Ok) (hle : ∀ e t φ, Ok' e t φ → Ok e t φ) : SafeUse root Ok' :=
  fun e₀ t φ₀ c h₀ => h e₀ t φ₀ c (hle _ _ _ h₀)

end Flick
