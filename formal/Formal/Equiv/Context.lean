import Formal.Equiv.TopTyped
import Formal.Equiv.CfgStep
import Formal.Soundness

/-!
# Contextual equivalence of terms of the core

Two terms are contextually equivalent (`CtxEquiv`) when every two typed programs one context
builds around them, a term of the source language with any number of holes anywhere (`Ctx`),
may end with the same literals (`Ends`). A filling stays under the binders of the context, which
bind its free variables.

The rewrites of O1 make or force a lazy cell at their top (`Tm.LzShape`). Where a pattern reads a
term without running it they are no pair of `Sim` (`Pos.top`), but typing keeps them away from
there (`Typ.noLzTop`): such a place has the pure stage, or no term judgment, while a term that
makes a cell has the stage `rt` at least. Typing reaches every place but the body of a dead arm of
a GADT; an arm whose body has such a term is related whatever the bodies (`Sim.aconsD`), and no
run takes it, as what a run reaches is typed.

Two terms related by `Sim` both ways in every world are contextually equivalent
(`ctxEquiv_of_sim`): the context relates the two programs (`Ctx.sim`), typing makes their threads
closed and keeps lazy cells away from patterns wherever their runs go (`Prog.reach`), and the
simulation of runs answers a left run that ends with a literal by a right one that ends with it
(`csteps_sim`, `lit_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

/-- Two programs with holes: one context of the source language, with `e₁` and `e₂` in its
holes, which may stand anywhere. -/
inductive Ctx (e₁ e₂ : Tm) : Tm → Tm → Prop
  | hole : Ctx e₁ e₂ e₁ e₂
  | var {i} : Ctx e₁ e₂ (.var i) (.var i)
  | lit {l} : Ctx e₁ e₂ (.lit l) (.lit l)
  | const {c} : Ctx e₁ e₂ (.const c) (.const c)
  | ctor {K} : Ctx e₁ e₂ (.ctor K) (.ctor K)
  | rnil : Ctx e₁ e₂ .rnil .rnil
  | anil : Ctx e₁ e₂ .anil .anil
  | todo : Ctx e₁ e₂ .todo .todo
  | glob {g} : Ctx e₁ e₂ (.glob g) (.glob g)
  | lam {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.lam b) (.lam b')
  | fix {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.fix b) (.fix b')
  | task {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.task b) (.task b')
  | await {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.await b) (.await b')
  | lazy {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.lazy b) (.lazy b')
  | force {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.force b) (.force b')
  | raise {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.raise b) (.raise b')
  | scope {b b'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.scope b) (.scope b')
  | proj {b b' l} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.proj b l) (.proj b' l)
  | drec {c r r'} : Ctx e₁ e₂ r r' → Ctx e₁ e₂ (.drec c r) (.drec c r')
  | app {f f' a a'} : Ctx e₁ e₂ f f' → Ctx e₁ e₂ a a' → Ctx e₁ e₂ (.app f a) (.app f' a')
  | let_ {a a' b b'} : Ctx e₁ e₂ a a' → Ctx e₁ e₂ b b' → Ctx e₁ e₂ (.let_ a b) (.let_ a' b')
  | letrec {a a' b b'} : Ctx e₁ e₂ a a' → Ctx e₁ e₂ b b' →
      Ctx e₁ e₂ (.letrec a b) (.letrec a' b')
  | rcons {l a a' r r'} : Ctx e₁ e₂ a a' → Ctx e₁ e₂ r r' →
      Ctx e₁ e₂ (.rcons l a r) (.rcons l a' r')
  | update {l a a' b b'} : Ctx e₁ e₂ a a' → Ctx e₁ e₂ b b' →
      Ctx e₁ e₂ (.update a l b) (.update a' l b')
  | match_ {a a' al al'} : Ctx e₁ e₂ a a' → Ctx e₁ e₂ al al' →
      Ctx e₁ e₂ (.match_ a al) (.match_ a' al')
  | acons {p b b' r r'} : Ctx e₁ e₂ b b' → Ctx e₁ e₂ r r' →
      Ctx e₁ e₂ (.acons p b r) (.acons p b' r')
  | aguard {p g g' b b' r r'} : Ctx e₁ e₂ g g' → Ctx e₁ e₂ b b' → Ctx e₁ e₂ r r' →
      Ctx e₁ e₂ (.aguard p g b r) (.aguard p g' b' r')
  | try_ {a a' al al'} : Ctx e₁ e₂ a a' → Ctx e₁ e₂ al al' →
      Ctx e₁ e₂ (.try_ a al) (.try_ a' al')
  | handle {v v' al al'} : Ctx e₁ e₂ v v' → Ctx e₁ e₂ al al' →
      Ctx e₁ e₂ (.handle v al) (.handle v' al')
  | hdl {E al al' r r' b b'} : Ctx e₁ e₂ al al' → Ctx e₁ e₂ r r' → Ctx e₁ e₂ b b' →
      Ctx e₁ e₂ (.hdl E al r b) (.hdl E al' r' b')

variable {e₁ e₂ : Tm}

theorem Ctx.symm {C₁ C₂ : Tm} (h : Ctx e₁ e₂ C₁ C₂) : Ctx e₂ e₁ C₂ C₁ := by
  induction h <;> constructor <;> assumption

theorem Ctx.src {C₁ C₂ : Tm} (h : Ctx e₁ e₂ C₁ C₂) (he : e₂.Src) : C₂.Src := by
  induction h <;> simp_all [Tm.Src]

theorem Pos.fr_any {x : Pos} (hx : x ≠ .park) : x.fr = .any := by
  cases x <;> simp_all

/-- **The context relates the two programs**: where a pattern reads, a hole would put a lazy cell
there, so it stands only where a term runs or is bound, or in a dead arm. -/
theorem Ctx.sim {W : World} (h₀ : Sim W .any e₁ e₂) (hz : e₁.LzShape ∨ e₂.LzShape)
    (he : e₂.Src) {C₁ C₂ : Tm} (h : Ctx e₁ e₂ C₁ C₂) :
    ∀ {x : Pos}, x ≠ .park → ¬ C₁.LzTop x → ¬ C₂.LzTop x → Sim W x C₁ C₂ := by
  have any : Pos.any ≠ .park := by decide
  have top : Pos.top ≠ .park := by decide
  induction h with
  | hole =>
    intro x hx h₁ h₂
    cases x
    · exact h₀
    · exact (hz.elim (fun hs => h₁ (.here hs)) (fun hs => h₂ (.here hs))).elim
    · exact (hx rfl).elim
  | var => exact fun hx _ _ => .var hx
  | lit => exact fun _ _ _ => .lit
  | const => exact fun _ _ _ => .const
  | ctor => exact fun _ _ _ => .ctor
  | rnil => exact fun _ _ _ => .rnil
  | anil => exact fun _ _ _ => .anil
  | todo => exact fun _ _ _ => .todo
  | glob => exact fun _ _ _ => .glob
  | lam h ih => exact fun _ h₁ h₂ => .lam (ih any (h₁ ∘ .lam) (h₂ ∘ .lam)) (h.src he).noIns
  | fix h ih => exact fun _ h₁ h₂ => .fix (ih any (h₁ ∘ .fix) (h₂ ∘ .fix)) (h.src he).noIns
  | task h ih => exact fun _ h₁ h₂ => .task (ih any (h₁ ∘ .task) (h₂ ∘ .task)) (h.src he).noIns
  | await _ ih =>
    exact fun hx h₁ h₂ => .await ((Pos.fr_any hx).symm ▸ ih any (h₁ ∘ .await) (h₂ ∘ .await))
  | lazy _ ih => exact fun _ h₁ h₂ => .lazy (ih any (h₁ ∘ .lazy) (h₂ ∘ .lazy))
  | force _ ih =>
    exact fun hx h₁ h₂ => .force ((Pos.fr_any hx).symm ▸ ih any (h₁ ∘ .force) (h₂ ∘ .force))
  | raise _ ih =>
    exact fun hx h₁ h₂ => .raise ((Pos.fr_any hx).symm ▸ ih any (h₁ ∘ .raise) (h₂ ∘ .raise))
  | scope _ ih => exact fun _ h₁ h₂ => .scope (ih any (h₁ ∘ .scope) (h₂ ∘ .scope))
  | proj _ ih =>
    exact fun hx h₁ h₂ => .proj ((Pos.fr_any hx).symm ▸ ih any (h₁ ∘ .proj) (h₂ ∘ .proj))
  | drec h ih => exact fun _ h₁ h₂ => .drec (ih top (h₁ ∘ .drec) (h₂ ∘ .drec)) (h.src he).noIns
  | app _ _ ih₁ ih₂ =>
    exact fun hx h₁ h₂ => .app hx (ih₁ hx (h₁ ∘ .appL) (h₂ ∘ .appL)) (ih₂ hx (h₁ ∘ .appR) (h₂ ∘ .appR))
  | let_ _ _ ih₁ ih₂ =>
    exact fun hx h₁ h₂ => .let_ ((Pos.fr_any hx).symm ▸ ih₁ any (h₁ ∘ .letL) (h₂ ∘ .letL))
      (ih₂ any (h₁ ∘ .letR) (h₂ ∘ .letR))
  | letrec h₁ _ ih₁ ih₂ =>
    exact fun _ k₁ k₂ => .letrec (ih₁ any (k₁ ∘ .letrecL) (k₂ ∘ .letrecL)) (h₁.src he).noIns
      (ih₂ any (k₁ ∘ .letrecR) (k₂ ∘ .letrecR))
  | rcons _ _ ih₁ ih₂ =>
    exact fun hx h₁ h₂ => .rcons hx (ih₁ hx (h₁ ∘ .rconsL) (h₂ ∘ .rconsL))
      (ih₂ hx (h₁ ∘ .rconsR) (h₂ ∘ .rconsR))
  | update _ _ ih₁ ih₂ =>
    exact fun hx h₁ h₂ => .update hx (ih₁ any (h₁ ∘ .updL) (h₂ ∘ .updL))
      (ih₂ any (h₁ ∘ .updR) (h₂ ∘ .updR))
  | match_ _ _ ih₁ ih₂ =>
    exact fun hx h₁ h₂ => .match_ ((Pos.fr_any hx).symm ▸ ih₁ any (h₁ ∘ .matchL) (h₂ ∘ .matchL))
      (ih₂ top (h₁ ∘ .matchR) (h₂ ∘ .matchR))
  | @acons p b b' r r' hb _ ihb ihr =>
    intro x hx h₁ h₂
    by_cases hd : b.LzTop .any ∨ b'.LzTop .any
    · exact .aconsD hd (hb.src he).noIns (ihr hx (h₁ ∘ .acons) (h₂ ∘ .acons))
    · exact .acons (ihb any (fun h => hd (.inl h)) (fun h => hd (.inr h)))
        (ihr hx (h₁ ∘ .acons) (h₂ ∘ .acons))
  | aguard _ _ _ ih₁ ih₂ ih₃ =>
    exact fun hx h₁ h₂ => .aguard (ih₁ any (h₁ ∘ .aguardG) (h₂ ∘ .aguardG))
      (ih₂ any (h₁ ∘ .aguardB) (h₂ ∘ .aguardB)) (ih₃ hx (h₁ ∘ .aguardR) (h₂ ∘ .aguardR))
  | try_ _ _ ih₁ ih₂ =>
    exact fun hx h₁ h₂ => .try_ ((Pos.fr_any hx).symm ▸ ih₁ any (h₁ ∘ .tryL) (h₂ ∘ .tryL))
      (ih₂ top (h₁ ∘ .tryR) (h₂ ∘ .tryR))
  | handle _ _ ih₁ ih₂ =>
    exact fun _ h₁ h₂ => .handle (ih₁ top (h₁ ∘ .handleV) (h₂ ∘ .handleV))
      (ih₂ top (h₁ ∘ .handleA) (h₂ ∘ .handleA))
  | hdl _ _ _ ih₁ ih₂ ih₃ =>
    exact fun _ h₁ h₂ => .hdl (ih₁ top (h₁ ∘ .hdlA) (h₂ ∘ .hdlA)) (ih₂ any (h₁ ∘ .hdlR) (h₂ ∘ .hdlR))
      (ih₃ any (h₁ ∘ .hdlE) (h₂ ∘ .hdlE))

/-- The world of two runs that have not started; `b`: the pairs of O1b and O4 may stand in it. -/
def World.empty (root : Roots) (b : Prop) : World :=
  ⟨root, fun _ _ _ => False, fun _ => False, fun _ => False, fun _ _ => False,
    fun _ _ => False, fun _ _ => False, fun _ _ => False, b⟩

theorem StR.empty (root : Roots) (b : Prop) : StR (World.empty root b) [] [] :=
  ⟨fun _ _ _ h => h.elim, fun _ _ _ _ _ _ h => h.elim, fun _ _ _ _ _ h => h.elim,
    fun _ h => h.elim, fun _ h => h.elim, fun _ _ h => h.elim, fun _ _ _ h => h.elim,
    fun _ _ h => h.elim, fun _ _ h => h.elim, fun _ _ h => h.elim⟩

/-- A whole program: a closed typed term of the core under typed roots of the source language
(T1). -/
def Prog (S : Sig) (root : Roots) (e : Tm) : Prop :=
  RootTy S root ∧ root.Src ∧ e.Below 0 ∧
    ∃ t φ, Typ S [] Facts.none [] e (.ex t φ) ∧ t.YF 0 ∧ φ.substT (kill 0) = φ

/-- The literals a program may end with: its main thread ends with one. -/
def Ends (root : Roots) (e : Tm) (o : Lit) : Prop :=
  ∃ Λ μ, CSteps root ⟨[], [], e⟩ ⟨Λ, μ, .lit o⟩

/-- **Contextual equivalence**: every two typed programs one context builds around `e₁` and
`e₂` may end with the same literals. -/
def CtxEquiv (S : Sig) (e₁ e₂ : Tm) : Prop :=
  ∀ root C₁ C₂, Ctx e₁ e₂ C₁ C₂ → Prog S root C₁ → Prog S root C₂ →
    ∀ o, Ends root C₁ o ↔ Ends root C₂ o

theorem closed_of_typ {S : Sig} {Ψ : List Ty} {Θ : Facts} {e : Tm} {j : J}
    (h : Typ S Ψ Θ [] e j) : e.Closed := fun i hi => by
  have := h.frees i hi
  simp at this

/-- **Typing closes the threads** of every configuration a program reaches, keeps lazy cells
away from where patterns read, guarantees pattern matches bind values, and keeps lazy cells as
lazy cells across steps. -/
theorem Prog.reach {S : Sig} {root : Roots} {e : Tm} (h : Prog S root e) :
    Reach root ⟨[], [], e⟩ := by
  obtain ⟨hroot, -, hb, t, φ, h₀, ht, hφ⟩ := h
  intro c hs
  obtain ⟨X, hcfg⟩ := steps_typed hroot hs ht hφ (CfgTy.init h₀ hb)
  have hμ := hcfg.1
  have hm := hcfg.2.1
  have hΛ := hcfg.2.2.1
  refine ⟨⟨closed_of_typ hm, hm.noLzTop (fun h => nomatch h), hm.matchVals, fun p g hg k hk => ?_⟩,
    fun l c'' hs' i d hd hic => ?_⟩
  · obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
    exact ⟨closed_of_typ hk', hk'.noLzTop (fun h => nomatch h), hk'.matchVals⟩
  · obtain ⟨X', hcfg', -⟩ := cstep_typed hroot ht hφ hcfg hs'
    have hμ' := hcfg'.1
    have hi : i < ([] ++ X).length := hμ.1 ▸ (List.getElem?_eq_some_iff.mp hd).1
    have hty := List.getElem?_eq_getElem hi
    rcases hμ.2 i d _ hd hty with ⟨hv, -⟩ | ⟨r, ε, a, C, hu, -⟩
    · exact (hv.not_cell hic).elim
    · have hi' : i < c''.mem.length := hμ'.1.symm ▸ Nat.lt_of_lt_of_le hi (by simp)
      have hd' := List.getElem?_eq_getElem hi'
      have hty' : (([] ++ X) ++ X')[i]? = some (.lazyCell r ε a C) := by
        rw [List.getElem?_append_left hi, hty, hu]
      rcases hμ'.2 i _ _ hd' hty' with ⟨hv', hvt⟩ | ⟨_, _, _, _, _, hc'⟩
      · exact (hvt.value_not_cell hv').elim
      · exact ⟨_, hd', hc'.isCell⟩

theorem Prog.noLzTop {S : Sig} {root : Roots} {e : Tm} (h : Prog S root e) : ¬ e.LzTop .any :=
  (h.reach _ .refl).1.2.1

/-- A program simulated by another one from the start ends with a literal it ends with. -/
theorem Ends.sim {S : Sig} {root : Roots} {C₁ C₂ : Tm} {o : Lit} {b : Prop}
    (hs : Sim (World.empty root b) .any C₁ C₂) (p₁ : Prog S root C₁) (p₂ : Prog S root C₂)
    (h : Ends root C₁ o) : Ends root C₂ o := by
  obtain ⟨Λ, μ, h⟩ := h
  have hr : CfgR (World.empty root b) ⟨[], [], C₁⟩ ⟨[], [], C₂⟩ :=
    ⟨.inl hs, StR.empty root b, ⟨rfl, fun _ _ _ h => by simp at h⟩⟩
  obtain ⟨W', c₂, -, hs₂, hr'⟩ := csteps_sim p₁.2.1 h hr rfl p₁.reach p₂.reach
  obtain ⟨Λ₂, μ₂, hs₃⟩ := lit_sim (root := root) hr' (fun _ => (p₂.reach _ hs₂).1.1)
  exact ⟨Λ₂, μ₂, hs₂.trans hs₃⟩

/-- **Terms related both ways under typed roots are contextually equivalent**, when one makes or
forces a lazy cell at its top. -/
theorem ctxEquiv_of_sim_root {S : Sig} {b : Prop}
    (h₁₂ : ∀ root, RootTy S root → Sim (World.empty root b) .any e₁ e₂)
    (h₂₁ : ∀ root, RootTy S root → Sim (World.empty root b) .any e₂ e₁)
    (hz : e₁.LzShape ∨ e₂.LzShape) (s₁ : e₁.Src) (s₂ : e₂.Src) : CtxEquiv S e₁ e₂ :=
  fun root _ _ hc p₁ p₂ _ =>
    ⟨Ends.sim (hc.sim (h₁₂ root p₁.1) hz s₂ (by decide) p₁.noLzTop p₂.noLzTop) p₁ p₂,
      Ends.sim (hc.symm.sim (h₂₁ root p₁.1) hz.symm s₁ (by decide) p₂.noLzTop p₁.noLzTop) p₂ p₁⟩

/-- **Terms related both ways are contextually equivalent**, when one makes or forces a lazy cell
at its top. -/
theorem ctxEquiv_of_sim {S : Sig} {b : Prop}
    (h₁₂ : ∀ root, Sim (World.empty root b) .any e₁ e₂)
    (h₂₁ : ∀ root, Sim (World.empty root b) .any e₂ e₁)
    (hz : e₁.LzShape ∨ e₂.LzShape) (s₁ : e₁.Src) (s₂ : e₂.Src) : CtxEquiv S e₁ e₂ :=
  ctxEquiv_of_sim_root (fun root _ => h₁₂ root) (fun root _ => h₂₁ root) hz s₁ s₂

theorem _root_.Flick.Tm.Src.rename : ∀ {e : Tm}, e.Src → ∀ ξ, (e.rename ξ).Src := by
  intro e
  induction e <;> intro h ξ <;> simp_all [Tm.Src, Tm.rename]

/-- **O1a**: a cell nothing names is not made. -/
theorem o1a {S : Sig} {e c : Tm} (he : e.Src) (hc : c.Src) :
    CtxEquiv S (.let_ (.lazy e) (c.rename (· + 1))) c :=
  ctxEquiv_of_sim (b := False) (fun _ => .dropA rfl (Sim.refl hc _ (by decide)))
    (fun _ => .dropC rfl hc.noIns (Sim.refl hc _ (by decide))) (.inl trivial) ⟨he, hc.rename _⟩ hc

/-- **O1b**: a cell of a value is made done. -/
theorem o1b {S : Sig} {v : Tm} (hv : Value v) (hs : v.Src) : CtxEquiv S (.lazy v) (born v) :=
  ctxEquiv_of_sim (b := True) (fun _ => .bornA trivial hv hv (Sim.refl hs _ (by decide)))
    (fun _ => .bornB trivial hv hv (Sim.refl hs _ (by decide))) (.inl trivial) hs
    (by simp [born, Tm.Src, hs])

/-- **O1c**: a cell forced where it is made is not made. -/
theorem o1c {S : Sig} {e : Tm} (he : e.Src) : CtxEquiv S (.force (.lazy e)) e :=
  ctxEquiv_of_sim (b := False) (fun _ => .forceA (Sim.refl he _ (by decide)))
    (fun _ => .forceC he.noIns (Sim.refl he _ (by decide))) (.inl trivial) he he

/-- **O4 (open loop-invariant initializer)**: hoisting a pure lazy cell whose free variables are
loop-invariant out of `fix` preserves all literals a typed program may end with. -/
theorem o4_open {S : Sig} {e b : Tm} (hp : ∀ root, RootTy S root → PureOpen root e)
    (he : e.Src) (hb : b.Src) :
    CtxEquiv S (.fix (.let_ (.lazy (e.rename (· + 2))) b)) (.let_ (.lazy e) (.fix (b.rename rot))) :=
  ctxEquiv_of_sim_root (b := True)
    (fun root hroot => .o4A trivial rfl rfl (hp root hroot) (hp root hroot)
      (Sim.refl he _ (by decide)) (Sim.refl hb _ (by decide)) he.noIns hb.noIns)
    (fun root hroot => .o4B trivial rfl rfl (hp root hroot) (hp root hroot)
      (Sim.refl he _ (by decide)) (Sim.refl hb _ (by decide)) he.noIns hb.noIns)
    (.inr trivial) ⟨he.rename _, hb⟩ ⟨he, hb.rename _⟩

/-- **O4 (closed pure initializer)**: hoisting a pure closed `lazy e` (`· ⊢ e : τ ! P`) out of
`fix` preserves all literals a typed program may end with. -/
theorem o4 {S : Sig} {e b : Tm} {τ : Ty} (ht : Typ S [] Facts.none [] e (.ex τ .pure))
    (he : e.Src) (hb : b.Src) :
    CtxEquiv S (.fix (.let_ (.lazy (e.rename (· + 2))) b)) (.let_ (.lazy e) (.fix (b.rename rot))) :=
  o4_open (fun _ hroot => PureOpen.of_closed (closed_of_typ ht) (ht.pure_eval hroot)) he hb

end Flick.Equiv
