import Formal.Syntax

/-!
# Renaming and parallel substitution on de Bruijn terms
-/

namespace Flick

/-- Lift a renaming under `n` binders. -/
def liftR (n : Nat) (ξ : Nat → Nat) (i : Nat) : Nat :=
  if i < n then i else ξ (i - n) + n

def Tm.rename (ξ : Nat → Nat) : Tm → Tm
  | .var i => .var (ξ i)
  | .lam b => .lam (b.rename (liftR 1 ξ))
  | .app f a => .app (f.rename ξ) (a.rename ξ)
  | .let_ e b => .let_ (e.rename ξ) (b.rename (liftR 1 ξ))
  | .letrec f b => .letrec (f.rename (liftR 2 ξ)) (b.rename (liftR 1 ξ))
  | .fix f => .fix (f.rename (liftR 2 ξ))
  | .lit l => .lit l
  | .loc l => .loc l
  | .const c => .const c
  | .ctor K => .ctor K
  | .rnil => .rnil
  | .rcons l e r => .rcons l (e.rename ξ) (r.rename ξ)
  | .proj e l => .proj (e.rename ξ) l
  | .update e l e' => .update (e.rename ξ) l (e'.rename ξ)
  | .match_ e al => .match_ (e.rename ξ) (al.rename ξ)
  | .anil => .anil
  | .acons p b r => .acons p (b.rename (liftR p.binds ξ)) (r.rename ξ)
  | .aguard p g b r =>
    .aguard p (g.rename (liftR p.binds ξ)) (b.rename (liftR p.binds ξ)) (r.rename ξ)
  | .todo => .todo
  | .task e => .task (e.rename ξ)
  | .await e => .await (e.rename ξ)
  | .raise e => .raise (e.rename ξ)
  | .try_ e al => .try_ (e.rename ξ) (al.rename ξ)
  | .handle v al => .handle (v.rename ξ) (al.rename ξ)
  | .provide l e => .provide (l.rename ξ) (e.rename ξ)
  | .with_ q θ e => .with_ q (θ.rename ξ) (e.rename ξ)
  | .enil => .enil
  | .econs S v θ => .econs S (v.rename ξ) (θ.rename ξ)
  | .bind S e => .bind S (e.rename ξ)
  | .join e₁ e₂ => .join (e₁.rename ξ) (e₂.rename ξ)
  | .scope e => .scope (e.rename (liftR 1 ξ))
  | .inside p e => .inside p (e.rename ξ)
  | .cap p => .cap p
  | .own p => .own p

/-- Lift a substitution under `n` binders. -/
def liftS (n : Nat) (σ : Nat → Tm) (i : Nat) : Tm :=
  if i < n then .var i else (σ (i - n)).rename (· + n)

def Tm.subst (σ : Nat → Tm) : Tm → Tm
  | .var i => σ i
  | .lam b => .lam (b.subst (liftS 1 σ))
  | .app f a => .app (f.subst σ) (a.subst σ)
  | .let_ e b => .let_ (e.subst σ) (b.subst (liftS 1 σ))
  | .letrec f b => .letrec (f.subst (liftS 2 σ)) (b.subst (liftS 1 σ))
  | .fix f => .fix (f.subst (liftS 2 σ))
  | .lit l => .lit l
  | .loc l => .loc l
  | .const c => .const c
  | .ctor K => .ctor K
  | .rnil => .rnil
  | .rcons l e r => .rcons l (e.subst σ) (r.subst σ)
  | .proj e l => .proj (e.subst σ) l
  | .update e l e' => .update (e.subst σ) l (e'.subst σ)
  | .match_ e al => .match_ (e.subst σ) (al.subst σ)
  | .anil => .anil
  | .acons p b r => .acons p (b.subst (liftS p.binds σ)) (r.subst σ)
  | .aguard p g b r =>
    .aguard p (g.subst (liftS p.binds σ)) (b.subst (liftS p.binds σ)) (r.subst σ)
  | .todo => .todo
  | .task e => .task (e.subst σ)
  | .await e => .await (e.subst σ)
  | .raise e => .raise (e.subst σ)
  | .try_ e al => .try_ (e.subst σ) (al.subst σ)
  | .handle v al => .handle (v.subst σ) (al.subst σ)
  | .provide l e => .provide (l.subst σ) (e.subst σ)
  | .with_ q θ e => .with_ q (θ.subst σ) (e.subst σ)
  | .enil => .enil
  | .econs S v θ => .econs S (v.subst σ) (θ.subst σ)
  | .bind S e => .bind S (e.subst σ)
  | .join e₁ e₂ => .join (e₁.subst σ) (e₂.subst σ)
  | .scope e => .scope (e.subst (liftS 1 σ))
  | .inside p e => .inside p (e.subst σ)
  | .cap p => .cap p
  | .own p => .own p

/-- Substitute `vs` for the innermost `vs.length` variables: `e[δ]`. -/
def inst (vs : List Tm) (i : Nat) : Tm :=
  if h : i < vs.length then vs[i] else .var (i - vs.length)

end Flick
