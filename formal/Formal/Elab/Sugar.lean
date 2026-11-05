import Formal.Elab.Weaken
import Formal.Elab.PreludeSchemes

/-!
# Elaboration: the sugar of §3 as source terms (elaboration.md §3)

`Sugar P` names the Prelude methods and the constants of Γ₀ the sugar uses, with their
schemes. `do`, `for` and `?` become source terms (`Exp`); their dictionaries come from
elaborating these terms, by (Method). Patterns are variables, and `_` (a binder nobody uses).

A tuple of the values of a group `p₁ <- e₁ and … and pₙ <- eₙ` is nested to the left,
`((a₁, a₂), …, aₙ)`, built by `map2 pair`; `λ(p₁, …, pₙ). e` is `unc n (λp₁ … pₙ. e)` with a
constant `unc n : (a₁ → … → aₙ → r) → T_n → r` for each `n ≥ 1`.

The schemes are the checker's types of the Prelude (`Prelude.fl`, `Seq.fl`) translated by
With `CRow` qualifiers (`elaboration.md`, Prelude schemes): a type with hidden storage (`Eff`, `Seq`) and the variable of a class over containers
take the qualifier of the value as their first argument, `(T ā)^q ↦ T q ā` (`appQ`), and data
without storage drops it. The core has no (≤-Cap) on that argument, so a qualifier the
caller supplies is a variable of the scheme, and one the result shows names what the
arguments it holds reach (`elaboration.md`, Prelude schemes).

The schemes of the Prelude names are generated from the checker (`PreludeSchemes`, `just
formal-schemes`). `bindTy`, `map2Ty` and `eachSch` are their forms the proofs use; the witness
holds them to the generated ones by `rfl` (`Witness/Prog.lean`, `eachSch_R`). `pair`, `unc n`
and `each_P` have no Prelude name and stay written here (`docs/formal/elaboration.md#the-loop-rule-l-was-missing`, README simplification 9);
`from_result` is a family over closed rows of errors (`frSch`, `docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`).
-/

namespace Flick.Elab

/-- `a →P b`, holding nothing. -/
abbrev arrP (a b : Ty) : Ty := .arr .P a b .cnil

/-- `a₁ →P^q … →P^q aₙ →P^q r`. -/
def curry (q : Ty) (as : List Ty) (r : Ty) : Ty := as.foldr (fun a r => .arr .P a r q) r

/-- The left-nested tuple `((t, a₁), …, aₙ)` of the type constructor `tup`. -/
def nest (tup : Nat) : Ty → List Ty → Ty
  | t, [] => t
  | t, a :: as => nest tup (.app (.app (.con tup) t) a) as

/-- `var 0, …, var (n-1)`. -/
def vars (n : Nat) : List Ty := (List.range n).map .var

