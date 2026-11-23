import Formal.Equiv.World

/-!
# Contextual equivalence: related terms of two runs (`Sim`)
-/

namespace Flick.Equiv

open Flick.Elab (All2)

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
  | bornA {v v'} : W.B → Value v → Value v' → Sim W .any v v' →
      Sim W .any (.lazy v) (.let_ (.lazy v') (.let_ (.force (.var 0)) (.var 1)))
  /-- O1b the other way round. -/
  | bornB {v v'} : W.B → Value v → Value v' → Sim W .any v v' →
      Sim W .any (.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))) (.lazy v')
  /-- The run of `born v` against the allocated `lazy v'`. -/
  | bornS {l l' v v' t} : W.B → W.L l l' (.born v v') → v.Closed → BornSt l v t →
      Sim W .any t (.loc l')
  /-- The left cell of a `born` pair settles; the right one is done. -/
  | bornV {l l' v v'} : W.B → W.L l l' (.born v v') → v.Closed → v'.Closed → v'.NoIns →
      Sim W .any (.settle l v) v'
  /-- A left thread that forced a running cell of a `born` or hoisted pair: it is stuck, the right
  one is not. -/
  | dead {b} : W.B → b.NoIns → Sim W .any .todo b
  /-- O4: two hoisted pure cells related many-to-one or one-to-many (`W.H`). -/
  | hloc {x l l'} : W.B → W.H l l' → Sim W x (.loc l) (.loc l')
  /-- O4: unhoisted pure lazy inside `fix` against hoisted `let l = lazy e' in fix`. -/
  | o4A {e e' b b' f br} : W.B → f = .let_ (.lazy (e.rename (· + 2))) b → br = b'.rename rot →
      PureOpen W.R e → PureOpen W.R e' → Sim W .any e e' → Sim W .any b b' →
      e'.NoIns → b'.NoIns →
      Sim W .any (.fix f) (.let_ (.lazy e') (.fix br))
  /-- O4 the other way round. -/
  | o4B {e e' b b' br f'} : W.B → br = b.rename rot → f' = .let_ (.lazy (e'.rename (· + 2))) b' →
      PureOpen W.R e → PureOpen W.R e' → Sim W .any e e' → Sim W .any b b' →
      e'.NoIns → b'.NoIns →
      Sim W .any (.let_ (.lazy e) (.fix br)) (.fix f')
  /-- O4 forward, once the hoisted right cell `l'` is allocated. -/
  | o4Fix1 {x l' e e' b b' f f'} : W.B → W.HR l' e' →
      f = .let_ (.lazy (e.rename (· + 2))) b → f' = b'.subst (inst [.loc l']) →
      e.Closed → PureEval W.R e → Sim W .any e e' → Sim W .any b b' → b'.NoIns →
      Sim W x (.fix f) (.fix f')
  /-- O4 reverse, once the hoisted left cell `l` is allocated. -/
  | o4Fix2 {x l e e' b b' f f'} : W.B → W.HL l e →
      f = b.subst (inst [.loc l]) → f' = .let_ (.lazy (e'.rename (· + 2))) b' →
      e'.Closed → PureEval W.R e' → Sim W .any e e' → Sim W .any b b' → e'.NoIns → b'.NoIns →
      Sim W x (.fix f) (.fix f')
  /-- O4 forward: per-iteration allocation on the left against the shared cell `l'` on the right. -/
  | o4Alloc1 {l' e e' c c' s'} : W.B → W.HR l' e' → s' = c'.subst (inst [.loc l']) →
      e.Closed → PureEval W.R e → Sim W .any e e' → Sim W .any c c' → c'.NoIns →
      Sim W .any (.let_ (.lazy e) c) s'
  /-- O4 reverse: the shared left cell `l` against per-iteration allocation on the right. -/
  | o4AllocR {l e e' s c c'} : W.B → W.HL l e → s = c.subst (inst [.loc l]) →
      e'.Closed → PureEval W.R e' → Sim W .any e e' → Sim W .any c c' →
      Sim W .any s (.let_ (.lazy e') c')
  /-- O4: binding a freshly allocated left hoisted cell `l`. -/
  | o4Bind {l c c'} : W.B → Sim W .any (c.subst (inst [.loc l])) c' →
      Sim W .any (.let_ (.loc l) c) c'
  /-- O4: evaluating a pure hoisted initializer inside `settle l` while the right side waits at
  `force l'`. -/
  | o4Settle {l l' e t} : W.B → W.H l l' → W.HL l e → e.Closed → t.Closed → PSteps W.R e t →
      Sim W .any (.settle l t) (.force (.loc l'))
  /-- O2a forward: forcing a cell known to be done with `v` against `v'`. -/
  | doneF {l l' : Nat} {v v' : Tm} : ¬ W.B → W.D l l' v v' → Value v' → v'.Closed → v'.NoIns →
      Sim W .any (.force (.loc l)) v'
  /-- O2a reverse: `v` against forcing a cell known to be done with `v'`. -/
  | doneR {l l' : Nat} {v v' : Tm} : ¬ W.B → W.D l l' v v' → Value v → v.Closed →
      Sim W .any v (.force (.loc l'))
  /-- O2a forward, state 1: `let y = force e in C[force e]` against `let y = force e' in C[y]`. -/
  | againF1 {c₀ e e' : Tm} {σ σ' : Nat → Tm} : ¬ W.B → c₀.Src →
      (∀ y, y ≠ .park → Sim W y e e') →
      ((∃ j, e = .var j) ∨ Value e) → ((∃ j, e' = .var j) ∨ Value e') →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Sim W .any (.let_ (.force e) (c₀.subst (subForce e σ)))
        (.let_ (.force e') (c₀.subst (subVar σ')))
  /-- O2a forward, state 2: `let y = settle l t in C[force l]` against
  `let y = settle l' t' in C[y]`. Allowed at `park` when parked by an outer handler. -/
  | againF2 {x : Pos} {l l' : Nat} {t t' c₀ : Tm} {σ σ' : Nat → Tm} : x ≠ .top → ¬ W.B →
      W.L l l' .sym → Sim W x.fr t t' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Sim W x (.let_ (.settle l t) (c₀.subst (subForce (.loc l) σ)))
        (.let_ (.settle l' t') (c₀.subst (subVar σ')))
  /-- O2a forward, state 3: `let y = v in C[force l]` against `let y = v' in C[y]` with cell done. -/
  | againF3 {l l' : Nat} {v v' c₀ : Tm} {σ σ' : Nat → Tm} : ¬ W.B → W.D l l' v v' →
      Value v → Value v' → v.Closed → v'.Closed → v'.NoIns → Sim W .any v v' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Sim W .any (.let_ v (c₀.subst (subForce (.loc l) σ)))
        (.let_ v' (c₀.subst (subVar σ')))
  /-- O2a reverse, state 1: `let y = force e in C[y]` against `let y = force e' in C[force e']`. -/
  | againR1 {c₀ e e' : Tm} {σ σ' : Nat → Tm} : ¬ W.B → c₀.Src →
      (∀ y, y ≠ .park → Sim W y e e') →
      ((∃ j, e = .var j) ∨ Value e) → ((∃ j, e' = .var j) ∨ Value e') →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Sim W .any (.let_ (.force e) (c₀.subst (subVar σ)))
        (.let_ (.force e') (c₀.subst (subForce e' σ')))
  /-- O2a reverse, state 2: `let y = settle l t in C[y]` against
  `let y = settle l' t' in C[force l']`. Allowed at `park` when parked by an outer handler. -/
  | againR2 {x : Pos} {l l' : Nat} {t t' c₀ : Tm} {σ σ' : Nat → Tm} : x ≠ .top → ¬ W.B →
      W.L l l' .sym → Sim W x.fr t t' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Sim W x (.let_ (.settle l t) (c₀.subst (subVar σ)))
        (.let_ (.settle l' t') (c₀.subst (subForce (.loc l') σ')))
  /-- O2a reverse, state 3: `let y = v in C[y]` against `let y = v' in C[force l']` with cell done. -/
  | againR3 {l l' : Nat} {v v' c₀ : Tm} {σ σ' : Nat → Tm} : ¬ W.B → W.D l l' v v' →
      Value v → Value v' → v.Closed → v'.Closed → v'.NoIns → Sim W .any v v' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Sim W .any (.let_ v (c₀.subst (subVar σ)))
        (.let_ v' (c₀.subst (subForce (.loc l') σ')))
  /-- O2a: dead `let` whose scrutinee raised an error (bodies are never entered). -/
  | letRaise {v v' b₁ b₂ : Tm} : ¬ W.B → Value v → Value v' → Sim W .any v v' →
      Sim W .any (.let_ (.raise v) b₁) (.let_ (.raise v') b₂)
  /-- O2a: dead `let` whose scrutinee is unwinding (bodies are never entered). -/
  | letUnw {p ℓ ℓ' : Nat} {k : Kd} {a a' b₁ b₂ : Tm} : ¬ W.B → W.L ℓ ℓ' k → Sim W .any a a' →
      Sim W .any (.let_ (.unw p ℓ a) b₁) (.let_ (.unw p ℓ' a') b₂)
  /-- O2a: dead `let` whose scrutinee hit a defect (bodies are never entered). -/
  | letTodo {b₁ b₂ : Tm} : ¬ W.B → Sim W .any (.let_ .todo b₁) (.let_ .todo b₂)

end Flick.Equiv
