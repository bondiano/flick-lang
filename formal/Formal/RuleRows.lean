import Formal.Typing

/-!
# Rows of the typing rules (regions.md §2.7)

Errors raised by a value, what a handler gives its body, and the obligations
the end of a region discharges.
-/

namespace Flick

/-- The row of errors of `raise e` with `e : E ῡ`: `{E ῡ}` and `⌈E ῡ⌉`, what the element
`rnm E U` stands for. -/
def Row.err (E : Name) (U : Ty) : Row := Row.union (Row.inst E U.classOf) (Row.ofCaps U.reach)

/-- What a handler of `E` answering the arguments `A` (`Sig.answers`) lets its body do with the
row `r` outside: `E` at those arguments, every other name of `r`, and the variables of `r`. A
variable may give `E` back, standing without fewer names inside than outside (`Row.back`),
only when the handler answers every argument; otherwise it stands without `E` inside, so no
instance of `E` the handler cannot answer comes in through it. -/
def Row.hBody (E : Name) (A : TyClass → Prop) (r : Row) : Row
  | .nm N c => (N = E ∧ A c) ∨ (N ≠ E ∧ r (.nm N c))
  | .rv a G' => ∃ G, r (.rv a G) ∧ (∀ n, G' n → G n ∨ n = E ∧ ¬ ∀ c, A c) ∧
      (∀ n, G n → G' n ∨ n = E ∧ ∀ c, A c) ∧ ((¬ ∀ c, A c) → G' E)
  | x => r x

/-- What a handler of `E` of the region `q` gives its body: as `Row.hBody`, and it discharges
an obligation `Provided {E} ϱ` when `q` outlives `ϱ`, at every argument, as it answers by name
whatever operation of `E` reaches it. -/
def Row.hP (Θ : Facts) (q : Nat) (E : Name) (A : TyClass → Prop) (r : Row) : Row
  | .ob N c a => (N = E → Outl Θ (· = .r q) a.ty) ∧ (N ≠ E → r (.ob N c a))
  | x => Row.hBody E A r x

/-- No obligation of the stage `φ₀` of the body of a handler is about an effect it may
give (regions.md §2.7, first case, rejected): a named obligation needs `o` without row
variables and without its effect, one over a row variable needs `o` without effects. -/
def Stage.ObOut (φ₀ : Stage) (o : Row) : Prop :=
  (∀ N c a, φ₀.svc (.ob N c a) → o.Named ∧ ∀ c', ¬ o (.nm N c')) ∧
  ∀ v G a, φ₀.svc (.obv v G a) → o.Named ∧ ∀ N c, ¬ o (.nm N c)

/-- The obligations `Provided {N} a` that the end of the region `a` discharges: `N` comes
from outside the region, whose handler outlives it (regions.md §2.7, second case); and so
does `Provided v a` for a row variable `v` of the row outside. -/
def Row.disch (a : Atom) (r : Row) : Row
  | .ob N c b => b = a ∧ r (.nm N c)
  | .obv v G b => b = a ∧ r (.rv v G)
  | _ => False

theorem Stage.le_rt_sub {φ : Stage} {r r' ε : Row} (h : φ.le (.rt r ε)) (hr : Row.Sub r r') :
    φ.le (.rt r' ε) := by
  cases φ with
  | pure => trivial
  | rt r₀ e₀ => exact ⟨fun n hn => hr n (h.1 n hn), h.2⟩

theorem Stage.le_of_svc {φ : Stage} {r r' ε : Row} (h : φ.le (.rt r ε)) (hr : ∀ x, φ.svc x → r' x) :
    φ.le (.rt r' ε) := by
  cases φ with
  | pure => trivial
  | rt r₀ e₀ => exact ⟨hr, h.2⟩

end Flick
