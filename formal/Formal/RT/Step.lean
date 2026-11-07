import Formal.RT.Local

/-!
# λRT: steps (runtime.md §3)

`Step K L C C'` is one step of §3.1: a step of one fiber chosen freely, a step of a closing
(run by the fiber that ran the body), or an external event (a Promise settles, a request to
stop arrives).
-/

namespace Flick.RT

variable {T V : Type}

/-- The steps of §3. -/
inductive Step (K : Core T V) (L : Lib V) : Conf T V → Conf T V → Prop
  | fiber {C f φ φ'} : C.Φ f = some (.run φ) → Local K L C φ φ' →
      Step K L C { C with
        Φ := setF C f φ' }
  /-- a fiber ends -/
  | done {C f φ w} : C.Φ f = some (.run φ) → φ.code = .out w → φ.stack = [] →
      (φ.role = .watcher → False) →
      Step K L C { C with
        Φ := upd C.Φ f (some (.done w)), J := .done f w :: C.J }
  /-- a watcher ends, its supervisor returned -/
  | doneW {C f φ v} : C.Φ f = some (.run φ) → φ.role = .watcher → φ.code = .out (.ok v) →
      φ.stack = [] →
      Step K L C { C with
        Φ := upd C.Φ f (some (.done (.ok v))),
        Λ := updO C.Λ φ.base fun o => { o with W := o.W.erase f },
        J := .done f (.ok v) :: C.J }
  /-- a watcher ends: its supervisor failed, `D += c_g` -/
  | doneWF {C f φ c} : C.Φ f = some (.run φ) → φ.role = .watcher → φ.code = .out (.fail c) →
      φ.stack = [] →
      Step K L C { C with
        Φ := upd C.Φ f (some (.done (.fail c))),
        Λ := updO C.Λ φ.base fun o => { o with W := o.W.erase f },
        D := c :: C.D, J := .done f (.fail c) :: .defect c :: C.J }
  | make {C f φ e e' v} : C.Φ f = some (.run φ) → φ.code = .tm e → K.step e (.make v) e' →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok (K.ofId C.n))) },
        μ := upd C.μ C.n (some v), J := .make C.n v :: C.J, n := C.n + 1 }
  /-- (§3.7) `Ref.modify` in one step -/
  | modify {C f φ e e' l g v} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.modify l g) e' → C.μ l = some v →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok (g v).1)) },
        μ := upd C.μ l (some (g v).2), J := .modify l v (g v).1 (g v).2 :: C.J }
  | modWrite {C f φ l g v} : C.Φ f = some (.run φ) → φ.code = .modw l g v → L.refAtomic = false →
      Step K L C { C with
        Φ := setF C f { φ with code := .out (.ok (g v).1) },
        μ := upd C.μ l (some (g v).2), J := .modify l v (g v).1 (g v).2 :: C.J }
  /-- (Spawn) -/
  | spawn {C f φ e e' π g t o} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.spawn π g t) e' → C.Λ π = some o → o.st = .open →
      Step K L C { C with
        Φ := upd (upd (setF C f { φ with code := .tm (K.plug e' (.ok (K.ofId C.n))) })
               C.n (some (.run ⟨.tm t, [], none, π, φ.env, φ.map, .child⟩)))
               (C.n + 1) (some (.run ⟨.watch π C.n g, [], none, π, φ.env, φ.map, .watcher⟩)),
        Λ := upd C.Λ π (some { o with K := C.n :: o.K, W := (C.n + 1) :: o.W }),
        J := .spawn π C.n (C.n + 1) :: C.J, n := C.n + 2 }
  /-- (Spawn-Closed) -/
  | spawnClosed {C f φ e e' π g t o} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.spawn π g t) e' → C.Λ π = some o → o.st ≠ .open →
      Step K L C { C with
        Φ := setF C f
          { φ with code := .tm (K.plug e' (.fail [.die (K.ofId π)])) } }
  /-- (Fin-Add) -/
  | addFin {C f φ e e' π t o} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.addFin π t) e' → C.Λ π = some o → o.st.live = true →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok K.unit)) },
        Λ := upd C.Λ π (some { o with F := ⟨C.n, .code t⟩ :: o.F }),
        J := .fin π C.n :: .reg π C.n :: C.J, n := C.n + 1 }
  /-- (Fin-Now) -/
  | addNow {C f φ e e' π t o} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.addFin π t) e' → C.Λ π = some o → o.st.live = false → L.lateRuns = true →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug t o.x),
                                                stack := .then (.ok K.unit) :: .resume e' :: φ.stack },
        J := .now π C.n :: .reg π C.n :: C.J, n := C.n + 1 }
  /-- without G7(a): a finalizer added to a closed area is dropped -/
  | addDrop {C f φ e e' π t o} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.addFin π t) e' → C.Λ π = some o → o.st.live = false → L.lateRuns = false →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok K.unit)) },
        J := .reg π C.n :: C.J, n := C.n + 1 }
  /-- (Acquire): `acquire_release` returned `v`; `rel v` goes to `F(π)` -/
  | acqLive {C f φ v π rel s o} : C.Φ f = some (.run φ) → φ.code = .out (.ok v) →
      φ.stack = .acq π rel :: s → C.Λ π = some o → o.st.live = true →
      Step K L C { C with
        Φ := setF C f { φ with stack := s },
        Λ := upd C.Λ π (some { o with F := ⟨C.n, .code (K.plug rel (.ok v))⟩ :: o.F }),
        J := .fin π C.n :: .reg π C.n :: C.J, n := C.n + 1 }
  | acqNow {C f φ v π rel s o} : C.Φ f = some (.run φ) → φ.code = .out (.ok v) →
      φ.stack = .acq π rel :: s → C.Λ π = some o → o.st.live = false → L.lateRuns = true →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug (K.plug rel (.ok v)) o.x),
                                                stack := .then (.ok v) :: s },
        J := .now π C.n :: .reg π C.n :: C.J, n := C.n + 1 }
  | acqDrop {C f φ v π rel s o} : C.Φ f = some (.run φ) → φ.code = .out (.ok v) →
      φ.stack = .acq π rel :: s → C.Λ π = some o → o.st.live = false → L.lateRuns = false →
      Step K L C { C with
        Φ := setF C f { φ with stack := s },
        J := .reg π C.n :: C.J, n := C.n + 1 }
  /-- without G7(b): interrupted before the registration -/
  | acqLost {C f φ v π rel s g} : C.Φ f = some (.run φ) → φ.code = .out (.ok v) →
      φ.stack = .acq π rel :: s → φ.q = some g → L.acqAtomic = false →
      Step K L C { C with
        Φ := setF C f { φ with code := .out (.fail [.int (some g)]), stack := s },
        J := .reg π C.n :: C.J, n := C.n + 1 }
  /-- (Scope) -/
  | scope {C f φ e e' b} : C.Φ f = some (.run φ) → φ.code = .tm e → K.step e (.scope b) e' →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug b (.ok (K.ofId C.n))),
                               stack := .inside C.n none :: .resume e' :: φ.stack },
        Λ := upd C.Λ C.n (some (Owner.fresh (.ok K.unit) f)), n := C.n + 1 }
  /-- (C0) -/
  | close {C f φ w π env s o} : C.Φ f = some (.run φ) → φ.code = .out w →
      φ.stack = .inside π env :: s → C.Λ π = some o → o.st = .open →
      Step K L C { C with
        Φ := setF C f { φ with code := .idle, stack := .closing π :: s },
        Λ := upd C.Λ π (some { o with st := .closing, x := w, ph := .hooks false }),
        J := .close π :: C.J }
  /-- (C1) a hook that is code -/
  | hookCode {C f φ π s o b t H} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .hooks b → o.H = .code t :: H →
      o.st = .closing →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug t (.ok K.unit)) },
        Λ := upd C.Λ π (some { o with H := H }) }
  /-- (C1) a group's hook closes the group -/
  | hookStop {C f φ π s o b γ H g} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .hooks b → o.H = .stop γ :: H →
      C.G γ = some g → g.owner = π →
      o.st = .closing →
      Step K L C { C with
        Λ := upd C.Λ π (some { o with H := H, ph := .stop γ b }),
        G := upd C.G γ (some { g with isOpen := false }) }
  | stopDone {C f φ π s o b γ g} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .stop γ b → C.G γ = some g →
      (∀ k ∈ g.kids, Done C.Φ k) →
      o.st = .closing →
      Step K L C { C with
        Λ := upd C.Λ π (some { o with ph := .hooks b }) }
  /-- (C2) -/
  | kids {C f φ π s o} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .hooks false → o.H = [] →
      o.st = .closing →
      Step K L C { C with
        Λ := upd C.Λ π (some { o with ph := .kids }) }
  /-- (C3) -/
  | kidsDone {C f φ π s o} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .kids → o.K = [] → o.W = [] →
      o.st = .closing →
      Step K L C { C with
        Λ := upd C.Λ π (some { o with ph := .hooks true }) }
  /-- (C4) is the guard `P = ∅`; (C5) begins -/
  | fins {C f φ π s o} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .hooks true → o.H = [] →
      o.P = [] → o.K = [] → o.W = [] →
      o.st = .closing →
      Step K L C { C with
        Λ := upd C.Λ π (some { o with st := .finalizing, ph := .fins, F := [],
                                                      fs := L.closeOrder o.F }),
        J := .fins π :: C.J }
  /-- (C5) a finalizer that is code -/
  | finCode {C f φ π s o i t fs} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .fins → o.fs = ⟨i, .code t⟩ :: fs →
      o.st = .finalizing →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug t o.x) },
        Λ := upd C.Λ π (some { o with fs := fs }), J := .run π i :: C.J }
  /-- (C5) the observer of a layer, not the last one -/
  | finObs {C f φ π s o i m Ly fs mm e} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .fins →
      o.fs = ⟨i, .obs m Ly⟩ :: fs → C.M m = some mm → mm.ents Ly = some e → e.obs.erase i ≠ [] →
      o.st = .finalizing →
      Step K L C { C with
        Λ := upd C.Λ π (some { o with fs := fs }),
        M := setEnt C.M m Ly (some { e with obs := e.obs.erase i }), J := .run π i :: C.J }
  /-- (C5) the last observer closes the layer's area `σ` -/
  | finObsLast {C f φ π s o i m Ly fs mm e oσ} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .fins →
      o.fs = ⟨i, .obs m Ly⟩ :: fs → C.M m = some mm → mm.ents Ly = some e → e.obs.erase i = [] →
      e.σ ≠ π → C.Λ e.σ = some oσ → oσ.st = .open →
      o.st = .finalizing → e.st = .built →
      Step K L C { C with
        Φ := setF C f { φ with stack := .closing e.σ :: φ.stack },
        Λ := upd (upd C.Λ π (some { o with fs := fs })) e.σ
               (some { oσ with st := .closing, x := .ok K.unit, ph := .hooks false }),
        M := setEnt C.M m Ly none,
        J := .close e.σ :: .release m Ly e.σ :: .run π i :: C.J }
  /-- (C6) -/
  | closed {C f φ π s o} : C.Φ f = some (.run φ) → φ.code = .idle →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .fins → o.fs = [] →
      o.st = .finalizing →
      Step K L C { C with
        Φ := setF C f { φ with code := .out (closeResult L o.x o.cH o.cF),
                                                stack := s },
        Λ := upd C.Λ π (some { o with st := .closed }), J := .closed π :: C.J }
  /-- a hook ended: `c_H := c_H ⊕ cause` -/
  | absorbH {C f φ π s o w b} : C.Φ f = some (.run φ) → φ.code = .out w →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .hooks b →
      o.st = .closing →
      Step K L C { C with
        Φ := setF C f { φ with code := .idle },
        Λ := upd C.Λ π (some { o with cH := L.combine o.cH w.cause }) }
  /-- a finalizer ended: `c_F := c_F ⧺ cause` -/
  | absorbF {C f φ π s o w} : C.Φ f = some (.run φ) → φ.code = .out w →
      φ.stack = .closing π :: s → C.Λ π = some o → o.ph = .fins →
      o.st = .finalizing →
      Step K L C { C with
        Φ := setF C f { φ with code := .idle },
        Λ := upd C.Λ π (some { o with cF := L.concat o.cF w.cause }) }
  /-- (Int-Req): `f` waits for `k` -/
  | interrupt {C f φ e e' k} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.interrupt k) e' →
      Step K L C { C with
        Φ := setF C f { φ with code := .waitFib k, stack := .resume e' :: φ.stack } }
  /-- A request to stop reaches `k` for `f` (Int-Req, C2, a group's hook, `parallel`); it is
  sticky and may arrive at any time: that it is sent is liveness. -/
  | req {C k ψ f} : C.Φ k = some (.run ψ) →
      Step K L C { C with Φ := setF C k { ψ with q := ψ.q <|> some f } }
  /-- (Ext) -/
  | ext {C f φ e e' o} : C.Φ f = some (.run φ) → φ.code = .tm e → K.step e .ext e' →
      C.Λ φ.cur = some o → L.extTracked = true →
      Step K L C { C with
        Φ := setF C f { φ with code := .waitExt C.n, stack := .resume e' :: φ.stack },
        Λ := upd C.Λ φ.cur (some { o with P := C.n :: o.P }),
        X := upd C.X C.n (some φ.cur), J := .ext φ.cur C.n :: C.J, n := C.n + 1 }
  /-- without G9: the call is not tracked by `P` -/
  | extU {C f φ e e'} : C.Φ f = some (.run φ) → φ.code = .tm e → K.step e .ext e' →
      L.extTracked = false →
      Step K L C { C with
        Φ := setF C f { φ with code := .waitExt C.n, stack := .resume e' :: φ.stack },
        X := upd C.X C.n (some φ.cur), J := .ext φ.cur C.n :: C.J, n := C.n + 1 }
  /-- (Ext-Settle), an external event -/
  | settle {C p π} : C.X p = some π →
      Step K L C { C with
        X := upd C.X p none,
        Λ := updO C.Λ π fun o => { o with P := o.P.erase p }, J := .settle p :: C.J }
  /-- (Watch) -/
  | watchOk {C f φ π k g v} : C.Φ f = some (.run φ) → φ.code = .watch π k g →
      C.Φ k = some (.done (.ok v)) →
      Step K L C { C with
        Φ := setF C f { φ with code := .out (.ok K.unit) },
        Λ := updO C.Λ π fun o => { o with K := o.K.erase k } }
  | watchFail {C f φ π k g c} : C.Φ f = some (.run φ) → φ.code = .watch π k g →
      C.Φ k = some (.done (.fail c)) →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug g (.fail c)) },
        Λ := updO C.Λ π (fun o => { o with K := o.K.erase k }), J := .sup k c :: C.J }
  /-- `FiberSet.make` in a live region: its hook goes to `H(π)` -/
  | group {C f φ e e' π o} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.group π) e' → C.Λ π = some o → o.st.live = true →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok (K.ofId C.n))) },
        Λ := upd C.Λ π (some { o with H := o.H ++ [.stop C.n] }),
        G := upd C.G C.n (some ⟨π, true, []⟩), n := C.n + 1 }
  /-- `FiberSet.make` elsewhere: born closed -/
  | groupClosed {C f φ e e' π} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.group π) e' → (∀ o, C.Λ π = some o → o.st.live = false) →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok (K.ofId C.n))) },
        G := upd C.G C.n (some ⟨π, false, []⟩), n := C.n + 1 }
  | grun {C f φ e e' γ t g} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.grun γ t) e' → C.G γ = some g → g.isOpen = true →
      Step K L C { C with
        Φ := upd (setF C f { φ with code := .tm (K.plug e' (.ok (K.ofId C.n))) }) C.n
               (some (.run ⟨.tm t, [], none, g.owner, φ.env, φ.map, .gkid γ⟩)),
        G := upd C.G γ (some { g with kids := C.n :: g.kids }),
        J := .gkid g.owner C.n :: C.J, n := C.n + 1 }
  /-- `FiberSet.run` on a closed group: born interrupted -/
  | grunClosed {C f φ e e' γ t g} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.grun γ t) e' → C.G γ = some g → g.isOpen = false →
      Step K L C { C with
        Φ := upd (setF C f { φ with code := .tm (K.plug e' (.ok (K.ofId C.n))) }) C.n
               (some (.done (.fail [.int none]))),
        J := .done C.n (.fail [.int none]) :: C.J, n := C.n + 1 }
  /-- (Par): one more branch, acquiring in `π` -/
  | parStart {C f φ t ts ks s} : C.Φ f = some (.run φ) → φ.code = .parC (t :: ts) →
      φ.stack = .par ks :: s →
      Step K L C { C with
        Φ := upd (setF C f { φ with code := .parC ts, stack := .par (C.n :: ks) :: s }) C.n
               (some (.run ⟨.tm t, [], none, φ.cur, φ.env, φ.map, .branch f⟩)),
        J := .branch φ.cur C.n :: C.J, n := C.n + 1 }
  /-- (Provide): owner `πₚ`, a map forked from the fiber's -/
  | provide {C f φ e e' Ls b} : C.Φ f = some (.run φ) → φ.code = .tm e →
      K.step e (.provide Ls b) e' →
      Step K L C { C with
        Φ := setF C f { φ with code := .provC C.n (C.n + 1) Ls b,
                               stack := .inside C.n (some (C.n + 1)) :: .resume e' :: φ.stack },
        Λ := upd C.Λ C.n (some (Owner.fresh (.ok K.unit) f)),
        M := upd C.M (C.n + 1) (some ⟨φ.map, fun _ => none⟩), n := C.n + 2 }
  /-- `build`, `L ∈ m`: one more observer in `F(πₚ)` -/
  | provHit {C f φ πp m Ly c Ls b m' e o mm} : C.Φ f = some (.run φ) →
      φ.code = .provC πp m ((Ly, c) :: Ls) b → L.memo = true →
      lookup C.M Ly C.n m = some (m', e) → e.st = .built → C.Λ πp = some o → o.st = .open →
      C.M m' = some mm →
      Step K L C { C with
        Φ := setF C f { φ with code := .provC πp m Ls b },
        Λ := upd C.Λ πp (some { o with F := ⟨C.n, .obs m' Ly⟩ :: o.F }),
        M := setEnt C.M m' Ly (some { e with obs := C.n :: e.obs }),
        J := .fin πp C.n :: .obs πp m' Ly e.σ C.n :: C.J, n := C.n + 1 }
  /-- `build`, `L ∉ m`: build it in a new area `σ` -/
  | provBuild {C f φ πp m Ly c Ls b} : C.Φ f = some (.run φ) →
      φ.code = .provC πp m ((Ly, c) :: Ls) b → (L.memo = true → lookup C.M Ly C.n m = none) →
      (C.M m).isSome →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm c, stack := .build πp m Ly C.n Ls b :: φ.stack },
        Λ := upd C.Λ C.n (some (Owner.fresh (.ok K.unit) f)),
        M := setEnt C.M m Ly (some ⟨.building, C.n, []⟩),
        J := .build m Ly C.n :: C.J, n := C.n + 1 }
  | buildOk {C f φ πp m Ly σ Ls b s v o mm e} : C.Φ f = some (.run φ) → φ.code = .out (.ok v) →
      φ.stack = .build πp m Ly σ Ls b :: s → C.Λ πp = some o → o.st = .open →
      C.M m = some mm → mm.ents Ly = some e →
      Step K L C { C with
        Φ := setF C f { φ with code := .provC πp m Ls b, stack := s },
        Λ := upd C.Λ πp (some { o with F := ⟨C.n, .obs m Ly⟩ :: o.F }),
        M := setEnt C.M m Ly (some ⟨.built, σ, [C.n]⟩),
        J := .fin πp C.n :: .obs πp m Ly σ C.n :: C.J, n := C.n + 1 }
  /-- the build failed: close `σ`; the provide fails after it -/
  | buildFail {C f φ πp m Ly σ Ls b s c oσ} : C.Φ f = some (.run φ) → φ.code = .out (.fail c) →
      φ.stack = .build πp m Ly σ Ls b :: s → C.Λ σ = some oσ → oσ.st = .open →
      Step K L C { C with
        Φ := setF C f { φ with code := .idle, stack := .closing σ :: s },
        Λ := upd C.Λ σ (some { oσ with st := .closing, x := .fail c, ph := .hooks false }),
        M := setEnt C.M m Ly (some ⟨.failed c, σ, []⟩), J := .close σ :: C.J }
  /-- an operation answered by `πₚ` of `θ` -/
  | use {C f φ e e' π} : C.Φ f = some (.run φ) → φ.code = .tm e → K.step e (.use π) e' →
      π ∈ φ.env →
      Step K L C { C with
        Φ := setF C f { φ with code := .tm (K.plug e' (.ok K.unit)) },
        J := .use π :: C.J }

end Flick.RT
