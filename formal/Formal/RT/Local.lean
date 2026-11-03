import Formal.RT.Config

/-!
# λRT: steps of one fiber (runtime.md §3.1–§3.4)

`Local` collects the fiber steps that change nothing but the fiber. An interruption is
delivered at any point of user code and of the waits that are interruption points
(`Code.IntPoint`); without the mask this allows more interleavings than §3.2, so safety
proved for these steps holds for those of §3.2.
-/

namespace Flick.RT

variable {T V : Type}

/-- Where (Int-Deliver) may happen. Closing, watching and starting branches are masked. -/
def Code.IntPoint : Code T V → Prop
  | .tm _ | .waitFib _ | .waitExt _ | .provC .. => True
  | _ => False

/-- The reasons of `c` that are no interruption. -/
def noInt (c : Cause V) : Cause V := c.filter fun | .int _ => false | _ => true

/-- The causes of the failed exits. -/
def failsOf (xs : List (Out V)) : List (Cause V) :=
  xs.filterMap fun | .fail c => some c | .ok _ => none

/-- The result of `parallel` (Par), from the exits in input order: a late failure with
reasons other than `Int` gives `elect (a ⊕ noInt c')`. -/
def parResult (L : Lib V) (unit : V) (xs : List (Out V)) : Out V :=
  match failsOf xs with
  | [] => .ok unit
  | c :: cs => .fail (cs.foldl (fun a c' =>
      if (noInt c').isEmpty then a else L.elect (L.combine a (noInt c'))) c)

/-- The result of (C6). -/
def closeResult (L : Lib V) (w : Out V) (cH cF : Cause V) : Out V :=
  if L.combine cH cF = [] then w else .fail (L.combine (L.combine w.cause cH) cF)

/-- The exits of the fibers `ks` that are done. -/
def exits (Φ : Nat → Option (FState T V)) (ks : List Nat) : List (Out V) :=
  ks.filterMap fun k => match Φ k with
    | some (.done x) => some x
    | _ => none

/-- Steps that change only the fiber. -/
inductive Local (K : Core T V) (L : Lib V) (C : Conf T V) : Fiber T V → Fiber T V → Prop
  | tau {φ e e'} : φ.code = .tm e → K.step e .tau e' → Local K L C φ { φ with code := .tm e' }
  | ret {φ e e' w} : φ.code = .tm e → K.step e (.ret w) e' →
      Local K L C φ { φ with code := .out w }
  | resume {φ w e s} : φ.code = .out w → φ.stack = .resume e :: s →
      Local K L C φ { φ with code := .tm (K.plug e w), stack := s }
  | then_ {φ w w' s} : φ.code = .out w → φ.stack = .then w' :: s →
      Local K L C φ { φ with code := .out (match w with | .ok _ => w' | .fail c => .fail c),
                              stack := s }
  /-- (Int-Deliver) -/
  | deliver {φ g} : φ.q = some g → φ.code.IntPoint →
      Local K L C φ { φ with code := .out (.fail [.int (some g)]) }
  /-- (Acquire): the acquisition runs; the registration is `Step.acqLive` and its kin -/
  | acquire {φ e e' π a rel} : φ.code = .tm e → K.step e (.acquire π a rel) e' →
      Local K L C φ { φ with code := .tm a, stack := .acq π rel :: .resume e' :: φ.stack }
  | acqFail {φ c π rel s} : φ.code = .out (.fail c) → φ.stack = .acq π rel :: s →
      Local K L C φ { φ with code := .out (.fail c), stack := s }
  | join {φ e e' k} : φ.code = .tm e → K.step e (.join k) e' →
      Local K L C φ { φ with code := .waitFib k, stack := .resume e' :: φ.stack }
  /-- (Join) -/
  | waitFib {φ k x} : φ.code = .waitFib k → C.Φ k = some (.done x) →
      Local K L C φ { φ with code := .out x }
  /-- (Ext-Settle), the waiting fiber's side: it gets a value or a failure -/
  | wake {φ p w} : φ.code = .waitExt p → C.X p = none → Local K L C φ { φ with code := .out w }
  | par {φ e e' ts} : φ.code = .tm e → K.step e (.par ts) e' →
      Local K L C φ { φ with code := .parC ts, stack := .par [] :: .resume e' :: φ.stack }
  | parGo {φ} : φ.code = .parC [] → Local K L C φ { φ with code := .waitPar }
  /-- (Par): every branch is done -/
  | parDone {φ ks s} : φ.code = .waitPar → φ.stack = .par ks :: s → (∀ k ∈ ks, Done C.Φ k) →
      Local K L C φ { φ with code := .out (parResult L K.unit (exits C.Φ ks)), stack := s }
  /-- without G7(d): `parallel` returns before its branches end -/
  | parEarly {φ ks s} : L.joinWaits = false → φ.code = .waitPar → φ.stack = .par ks :: s →
      Local K L C φ { φ with code := .out (.fail []), stack := s }
  | provBody {φ πp m b} : φ.code = .provC πp m [] b → Local K L C φ { φ with code := .tm b }
  | provFailed {φ πp m Ly c Ls b m' e c'} : φ.code = .provC πp m ((Ly, c) :: Ls) b →
      L.memo = true → lookup C.M Ly C.n m = some (m', e) → e.st = .failed c' →
      Local K L C φ { φ with code := .out (.fail c') }
  /-- without G8: `Ref.modify` reads, and writes in a later step -/
  | modRead {φ e e' l g v} : L.refAtomic = false → φ.code = .tm e → K.step e (.modify l g) e' →
      C.μ l = some v →
      Local K L C φ { φ with code := .modw l g v, stack := .resume e' :: φ.stack }

/-- A fiber `f` with `φ` becomes `φ'`. -/
abbrev setF (C : Conf T V) (f : Nat) (φ' : Fiber T V) : Nat → Option (FState T V) :=
  upd C.Φ f (some (.run φ'))

/-- `⟨{f₀ ↦ e₀}, {π₀ ↦ open}, ∅, ∅, ∅⟩`: the main fiber `0` in the root owner `0`. -/
def init (e₀ : T) (unit : V) : Conf T V where
  Φ := upd (fun _ => none) 0 (some (.run ⟨.tm e₀, [], none, 0, [], none, .main⟩))
  Λ := upd (fun _ => none) 0 (some (Owner.fresh (.ok unit) 0))
  G := fun _ => none
  X := fun _ => none
  μ := fun _ => none
  M := fun _ => none
  D := []
  J := []
  n := 1

end Flick.RT
