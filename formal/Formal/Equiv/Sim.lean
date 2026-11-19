import Formal.Equiv.Top

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

/-- The locations two runs have in common, the cells only one side has, and the left cells
that one thread allocated for a force at its next step (`O`, by its position: `none` is the main
thread, `some (p, j)` the child `j` of the region `p`). -/
structure World where
  L : Nat → Nat → Kd → Prop
  J₁ : Nat → Prop
  J₂ : Nat → Prop
  O : Nat → Option (Nat × Nat) → Prop

/-- A world that grows. -/
structure World.le (W W' : World) : Prop where
  L : ∀ l l' k, W.L l l' k → W'.L l l' k
  J₁ : ∀ n, W.J₁ n → W'.J₁ n
  J₂ : ∀ n, W.J₂ n → W'.J₂ n
  O : ∀ n τ, W.O n τ → W'.O n τ

theorem World.le.refl (W : World) : W.le W :=
  ⟨fun _ _ _ h => h, fun _ h => h, fun _ h => h, fun _ _ h => h⟩

theorem World.le.trans {W₁ W₂ W₃ : World} (h₁ : W₁.le W₂) (h₂ : W₂.le W₃) : W₁.le W₃ :=
  ⟨fun _ _ _ h => h₂.L _ _ _ (h₁.L _ _ _ h), fun _ h => h₂.J₁ _ (h₁.J₁ _ h),
    fun _ h => h₂.J₂ _ (h₁.J₂ _ h), fun _ _ h => h₂.O _ _ (h₁.O _ _ h)⟩

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

/-- Related terms of two runs. -/
inductive Sim (W : World) : Pos → Tm → Tm → Prop
  | var {x i} : x ≠ .park → Sim W x (.var i) (.var i)
  | lam {x b b'} : Sim W .any b b' → b'.NoIns → Sim W x (.lam b) (.lam b')
  | app {x f f' a a'} : x ≠ .park → Sim W x f f' → Sim W x a a' → Sim W x (.app f a) (.app f' a')
  | appK {f f' a a'} : f.Spine → Sim W .park f f' → Sim W .any a a' →
      Sim W .park (.app f a) (.app f' a')
  | appV {f f' a a'} : Value f → Sim W .top f f' → Sim W .park a a' →
      Sim W .park (.app f a) (.app f' a')
  | let_ {x e e' b b'} : Sim W x.fr e e' → Sim W .any b b' → Sim W x (.let_ e b) (.let_ e' b')
  | letrec {x f f' b b'} : Sim W .any f f' → f'.NoIns → Sim W .any b b' →
      Sim W x (.letrec f b) (.letrec f' b')
  | fix {x f f'} : Sim W .any f f' → f'.NoIns → Sim W x (.fix f) (.fix f')
  | lit {x l} : Sim W x (.lit l) (.lit l)
  | loc {x l l' k} : W.L l l' k → Sim W x (.loc l) (.loc l')
  | const {x c} : Sim W x (.const c) (.const c)
  | ctor {x K} : Sim W x (.ctor K) (.ctor K)
  | rnil {x} : Sim W x .rnil .rnil
  | rcons {x l e e' r r'} : x ≠ .park → Sim W x e e' → Sim W x r r' →
      Sim W x (.rcons l e r) (.rcons l e' r')
  | rconsK {l e e' r r'} : e.Spine → Sim W .park e e' → Sim W .any r r' →
      Sim W .park (.rcons l e r) (.rcons l e' r')
  | rconsV {l e e' r r'} : Value e → Sim W .top e e' → Sim W .park r r' →
      Sim W .park (.rcons l e r) (.rcons l e' r')
  | proj {x e e' l} : Sim W x.fr e e' → Sim W x (.proj e l) (.proj e' l)
  | update {x e e' l a a'} : x ≠ .park → Sim W .any e e' → Sim W .any a a' →
      Sim W x (.update e l a) (.update e' l a')
  | updK {e e' l a a'} : e.Spine → Sim W .park e e' → Sim W .any a a' →
      Sim W .park (.update e l a) (.update e' l a')
  | updV {e e' l a a'} : Value e → Sim W .top e e' → Sim W .park a a' →
      Sim W .park (.update e l a) (.update e' l a')
  | match_ {x e e' al al'} : Sim W x.fr e e' → Sim W .top al al' →
      Sim W x (.match_ e al) (.match_ e' al')
  | anil {x} : Sim W x .anil .anil
  | acons {x p b b' r r'} : Sim W .any b b' → Sim W x r r' →
      Sim W x (.acons p b r) (.acons p b' r')
  | aguard {x p g g' b b' r r'} : Sim W .any g g' → Sim W .any b b' → Sim W x r r' →
      Sim W x (.aguard p g b r) (.aguard p g' b' r')
  /-- An arm no run takes: a body makes a lazy cell where a pattern reads (`Tm.LzTop`), which
  typing keeps out of every arm a run takes; the bodies are not compared. -/
  | aconsD {x p b b' r r'} : (b.LzTop .any ∨ b'.LzTop .any) → b'.NoIns → Sim W x r r' →
      Sim W x (.acons p b r) (.acons p b' r')
  | todo {x} : Sim W x .todo .todo
  | task {x e e'} : Sim W .any e e' → e'.NoIns → Sim W x (.task e) (.task e')
  | await {x e e'} : Sim W x.fr e e' → Sim W x (.await e) (.await e')
  | lazy {x e e'} : Sim W .any e e' → Sim W x (.lazy e) (.lazy e')
  | force {x e e'} : Sim W x.fr e e' → Sim W x (.force e) (.force e')
  | raise {x e e'} : Sim W x.fr e e' → Sim W x (.raise e) (.raise e')
  | try_ {x e e' al al'} : Sim W x.fr e e' → Sim W .top al al' →
      Sim W x (.try_ e al) (.try_ e' al')
  | handle {x v v' al al'} : Sim W .top v v' → Sim W .top al al' →
      Sim W x (.handle v al) (.handle v' al')
  | scope {x e e'} : Sim W .any e e' → Sim W x (.scope e) (.scope e')
  | inside {x p e e'} : Sim W x.fr e e' → Sim W x (.inside p e) (.inside p e')
  | cap {x p} : Sim W x (.cap p) (.cap p)
  | own {x p} : Sim W x (.own p) (.own p)
  | drec {x c r r'} : Sim W .top r r' → r'.NoIns → Sim W x (.drec c r) (.drec c r')
  | glob {x g} : Sim W x (.glob g) (.glob g)
  | hdl {x E al al' r r' e e'} : Sim W .top al al' → Sim W .any r r' → Sim W .any e e' →
      Sim W x (.hdl E al r e) (.hdl E al' r' e')
  | hwith {x p ℓ ℓ' k E al al' r r' e e'} : W.L ℓ ℓ' k → Sim W .top al al' → Sim W .any r r' →
      Sim W x.fr e e' → Sim W x (.hwith p ℓ E al r e) (.hwith p ℓ' E al' r' e')
  | hkeep {x q ℓ ℓ' k E al al' e e'} : W.L ℓ ℓ' k → Sim W .top al al' → Sim W x.fr e e' →
      Sim W x (.hkeep q ℓ E al e) (.hkeep q ℓ' E al' e')
  | hkarm {x q ℓ ℓ' k E al al' a a' c c'} : W.L ℓ ℓ' k → Sim W .top al al' → Sim W x.fr a a' →
      Sim W .park c c' → Sim W x (.hkarm q ℓ E al a c) (.hkarm q ℓ' E al' a' c')
  | hctl {x p ℓ ℓ' k κ κ' tl E al al' r r' a a' c c'} : W.L ℓ ℓ' k → KapR W tl κ κ' →
      Sim W .top al al' → Sim W .any r r' → Sim W x.fr a a' → Sim W .park c c' →
      Sim W x (.hctl p ℓ κ tl E al r a c) (.hctl p ℓ' κ' tl E al' r' a' c')
  | hres {x p ℓ ℓ' k a a' b b'} : W.L ℓ ℓ' k → Sim W .park a a' → Sim W x.fr b b' →
      Sim W x (.hres p ℓ a b) (.hres p ℓ' a' b')
  | hdone {x p a a'} : Sim W x.fr a a' → Sim W x (.hdone p a) (.hdone p a')
  | unw {x p ℓ ℓ' k a a'} : W.L ℓ ℓ' k → Sim W .any a a' → Sim W x (.unw p ℓ a) (.unw p ℓ' a')
  | kont {x p ℓ ℓ' k} : W.L ℓ ℓ' k → Sim W x (.kont p ℓ) (.kont p ℓ')
  | hole {x p} : Sim W x (.hole p) (.hole p)
  | settle {x ℓ ℓ' e e'} : W.L ℓ ℓ' .sym → Sim W x.fr e e' → Sim W x (.settle ℓ e) (.settle ℓ' e')
  /-- O1a: `let l = lazy e in c` with `l` unused, and `c`. -/
  | dropA {e b c c'} : b = c.rename (· + 1) → Sim W .any c c' → Sim W .any (.let_ (.lazy e) b) c'
  /-- O1a once its cell, on the left only, is allocated. -/
  | dropB {n b c c'} : W.J₁ n → b = c.rename (· + 1) → Sim W .any c c' →
      Sim W .any (.let_ (.loc n) b) c'
  /-- O1a the other way round. -/
  | dropC {e c c' b'} : b' = c'.rename (· + 1) → c'.NoIns → Sim W .any c c' →
      Sim W .any c (.let_ (.lazy e) b')
  /-- O1c: `force (lazy e)` and `e`. -/
  | forceA {e e'} : Sim W .any e e' → Sim W .any (.force (.lazy e)) e'
  /-- O1c the other way round. -/
  | forceC {e e'} : e'.NoIns → Sim W .any e e' → Sim W .any e (.force (.lazy e'))
  /-- The frame of a cell on the left only. -/
  | jL {x n a a'} : x ≠ .top → W.J₁ n → Sim W x.fr a a' → Sim W x (.settle n a) a'
  /-- The frame of a cell on the right only. -/
  | jR {x n a a'} : x ≠ .top → W.J₂ n → Sim W x.fr a a' → Sim W x a (.settle n a')
  /-- O1b: `lazy v` and `born v'`. -/
  | bornA {v v'} : Value v → Value v' → Sim W .any v v' →
      Sim W .any (.lazy v) (.let_ (.lazy v') (.let_ (.force (.var 0)) (.var 1)))
  /-- O1b the other way round. -/
  | bornB {v v'} : Value v → Value v' → Sim W .any v v' →
      Sim W .any (.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))) (.lazy v')
  /-- The run of `born v` against the allocated `lazy v'`. -/
  | bornS {l l' v v' t} : W.L l l' (.born v v') → v.Closed → BornSt l v t →
      Sim W .any t (.loc l')
  /-- The left cell of a `born` pair settles; the right one is done. -/
  | bornV {l l' v v'} : W.L l l' (.born v v') → v.Closed → v'.Closed → v'.NoIns →
      Sim W .any (.settle l v) v'
  /-- A left thread that forced a running cell of a `born` pair: it is stuck, the right one is
  not. -/
  | dead {b} : b.NoIns → Sim W .any .todo b

end Flick.Equiv
