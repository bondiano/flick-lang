import Formal.Typing

/-!
# Rows of the typing rules (regions.md §2.7)

Errors raised by a value, what a handler gives its body, and the obligations
the end of a region discharges.
-/

namespace Flick

/-- The row of errors of `raise e` with `e : E ῡ`: `{E}` and `⌈E ῡ⌉`. -/
def Row.err (E : Name) (U : Ty) : Row := Row.union (Row.single E) (Row.ofCaps U.reach)

/-- What a handler of `E` of the region `q` gives its body (regions.md §2.7): the frame
answers `E`, and it discharges an obligation `Provided {E} ϱ` when `q` outlives `ϱ`;
everything else comes from outside. -/
def Row.hP (Θ : Facts) (q : Nat) (E : Name) (r : Row) : Row
  | .nm N => N = E ∨ r (.nm N)
  | .ob N a => (N = E → Outl Θ (· = .r q) a.ty) ∧ (N ≠ E → r (.ob N a))
  | x => r x

/-- No obligation of the stage `φ₀` of the body of a handler is about an effect it may
give (regions.md §2.7, first case, rejected): a named obligation needs `o` without row
variables and without its effect, one over a row variable needs `o` without effects. -/
def Stage.ObOut (φ₀ : Stage) (o : Row) : Prop :=
  (∀ N a, φ₀.svc (.ob N a) → o.Named ∧ ¬ o (.nm N)) ∧
  ∀ v a, φ₀.svc (.obv v a) → o.Named ∧ ∀ N, ¬ o (.nm N)

/-- The obligations `Provided {N} a` that the end of the region `a` discharges: `N` comes
from outside the region, whose handler outlives it (regions.md §2.7, second case); and so
does `Provided v a` for a row variable `v` of the row outside. -/
def Row.disch (a : Atom) (r : Row) : Row
  | .ob N b => b = a ∧ r (.nm N)
  | .obv v b => b = a ∧ r (.rv v)
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
