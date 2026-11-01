import Formal.Elab.Weaken

/-!
# Elaboration: the sugar of §3 as source terms (elaboration.md §3)

`Sugar P` names the Prelude methods and the constants of Γ₀ the sugar uses, with their
schemes. `do`, `for` and `?` become source terms (`Exp`); their dictionaries come from
elaborating these terms, by (Method). Patterns are variables, and `_` (a binder nobody uses).

A tuple of the values of a group `p₁ <- e₁ and … and pₙ <- eₙ` is nested to the left,
`((a₁, a₂), …, aₙ)`, built by `map2 pair`; `λ(p₁, …, pₙ). e` is `unc n (λp₁ … pₙ. e)` with a
constant `unc n : (a₁ → … → aₙ → r) → T_n → r` for each `n ≥ 1`.
-/

namespace Flick.Elab

/-- `a →P b`. -/
abbrev arrP (a b : Ty) : Ty := arrTy .P a b

/-- `a₁ →P … →P aₙ →P r`. -/
def curry (as : List Ty) (r : Ty) : Ty := as.foldr arrP r

/-- The left-nested tuple `((t, a₁), …, aₙ)` of the type constructor `tup`. -/
def nest (tup : Nat) : Ty → List Ty → Ty
  | t, [] => t
  | t, a :: as => nest tup (.app (.app (.con tup) t) a) as

/-- `var 0, …, var (n-1)`. -/
def vars (n : Nat) : List Ty := (List.range n).map .var

/-- `∀a₁ … aₙ r. (a₁ → … → aₙ → r) → T_n → r`, for `n ≥ 1`. -/
def uncSch (tup n : Nat) : Scheme :=
  ⟨n + 1, [], arrP (curry (vars n) (.var n)) (arrP (nest tup (.var 0) (vars n).tail) (.var n))⟩

/-- The Prelude classes and methods, the constants of Γ₀ and the types the sugar uses. -/
structure Sugar (P : Prog) where
  tup : Nat
  seq : Nat
  list : Nat
  unit : Nat
  result : Nat
  errors : Nat
  monad : Nat
  applicative : Nat
  alternative : Nat
  iterable : Nat
  flatMap : Nat
  map2 : Nat
  pure : Nat
  nothing : Nat
  iter : Nat
  pair : Nat
  unc : Nat → Nat
  noneC : Nat
  seqFlatMap : Nat
  singleton : Nat
  empty : Nat
  toList : Nat
  each : Stage → Nat
  unitV : Nat
  fromResult : Nat
  /-- `flat_map : ∀m a b. Monad m ⇒ (a → m b) → m a → m b` -/
  hFlatMap : P.methods flatMap = some (monad, ⟨3, [⟨monad, [.var 0]⟩],
    arrP (arrP (.var 1) (.app (.var 0) (.var 2)))
      (arrP (.app (.var 0) (.var 1)) (.app (.var 0) (.var 2)))⟩)
  /-- `map2 : ∀f a b x. Applicative f ⇒ (a → b → x) → f a → f b → f x` -/
  hMap2 : P.methods map2 = some (applicative, ⟨4, [⟨applicative, [.var 0]⟩],
    arrP (arrP (.var 1) (arrP (.var 2) (.var 3)))
      (arrP (.app (.var 0) (.var 1)) (arrP (.app (.var 0) (.var 2)) (.app (.var 0) (.var 3))))⟩)
  /-- `nothing : ∀f a. Alternative f ⇒ f a` -/
  hNothing : P.methods nothing = some (alternative, ⟨2, [⟨alternative, [.var 0]⟩],
    .app (.var 0) (.var 1)⟩)
  /-- `iter : ∀f a. Iterable f ⇒ f a → Seq a` -/
  hIter : P.methods iter = some (iterable, ⟨2, [⟨iterable, [.var 0]⟩],
    arrP (.app (.var 0) (.var 1)) (.app (.con seq) (.var 1))⟩)
  hPair : P.consts pair = some ⟨2, [], arrP (.var 0) (arrP (.var 1)
    (.app (.app (.con tup) (.var 0)) (.var 1)))⟩
  hUnc : ∀ n, 0 < n → P.consts (unc n) = some (uncSch tup n)
  hNone : P.consts noneC = some ⟨1, [], optTy (.var 0)⟩
  /-- `Seq.flat_map : ∀a b. (a → Seq b) → Seq a → Seq b` -/
  hSeqFlatMap : P.consts seqFlatMap = some ⟨2, [], arrP (arrP (.var 0) (.app (.con seq) (.var 1)))
    (arrP (.app (.con seq) (.var 0)) (.app (.con seq) (.var 1)))⟩
  hSingleton : P.consts singleton = some ⟨1, [], arrP (.var 0) (.app (.con seq) (.var 0))⟩
  hEmpty : P.consts empty = some ⟨1, [], .app (.con seq) (.var 0)⟩
  hToList : P.consts toList = some ⟨1, [], arrP (.app (.con seq) (.var 0)) (.app (.con list) (.var 0))⟩
  /-- `each_φ : ∀a. (a →φ Unit) →P Seq a →φ Unit`, the loop of `for … do` at the stage `φ` -/
  hEach : ∀ φ, P.consts (each φ) = some ⟨1, [], arrP (arrTy φ (.var 0) (.con unit))
    (arrTy φ (.app (.con seq) (.var 0)) (.con unit))⟩
  hUnitV : P.consts unitV = some ⟨0, [], .con unit⟩
  /-- `from_result : ∀e a. Result (Errors e) a →R a` (`core.md` §1.4) -/
  hFromResult : P.consts fromResult = some ⟨2, [], arrTy .R
    (.app (.app (.con result) (.app (.con errors) (.var 0))) (.var 1)) (.var 1)⟩

