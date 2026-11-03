import Formal.RT.Recover

/-!
# λRT: configurations (runtime.md §1)

The core is a parameter: `Core.step e a e'` is a labelled transition of a fiber's code,
where the label `a` says what the code asks of the runtime and `e'` is the continuation.
`Core.plug e' w` continues `e'` with the outcome `w` (or applies a function to it). Any
relation will do, so every theorem holds for every program of every core.

The parts of a step that Effect 4.1.2 or the Go memory model implement are fields of
`Lib`; `Hyp` (`Hyp.lean`) states G7–G9 about them.
-/

namespace Flick.RT

/-- An exit (`Exit`, runtime.md §1.1). -/
inductive Out (V : Type) where
  | ok (v : V)
  | fail (c : Cause V)

/-- `cause(w)` of §3.2. -/
def Out.cause {V : Type} : Out V → Cause V
  | .ok _ => []
  | .fail c => c

/-- What a fiber's code asks of the runtime: the labels of `Core.step`. -/
inductive Act (T V : Type) where
  /-- a step of the core that the runtime does not see -/
  | tau
  /-- the code is finished -/
  | ret (w : Out V)
  /-- `Ref.make v` -/
  | make (v : V)
  /-- `Ref.modify ℓ g`, `g` a pure arrow (T2(a) of the core, §3.7) -/
  | modify (l : Nat) (g : V → V × V)
  /-- `Fiber.spawn (own π) g t` -/
  | spawn (π : Nat) (g t : T)
  /-- `Scope.add_finalizer π t`; `t` is applied to the exit of `π` -/
  | addFin (π : Nat) (t : T)
  /-- `acquire_release π a rel` -/
  | acquire (π : Nat) (a rel : T)
  /-- `scope s { b }`; `b` is applied to `own π` -/
  | scope (b : T)
  | interrupt (k : Nat)
  | join (k : Nat)
  /-- a foreign `async` call -/
  | ext
  /-- `FiberSet.make` in the region `π` -/
  | group (π : Nat)
  /-- `FiberSet.run γ t` -/
  | grun (γ : Nat) (t : T)
  /-- `parallel t₁ … tₙ` -/
  | par (ts : List T)
  /-- `provide L { b }`: the memoizable layers of `L` in build order, each an identity
  and its build code -/
  | provide (Ls : List (Nat × T)) (b : T)
  /-- an `ask_S` answered by the `provide` owner `π` of the environment -/
  | use (π : Nat)

/-- The core, a parameter of λRT. -/
structure Core (T V : Type) where
  step : T → Act T V → T → Prop
  plug : T → Out V → T
  /-- the value of a handle: `fib k`, `own π`, a group, a location -/
  ofId : Nat → V
  unit : V

