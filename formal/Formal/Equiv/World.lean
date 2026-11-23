import Formal.Equiv.Rot

/-!
# Contextual equivalence: related terms of two runs

The optimizations of `docs/formal/lazy.md#6-optimizations-as-equivalences` are contextual
equivalences of the core: a program and the program rewritten at some of its places end with
the same literals (`Formal/Equiv/Context.lean`). Each is proved with two simulations, one of the
program by the rewritten one and one the other way round. Their runs are related by a world `W`
and the relation `Sim W x a b` of this module.

A world relates the locations the two stores have in common (`L`, a partial bijection with the
kind of the pair of cells) and names the cells only one side has (`J₁` on the left, `J₂` on the
right): no term of the other side names them, so what they hold is never compared.

`Sim W x a b` is a congruence, with locations related by `W`, and the special pairs: a rewrite
(`dropA`, `forceA`, `bornA` and their mirror images `dropC`, `forceC`, `bornB`) and the states
its run passes through. The index `x` is that of `Formal/Elab/CoreCg.lean`: in the spine of a
dictionary record no special pair stands at the top, as a pattern reads there without
evaluating. A value on the right of a congruence holds no run-time form (`Sim.value_noIns`), so
a body parked at a hole is related only to such a body (`Formal/Equiv/Frames.lean`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

/-- How two related cells are related: `sym` cells hold related states and change together; a
`born v v'` pair holds the values `v` and `v'`, the initializers of `lazy v` and of its rewrite
`born v'` (O1b): the left cell is pending, running or done with `v`, the right one pending or
done with `v'`. -/
inductive Kd
  | sym
  | born (v v' : Tm)

/-- The locations two runs have in common, the cells only one side has, the left cells that one
thread allocated for a force at its next step (`O`, by its position: `none` is the main thread,
`some (p, j)` the child `j` of the region `p`), and the pure hoisted cells of O4 (`HL`, `HR`, `H`)
over the program's roots `R`. -/
structure World where
  R : Roots
  L : Nat → Nat → Kd → Prop
  J₁ : Nat → Prop
  J₂ : Nat → Prop
  O : Nat → Option (Nat × Nat) → Prop
  /-- Pure hoisted cells on the left (`HL l e`) and right (`HR l' e'`), and their many-to-one /
  one-to-many correspondence (`H l l'`). Guarded by `B`. -/
  HL : Nat → Tm → Prop
  HR : Nat → Tm → Prop
  H : Nat → Nat → Prop
  /-- Done `sym` cells recorded for O2a (`D l l' v v'`). -/
  D : Nat → Nat → Tm → Tm → Prop
  /-- The pairs of O1b and O4 may stand in the relation (`bornA`, `bornB`, `bornS`, `bornV`,
  `o4*`, `dead`). O1a, O1c and O2a start without them, so their runs never meet a left thread that
  dies where the right one does not. -/
  B : Prop

/-- A world that grows. -/
structure World.le (W W' : World) : Prop where
  R : W.R = W'.R
  L : ∀ l l' k, W.L l l' k → W'.L l l' k
  J₁ : ∀ n, W.J₁ n → W'.J₁ n
  J₂ : ∀ n, W.J₂ n → W'.J₂ n
  O : ∀ n τ, W.O n τ → W'.O n τ
  HL : ∀ l e, W.HL l e → W'.HL l e
  HR : ∀ l' e', W.HR l' e' → W'.HR l' e'
  H : ∀ l l', W.H l l' → W'.H l l'
  D : ∀ l l' v v', W.D l l' v v' → W'.D l l' v v'
  B : W.B ↔ W'.B

theorem World.le.refl (W : World) : W.le W :=
  ⟨rfl, fun _ _ _ h => h, fun _ h => h, fun _ h => h, fun _ _ h => h,
    fun _ _ h => h, fun _ _ h => h, fun _ _ h => h, fun _ _ _ _ h => h, Iff.rfl⟩

theorem World.le.trans {W₁ W₂ W₃ : World} (h₁ : W₁.le W₂) (h₂ : W₂.le W₃) : W₁.le W₃ :=
  ⟨h₁.R.trans h₂.R, fun _ _ _ h => h₂.L _ _ _ (h₁.L _ _ _ h), fun _ h => h₂.J₁ _ (h₁.J₁ _ h),
    fun _ h => h₂.J₂ _ (h₁.J₂ _ h), fun _ _ h => h₂.O _ _ (h₁.O _ _ h),
    fun _ _ h => h₂.HL _ _ (h₁.HL _ _ h), fun _ _ h => h₂.HR _ _ (h₁.HR _ _ h),
    fun _ _ h => h₂.H _ _ (h₁.H _ _ h), fun _ _ _ _ h => h₂.D _ _ _ _ (h₁.D _ _ _ _ h),
    h₁.B.trans h₂.B⟩

/-- Source code: no run-time form. -/
def _root_.Flick.Tm.Src : Tm → Prop
  | .var _ | .lit _ | .const _ | .ctor _ | .rnil | .anil | .todo | .glob _ => True
  | .lam b | .fix b | .task b | .await b | .lazy b | .force b | .raise b | .scope b | .proj b _ =>
    b.Src
  | .drec _ r => r.Src
  | .app f a | .let_ f a | .letrec f a | .rcons _ f a | .update f _ a | .match_ f a
  | .acons _ f a | .try_ f a | .handle f a => f.Src ∧ a.Src
  | .aguard _ g b r | .hdl _ g b r => g.Src ∧ b.Src ∧ r.Src
  | _ => False

/-- Substitution for the O2a body `C[force e]` under the `let y` binder: `0 ↦ force (↑e)`, `1 ↦ y`,
`i + 2 ↦ ↑(σ i)`. -/
def subForce (e : Tm) (σ : Nat → Tm) : Nat → Tm
  | 0 => .force (e.rename (· + 1))
  | 1 => .var 0
  | i + 2 => (σ i).rename (· + 1)

/-- Substitution for the O2a body `C[y]` under the `let y` binder: `0 ↦ y`, `1 ↦ y`,
`i + 2 ↦ ↑(σ i)`. -/
def subVar (σ : Nat → Tm) : Nat → Tm
  | 0 => .var 0
  | 1 => .var 0
  | i + 2 => (σ i).rename (· + 1)

/-- Substitution after `let y = v` fires on the `C[force l]` side. -/
def subDoneF (l : Nat) (v : Tm) (σ : Nat → Tm) : Nat → Tm
  | 0 => .force (.loc l)
  | 1 => v
  | i + 2 => σ i

/-- Substitution after `let y = v` fires on the `C[y]` side. -/
def subDoneV (v : Tm) (σ : Nat → Tm) : Nat → Tm
  | 0 => v
  | 1 => v
  | i + 2 => σ i

/-- `κ` of a `control` arm is a cell, that of a `tail` arm is not used. -/
def KapR (W : World) (tl : Bool) (κ κ' : Nat) : Prop :=
  (tl = true ∧ κ = κ') ∨ (tl = false ∧ ∃ k, W.L κ κ' k)

/-- `born v`: a cell born done with `v`, as the core writes it (O1b): `let l = lazy v in force l;
l`. -/
def born (v : Tm) : Tm := .let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))

/-- The states of `born v` once its cell `l` is allocated; the last one is stuck, as its force met
the cell running. -/
def BornSt (l : Nat) (v t : Tm) : Prop :=
  t = .let_ (.loc l) (.let_ (.force (.var 0)) (.var 1)) ∨ t = .let_ (.force (.loc l)) (.loc l) ∨
    t = .let_ (.settle l v) (.loc l) ∨ t = .let_ v (.loc l) ∨ t = .let_ .todo (.loc l)

/-- A body or an arm parked at its hole: the hole in ordered frames (`Frame.Parks`). -/
def _root_.Flick.Tm.Spine : Tm → Prop
  | .hole _ => True
  | .app f a | .rcons _ f a | .update f _ a => f.Spine ∨ (Value f ∧ a.Spine)
  | .let_ e _ | .match_ e _ | .proj e _ | .await e | .try_ e _ | .raise e | .inside _ e
  | .hwith _ _ _ _ _ e | .hkeep _ _ _ _ e | .hdone _ e | .force e | .settle _ e => e.Spine
  | .hctl _ _ _ _ _ _ _ a _ | .hkarm _ _ _ _ a _ | .hres _ _ _ a => a.Spine
  | _ => False


end Flick.Equiv