variable {P : Prog} (S : Sugar P)

/-- `λp₁ … pₙ. b` -/
def lams : Nat → Exp → Exp
  | 0, b => b
  | n + 1, b => .lam (lams n b)

def Sugar.bind (k e : Exp) : Exp := .app (.app (.meth S.flatMap) k) e

def Sugar.both (f a b : Exp) : Exp := .app (.app (.app (.meth S.map2) f) a) b

/-- `map2 pair (… (map2 pair e₁ e₂) …) eₙ` -/
def Sugar.tuple : Exp → List Exp → Exp
  | acc, [] => acc
  | acc, e :: es => Sugar.tuple (S.both (.const S.pair) acc e) es

/-- A statement of `do`; `group e₁ mid eₙ` is `p₁ <- e₁ and … and pₙ <- eₙ`. -/
inductive Stmt where
  | bind (e : Exp)
  | exp (e : Exp)
  | let_ (e : Exp)
  | letSig (σ : Scheme) (e : Exp)
  | guard (c : Exp)
  | group (e₁ : Exp) (mid : List Exp) (eₙ : Exp)

/-- The argument `e` of a last statement `pure e` after a group. -/
def Sugar.pureLast : List Stmt → Exp → Option Exp
  | [], .app (.meth p) v => if p = S.pure then some v else none
  | _, _ => none

/-- `⟦do { s̄; r }⟧` of §3.1. -/
def Sugar.desugarDo : List Stmt → Exp → Exp
  | [], r => r
  | .bind e :: ss, r => S.bind (.lam (Sugar.desugarDo ss r)) e
  | .exp e :: ss, r => S.bind (.lam ((Sugar.desugarDo ss r).rename Nat.succ)) e
  | .let_ e :: ss, r => .let_ e (Sugar.desugarDo ss r)
  | .letSig σ e :: ss, r => .letSig σ e (Sugar.desugarDo ss r)
  | .guard c :: ss, r => .ite c (Sugar.desugarDo ss r) (.meth S.nothing)
  | .group e₁ mid eₙ :: ss, r =>
    match S.pureLast ss r with
    | some v => S.both (.app (.const (S.unc (mid.length + 1))) (lams (mid.length + 2) v))
        (S.tuple e₁ mid) eₙ
    | none => S.bind (.app (.const (S.unc (mid.length + 2))) (lams (mid.length + 2)
        (Sugar.desugarDo ss r))) (S.tuple e₁ (mid ++ [eₙ]))

/-- A clause of `for`. -/
inductive Clause where
  | gen (s : Exp)
  | guard (c : Exp)
  | let_ (e : Exp)

/-- `G⟦c̄ ; e⟧` of §3.2, with `iter` on each source (Р183). -/
def Sugar.gen : List Clause → Exp → Exp
  | [], e => .app (.const S.singleton) e
  | .gen s :: cs, e =>
    .app (.app (.const S.seqFlatMap) (.lam (Sugar.gen cs e))) (.app (.meth S.iter) s)
  | .guard c :: cs, e => .ite c (Sugar.gen cs e) (.const S.empty)
  | .let_ x :: cs, e => .let_ x (Sugar.gen cs e)

/-- `L⟦c̄ ; e⟧` of §3.2 at the stage `φ` (Р184). -/
def Sugar.loop (φ : Stage) : List Clause → Exp → Exp
  | [], e => e
  | .gen s :: cs, e =>
    .app (.app (.const (S.each φ)) (.lam (Sugar.loop φ cs e))) (.app (.meth S.iter) s)
  | .guard c :: cs, e => .ite c (Sugar.loop φ cs e) (.const S.unitV)
  | .let_ x :: cs, e => .let_ x (Sugar.loop φ cs e)

/-- `[for c̄ yield e]` -/
def Sugar.forList (cs : List Clause) (e : Exp) : Exp := .app (.const S.toList) (S.gen cs e)

/-- `e?` on a `Result`, at a runtime stage. -/
def Sugar.tryResult (e : Exp) : Exp := .app (.const S.fromResult) e

/-- `let y = e? in b` on an `Option`: `match e with None → None | Some y → b`. -/
def Sugar.tryOption (e b : Exp) : Exp := .caseOpt e (.const S.noneC) b

/-- `s; rest` of a block: `let _ : Unit = s in rest`. -/
def Sugar.seqUnit (s rest : Exp) : Exp := .letSig ⟨0, [], .con S.unit⟩ s (rest.rename Nat.succ)

end Flick.Elab