/-- What the library does, where the rules depend on it. -/
structure Lib (V : Type) where
  /-- the order in which `Scope.close` runs the stack of finalizers (newest first) -/
  closeOrder : {α : Type} → List α → List α
  /-- adding to a closed area runs the finalizer at once (otherwise it is dropped) -/
  lateRuns : Bool
  /-- `acquireRelease` registers under the mask of the acquisition -/
  acqAtomic : Bool
  /-- `forEachConcurrent` waits for its branches before it returns -/
  joinWaits : Bool
  /-- the memo map finds a layer by its identity -/
  memo : Bool
  /-- `causeCombine` and `causeFromReasons` -/
  combine : Cause V → Cause V → Cause V
  concat : Cause V → Cause V → Cause V
  /-- `causeElectDefects`, applied to the join of late `forEachConcurrent` failures -/
  elect : Cause V → Cause V
  /-- what an interruption that skips a handler makes of the cause -/
  skip : Cause V → Cause V
  /-- `Ref.modify` is one step (the cell's mutex in Go) -/
  refAtomic : Bool
  /-- a foreign call puts its Promise into `P` of the current owner -/
  extTracked : Bool

/-- `st` of §1.3. -/
inductive St where
  | open | closing | finalizing | closed
  deriving DecidableEq

/-- `open` or `closing`: the owner accepts hooks and finalizers (I2). -/
def St.live : St → Bool
  | .open | .closing => true
  | _ => false

/-- The phase of a closing (§3.3): `hooks false` is C1; `stop γ b` a group's hook waiting for
the group's children; `kids` is C2–C3; `hooks true` runs the hooks added since C1 (`docs/formal/runtime.md#a-closing-hook-added-after-c1-does-not-run`) and
waits for `P` (C4); `fins` is C5. -/
inductive Phase where
  | hooks (late : Bool)
  | stop (γ : Nat) (late : Bool)
  | kids
  | fins

/-- A hook of `H`: stop a group, or code (detaching a listener). -/
inductive Hook (T : Type) where
  | stop (γ : Nat)
  | code (t : T)

/-- A finalizer of `F`: code applied to the exit, or the observer of a memoized layer. -/
inductive FinAct (T : Type) where
  | code (t : T)
  | obs (m L : Nat)

structure Fin (T : Type) where
  id : Nat
  act : FinAct T

/-- `Λ(π)` of §1.3, with the state of its closing. The stack `F` is newest first. -/
structure Owner (T V : Type) where
  st : St
  K : List Nat
  W : List Nat
  P : List Nat
  H : List (Hook T)
  F : List (Fin T)
  x : Out V
  ph : Phase
  cH : Cause V
  cF : Cause V
  /-- what (C5) took from `F`, in the order of `closeOrder`, not yet started -/
  fs : List (Fin T)
  /-- the fiber that runs its body (or builds its layer) -/
  hold : Nat

/-- A fresh owner whose body the fiber `f` runs. -/
def Owner.fresh {T V : Type} (x : Out V) (f : Nat) : Owner T V :=
  ⟨.open, [], [], [], [], [], x, .hooks false, [], [], [], f⟩

/-- Where a fiber stands. -/
inductive Code (T V : Type) where
  | tm (e : T)
  | out (w : Out V)
  /-- the runtime drives a closing -/
  | idle
  | waitFib (k : Nat)
  | waitExt (p : Nat)
  /-- a watcher waiting for its child, with the supervisor `g` -/
  | watch (π k : Nat) (g : T)
  /-- starting the branches of `parallel`, then waiting for them -/
  | parC (ts : List T)
  | waitPar
  /-- a non-atomic `Ref.modify` that has read `v` -/
  | modw (l : Nat) (g : V → V × V) (v : V)
  /-- `provide`: build the layers left, then the body -/
  | provC (πp m : Nat) (Ls : List (Nat × T)) (b : T)

/-- Run-time frames of a fiber, innermost first. -/
inductive Frame (T V : Type) where
  | resume (e : T)
  /-- an outcome `ok` becomes `w` -/
  | then (w : Out V)
  /-- the body of `scope` (`env = none`) or of `provide` with its memo map -/
  | inside (π : Nat) (env : Option Nat)
  | closing (π : Nat)
  | acq (π : Nat) (rel : T)
  | par (ks : List Nat)
  /-- building the layer `L` of `provide πp` into the map `m` in the area `σ` -/
  | build (πp m L σ : Nat) (Ls : List (Nat × T)) (b : T)

/-- Why a fiber exists; the invariants (`Inv.lean`) say who waits for it. -/
inductive Role where
  | main | child | watcher
  | gkid (γ : Nat)
  | branch (f : Nat)

/-- A fiber `⟨e, m, q, π, θ⟩` of §1.2. The mask `m` is not modelled: an interruption may
be delivered at any point of user code. `base` is the owner the fiber started in; the
current owner is the innermost `inside` or `build` frame, else `base`. -/
structure Fiber (T V : Type) where
  code : Code T V
  stack : List (Frame T V)
  q : Option Nat
  base : Nat
  θ : List Nat
  mm : Option Nat
  role : Role

inductive FState (T V : Type) where
  | run (φ : Fiber T V)
  | done (x : Out V)

/-- A group of fibers (§3.6). -/
structure Grp where
  owner : Nat
  isOpen : Bool
  kids : List Nat

/-- A memo-map entry (§3.5); `obs` are the ids of its observers, so `n` is its length. -/
inductive ESt (V : Type) where
  | building | built
  | failed (c : Cause V)

structure Ent (V : Type) where
  st : ESt V
  σ : Nat
  obs : List Nat

structure Memo (V : Type) where
  parent : Option Nat
  ents : Nat → Option (Ent V)

/-- Events of the journal, newest first. -/
inductive Ev (V : Type) where
  | spawn (π k w : Nat)
  | gkid (π k : Nat)
  | branch (π k : Nat)
  /-- a finalizer handed to `π`: `add_finalizer`, or `acquire_release` returned -/
  | reg (π i : Nat)
  /-- put into `F(π)` -/
  | fin (π i : Nat)
  /-- (Fin-Now): started at once -/
  | now (π i : Nat)
  /-- (C5) starts it -/
  | run (π i : Nat)
  | close (π : Nat)
  | fins (π : Nat)
  | closed (π : Nat)
  | done (k : Nat) (x : Out V)
  | sup (k : Nat) (c : Cause V)
  | defect (c : Cause V)
  | ext (π p : Nat)
  | settle (p : Nat)
  | make (l : Nat) (v : V)
  | modify (l : Nat) (v b v' : V)
  | build (m L σ : Nat)
  | obs (π m L σ i : Nat)
  | release (m L σ : Nat)
  | use (π : Nat)

/-- `⟨Φ, Λ, μ, M, D⟩` of §1.4, with groups `G`, pending Promises `X`, the journal `J` and
the next fresh id `n`. -/
structure Conf (T V : Type) where
  Φ : Nat → Option (FState T V)
  Λ : Nat → Option (Owner T V)
  G : Nat → Option Grp
  X : Nat → Option Nat
  μ : Nat → Option V
  M : Nat → Option (Memo V)
  D : List (Cause V)
  J : List (Ev V)
  n : Nat

/-- Point update of a map. -/
def upd {α : Type} (f : Nat → α) (k : Nat) (v : α) : Nat → α :=
  fun j => if j = k then v else f j

@[simp] theorem upd_same {α : Type} (f : Nat → α) (k : Nat) (v : α) : upd f k v k = v := by
  simp [upd]

theorem upd_other {α : Type} (f : Nat → α) {k j : Nat} (v : α) (h : j ≠ k) :
    upd f k v j = f j := by simp [upd, h]

section
variable {T V : Type}

/-- The current owner of a stack (§1.2 `π`). -/
def curOf (base : Nat) : List (Frame T V) → Nat
  | [] => base
  | .inside π _ :: _ => π
  | .build _ _ _ σ _ _ :: _ => σ
  | _ :: s => curOf base s

def Fiber.cur (φ : Fiber T V) : Nat := curOf φ.base φ.stack

/-- The provide owners whose services the stack sees: `θ` of §1.2. -/
def envOf : List (Frame T V) → List Nat
  | [] => []
  | .inside π (some _) :: s => π :: envOf s
  | _ :: s => envOf s

def Fiber.env (φ : Fiber T V) : List Nat := envOf φ.stack ++ φ.θ

/-- The current memo map. -/
def mapOf (mm : Option Nat) : List (Frame T V) → Option Nat
  | [] => mm
  | .inside _ (some m) :: _ => some m
  | _ :: s => mapOf mm s

def Fiber.map (φ : Fiber T V) : Option Nat := mapOf φ.mm φ.stack

/-- A fiber that is finished. -/
def Done (Φ : Nat → Option (FState T V)) (k : Nat) : Prop := ∃ x, Φ k = some (.done x)

/-- The memo map `m` and its ancestors, nearest first, for at most `fuel` maps. -/
def lookup (M : Nat → Option (Memo V)) (L : Nat) : Nat → Nat → Option (Nat × Ent V)
  | 0, _ => none
  | fuel + 1, m => match M m with
    | none => none
    | some mm => match mm.ents L with
      | some e => some (m, e)
      | none => match mm.parent with
        | some p => lookup M L fuel p
        | none => none

/-- Update the entry of `L` in the map `m`. -/
def setEnt (M : Nat → Option (Memo V)) (m L : Nat) (e : Option (Ent V)) :
    Nat → Option (Memo V) :=
  fun j => if j = m then (M m).map fun mm => { mm with ents := upd mm.ents L e } else M j

/-- Update the owner `π`, if it exists. -/
def updO (Λ : Nat → Option (Owner T V)) (π : Nat) (g : Owner T V → Owner T V) :
    Nat → Option (Owner T V) :=
  fun j => if j = π then (Λ π).map g else Λ j

end

end Flick.RT