/-- `T q a`: a type with hidden storage, or a class variable over containers, with the
qualifier `q` of its value (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`). -/
abbrev appQ (m q a : Ty) : Ty := .app (.app m q) a

/-- The type of `flat_map` for the container `m`:
`(a →P^c m e b) →P (m d a →P^(c ∪ e) m ((c ∪ e) ∪ (d ∪ a)) b)`. -/
def bindTy (m c d a b e : Ty) : Ty :=
  arrP (.arr .P a (appQ m e b) c)
    (.arr .P (appQ m d a) (appQ m (.cun (.cun c e) (.cun d a)) b) (.cun c e))

/-- The type of `map2` for the container `f`, with `R = (c ∪ g) ∪ ((d ∪ a) ∪ (d' ∪ b))`:
`(a →P^c b →P^g x) →P (f d a →P^(c ∪ g) f d' b →P^R f R x)`. -/
def map2Ty (f c d a b x g d' : Ty) : Ty :=
  let R : Ty := .cun (.cun c g) (.cun (.cun d a) (.cun d' b))
  arrP (.arr .P a (.arr .P b x g) c)
    (.arr .P (appQ f d a) (.arr .P (appQ f d' b) (appQ f R x) R) (.cun c g))

/-- `∀a₁ … aₙ r c. (a₁ →^c … → aₙ →^c r) →P (T_n →^c r)`, for `n ≥ 1`. -/
def uncSch (tup n : Nat) : Scheme :=
  ⟨n + 2, [], arrP (curry (.var (n + 1)) (vars n) (.var n))
    (.arr .P (nest tup (.var 0) (vars n).tail) (.var n) (.var (n + 1))), .cnil⟩

/-- `each_φ`, the loop of `for … do` at the stage `φ` (`docs/formal/elaboration.md#the-loop-rule-l-was-missing`):
`each_P : ∀c d a. (a →P^c Unit) →P (Seq d a →P^c Unit)`, which has no Prelude name, and for every
`R r e` the one `each_R : ∀c d a r e. (a →R⟨r;e⟩^c Unit) →P (Seq d a →R⟨r;e⟩^c Unit)`, which is
`Seq.each_effect` (`eachSch_R`) and is used at the rows of the loop. -/
def eachSch (unit seq : Nat) : Stage → Scheme
  | .P => ⟨3, [], arrP (.arr .P (.var 2) (.con unit) (.var 0))
      (.arr .P (appQ (.con seq) (.var 1) (.var 2)) (.con unit) (.var 0)), .cnil⟩
  | .R _ _ => ⟨5, [], arrP (.arr (.R (.var 3) (.var 4)) (.var 2) (.con unit) (.var 0))
      (.arr (.R (.var 3) (.var 4)) (appQ (.con seq) (.var 1) (.var 2)) (.con unit) (.var 0)), .cnil⟩

theorem eachSch_R (unit seq : Nat) (r e : Ty) :
    eachSch unit seq (.R r e) = PreludeSchemes.Seq.each_effect unit seq := rfl

/-- `from_result_n : ∀a. Result (Errors (con n)) a →R⟨∅; con n⟩ a`, `from_result` at the error
type `con n`. Its `raise` adds what the error reaches to the row; for a variable `e` that is the
atom `e`, which `⟦e⟧` as a row variable does not give, so the checker's `∀e a` becomes a family
over error types, as `each_φ` is over stages (`docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`).
`frSch_gen`: a member is the generated scheme at `e := con n`. -/
def frSch (result errors n : Nat) : Scheme :=
  ⟨1, [], .arr (.R .cnil (.con n)) (.app (.app (.con result) (.app (.con errors) (.con n))) (.var 0))
    (.var 0) .cnil, .cnil⟩

theorem frSch_gen (result errors n : Nat) :
    frSch result errors n = ⟨1, [], (PreludeSchemes.from_result result errors).ty.subst
      (fun i => if i = 0 then .con n else .var (i - 1)), .cnil⟩ := rfl

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
  fromResult : Nat → Nat
  /-- `flat_map : ∀m c d a b e. Monad m ⇒ bindTy m c d a b e` (`Prelude.fl`:
  `∀c d a b. (∀v. a^v → (m b)^(c + v))^c → ((m a)^d → (m b)^(c + d))^c`) -/
  hFlatMap : P.methods flatMap = some (monad, PreludeSchemes.flat_map monad)
  /-- `map2 : ∀f c d a b x g d'. Applicative f ⇒ map2Ty f c d a b x g d'` (`Prelude.fl`:
  `∀c d a b x. (∀u v. a^u → (b^v → x)^(c + u))^c →
  ((f a)^d → ((f b)^d → (f x)^(c + d))^(c + d))^c`) -/
  hMap2 : P.methods map2 = some (applicative, PreludeSchemes.map2 applicative)
  /-- `pure : ∀f c a. Applicative f ⇒ a →P f c a` (`∀c a. a^c → (f a)^c`) -/
  hPure : P.methods pure = some (applicative, PreludeSchemes.pure applicative)
  /-- `nothing : ∀f a r. Alternative f ⇒ f r a` (`∀a. f a`) -/
  hNothing : P.methods nothing = some (alternative, PreludeSchemes.nothing alternative)
  /-- `iter : ∀f d a. Iterable f ⇒ f d a →P Seq (d ∪ a) a` (`∀d a. (f a)^d → (Seq a)^d`) -/
  hIter : P.methods iter = some (iterable, PreludeSchemes.iter iterable seq)
  /-- `pair : ∀a b. a →P b →P (a, b)`, a constant of the model: no Prelude name (`docs/formal/elaboration.md#the-loop-rule-l-was-missing`,
  README simplification 9) -/
  hPair : P.consts pair = some ⟨2, [], arrP (.var 0) (arrP (.var 1)
    (.app (.app (.con tup) (.var 0)) (.var 1))), .cnil⟩
  /-- `unc n`, a constant of the model: no Prelude name (`docs/formal/elaboration.md#the-loop-rule-l-was-missing`, README simplification 9) -/
  hUnc : ∀ n, 0 < n → P.consts (unc n) = some (uncSch tup n)
  hNone : P.consts noneC = some ⟨1, [], optTy (.var 0), .cnil⟩
  /-- `Seq.flat_map : ∀c d a b e. bindTy Seq c d a b e` (`Seq.fl`) -/
  hSeqFlatMap : P.consts seqFlatMap = some (PreludeSchemes.Seq.flat_map seq)
  /-- `singleton : ∀a r. a →P Seq r a` (`a → Seq a`) -/
  hSingleton : P.consts singleton = some (PreludeSchemes.Seq.singleton seq)
  /-- `empty : ∀a r. Seq r a` (`Seq a`) -/
  hEmpty : P.consts empty = some (PreludeSchemes.Seq.empty seq)
  /-- `to_list : ∀d a. Seq d a →P List a` (`∀d a. (Seq a)^d → List a`) -/
  hToList : P.consts toList = some (PreludeSchemes.Seq.to_list seq list)
  /-- `each_φ` (`eachSch`) -/
  hEach : ∀ φ, P.consts (each φ) = some (eachSch unit seq φ)
  hUnitV : P.consts unitV = some ⟨0, [], .con unit, .cnil⟩
  /-- `from_result_n : ∀a. Result (Errors (con n)) a →R a` (`core.md` §1.4), the generated
  scheme at a closed row (`frSch_gen`, `docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`) -/
  hFromResult : ∀ n, P.consts (fromResult n) = some (frSch result errors n)

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

/-- `G⟦c̄ ; e⟧` of §3.2, with `iter` on each source (`docs/formal/elaboration.md#g-elaborated-the-for-source-without-iter`). -/
def Sugar.gen : List Clause → Exp → Exp
  | [], e => .app (.const S.singleton) e
  | .gen s :: cs, e =>
    .app (.app (.const S.seqFlatMap) (.lam (Sugar.gen cs e))) (.app (.meth S.iter) s)
  | .guard c :: cs, e => .ite c (Sugar.gen cs e) (.const S.empty)
  | .let_ x :: cs, e => .let_ x (Sugar.gen cs e)

/-- `L⟦c̄ ; e⟧` of §3.2 at the stage `φ` (`docs/formal/elaboration.md#the-loop-rule-l-was-missing`). -/
def Sugar.loop (φ : Stage) : List Clause → Exp → Exp
  | [], e => e
  | .gen s :: cs, e =>
    .app (.app (.const (S.each φ)) (.lam (Sugar.loop φ cs e))) (.app (.meth S.iter) s)
  | .guard c :: cs, e => .ite c (Sugar.loop φ cs e) (.const S.unitV)
  | .let_ x :: cs, e => .let_ x (Sugar.loop φ cs e)

/-- `[for c̄ yield e]` -/
def Sugar.forList (cs : List Clause) (e : Exp) : Exp := .app (.const S.toList) (S.gen cs e)

/-- `e?` on a `Result` with the row of errors `con n`, at a runtime stage. -/
def Sugar.tryResult (n : Nat) (e : Exp) : Exp := .app (.const (S.fromResult n)) e

/-- `let y = e? in b` on an `Option`: `match e with None → None | Some y → b`. -/
def Sugar.tryOption (e b : Exp) : Exp := .caseOpt e (.const S.noneC) b

/-- `s; rest` of a block: `let _ : Unit = s in rest`. -/
def Sugar.seqUnit (s rest : Exp) : Exp :=
  .letSig ⟨0, [], .con S.unit, .cnil⟩ s (rest.rename Nat.succ)

end Flick.Elab
