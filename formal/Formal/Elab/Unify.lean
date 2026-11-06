import Formal.Elab.Syntax

/-!
# Elaboration: the refinement of an arm on a constructor (Martelli–Montanari)

An arm on a constructor `K` of a GADT unifies the arguments of the type `K` builds with those
of the matched type. `Unify E θ` is a derivation of the most general unifier `θ` of the
equations `E` by the rules of Martelli and Montanari, `Clash E` one that `E` has none. They
are relations: the checker computes them, and the soundness in the core is by induction on the
derivation. An arrow is taken apart only when its stage and its qualifier are the same on both
sides, as the checker does: the core compares rows and captures as sets.
-/

namespace Flick.Elab

/-- Equations between types. -/
abbrev Eqs := List (Ty × Ty)

def Eqs.subst (σ : Nat → Ty) (E : Eqs) : Eqs := E.map fun p => (p.1.subst σ, p.2.subst σ)

/-- `[x := s]`. -/
def bindV (x : Nat) (s : Ty) : Nat → Ty := fun n => if n = x then s else .var n

/-- `x` occurs in `t` outside the rows and the qualifiers: then `t` is larger than `x`
under every substitution. -/
def Ty.OccS (x : Nat) : Ty → Prop
  | .var n => n = x
  | .app t u => t.OccS x ∨ u.OccS x
  | .arr _ a b _ => a.OccS x ∨ b.OccS x
  | _ => False

/-- Two types whose heads differ under every substitution. -/
def Ty.Apart : Ty → Ty → Prop
  | .con c, .con c' => c ≠ c'
  | .con _, .app _ _ => True
  | .app _ _, .con _ => True
  | .con _, .arr _ _ _ _ => True
  | .arr _ _ _ _, .con _ => True
  | .app _ _, .arr _ _ _ _ => True
  | .arr _ _ _ _, .app _ _ => True
  | _, _ => False

/-- `θ` is the most general unifier of `E`. -/
inductive Unify : Eqs → (Nat → Ty) → Prop
  | nil : Unify [] .var
  | delete {t E θ} : Unify E θ → Unify ((t, t) :: E) θ
  | swap {t u E θ} : Unify ((u, t) :: E) θ → Unify ((t, u) :: E) θ
  | app {t u t' u' E θ} : Unify ((t, t') :: (u, u') :: E) θ → Unify ((.app t u, .app t' u') :: E) θ
  | arr {φ q a b a' b' E θ} : Unify ((a, a') :: (b, b') :: E) θ →
      Unify ((.arr φ a b q, .arr φ a' b' q) :: E) θ
  | bind {x s E θ} : ¬ s.Occurs x → Unify (Eqs.subst (bindV x s) E) θ →
      Unify ((.var x, s) :: E) fun n => (bindV x s n).subst θ

/-- `E` has no unifier. -/
inductive Clash : Eqs → Prop
  | apart {t u E} : t.Apart u → Clash ((t, u) :: E)
  | occurs {x s E} : s ≠ .var x → s.OccS x → Clash ((.var x, s) :: E)
  | delete {t E} : Clash E → Clash ((t, t) :: E)
  | swap {t u E} : Clash ((u, t) :: E) → Clash ((t, u) :: E)
  | app {t u t' u' E} : Clash ((t, t') :: (u, u') :: E) → Clash ((.app t u, .app t' u') :: E)
  | arr {φ q a b a' b' E} : Clash ((a, a') :: (b, b') :: E) →
      Clash ((.arr φ a b q, .arr φ a' b' q) :: E)
  | bind {x s E} : ¬ s.Occurs x → Clash (Eqs.subst (bindV x s) E) → Clash ((.var x, s) :: E)

end Flick.Elab
